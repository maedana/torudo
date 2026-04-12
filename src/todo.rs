use chrono::NaiveDate;
use log::debug;
use serde::Serialize;
use std::{collections::HashMap, error::Error, fs};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct Item {
    pub completed: bool,
    pub priority: Option<char>,
    pub creation_date: Option<NaiveDate>,
    pub completion_date: Option<NaiveDate>,
    #[serde(rename = "title")]
    pub description: String,
    pub projects: Vec<String>,
    pub contexts: Vec<String>,
    pub id: Option<String>,
    #[serde(skip)]
    pub line_number: usize,
}

impl Item {
    pub fn parse(line: &str, line_number: usize) -> Self {
        let mut parts = line.split_whitespace().peekable();
        let mut item = Self {
            completed: false,
            priority: None,
            creation_date: None,
            completion_date: None,
            description: String::new(),
            projects: Vec::new(),
            contexts: Vec::new(),
            id: None,
            line_number,
        };
        let mut desc_parts = Vec::new();
        if parts.peek() == Some(&"x") {
            item.completed = true;
            parts.next();
            if let Some(date_str) = parts.peek()
                && let Ok(date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
            {
                item.completion_date = Some(date);
                parts.next();
            }
        }
        if let Some(part) = parts.peek()
            && part.len() == 3
            && part.starts_with('(')
            && part.ends_with(')')
            && let Some(c) = part.chars().nth(1)
            && c.is_ascii_uppercase()
        {
            item.priority = Some(c);
            parts.next();
        }
        if let Some(date_str) = parts.peek()
            && let Ok(date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
        {
            item.creation_date = Some(date);
            parts.next();
        }
        for part in parts {
            if let Some(stripped) = part.strip_prefix('+') {
                item.projects.push(stripped.to_string());
            } else if let Some(stripped) = part.strip_prefix('@') {
                item.contexts.push(stripped.to_string());
            } else if let Some(stripped) = part.strip_prefix("id:") {
                item.id = Some(stripped.to_string());
            } else {
                desc_parts.push(part);
            }
        }
        item.description = desc_parts.join(" ");
        item
    }
}

pub fn load_todos(file_path: &str) -> Result<Vec<Item>, Box<dyn Error>> {
    let content = fs::read_to_string(file_path)?;
    let mut todos: Vec<Item> = content
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(line_num, line)| Item::parse(line, line_num + 1))
        .collect();

    // Sort by priority first (A, B, C, None), then by line number
    todos.sort_by(|a, b| {
        // First compare by priority
        match (a.priority, b.priority) {
            (Some(p1), Some(p2)) => p1.cmp(&p2),               // A < B < C
            (Some(_), None) => std::cmp::Ordering::Less,       // Priority items come first
            (None, Some(_)) => std::cmp::Ordering::Greater,    // Non-priority items come last
            (None, None) => a.line_number.cmp(&b.line_number), // Same priority, sort by line number
        }
        .then_with(|| a.line_number.cmp(&b.line_number)) // Secondary sort by line number
    });

    Ok(todos)
}

pub fn add_missing_ids(file_path: &str) -> Result<(), Box<dyn Error>> {
    let content = fs::read_to_string(file_path)?;
    let lines: Vec<&str> = content.lines().collect();
    let mut modified = false;
    let mut new_lines = Vec::new();

    for (line_num, line) in lines.iter().enumerate() {
        if line.trim().is_empty() {
            new_lines.push(line.to_string());
            continue;
        }

        let todo = Item::parse(line, line_num + 1);
        if todo.id.is_none() {
            let new_id = Uuid::new_v4().to_string();
            let new_line = format!("{line} id:{new_id}");
            new_lines.push(new_line);
            modified = true;
        } else {
            new_lines.push(line.to_string());
        }
    }

    if modified {
        let new_content = new_lines.join("\n");
        debug!(
            "Adding missing IDs to {} lines in todo file",
            usize::from(modified)
        );
        fs::write(file_path, new_content)?;
    }

    Ok(())
}

pub fn mark_complete(todo_file: &str, todo_id: &str) -> Result<(), Box<dyn Error>> {
    let content = fs::read_to_string(todo_file)?;
    let lines: Vec<&str> = content.lines().collect();
    let mut new_lines = Vec::new();
    let mut completed_line = None;

    for (line_num, line) in lines.iter().enumerate() {
        if line.trim().is_empty() {
            new_lines.push(line.to_string());
            continue;
        }

        let todo = Item::parse(line, line_num + 1);
        if let Some(id) = &todo.id
            && id == todo_id
        {
            let today = chrono::Local::now().format("%Y-%m-%d").to_string();
            let completed_todo_line = if todo.completed {
                line.to_string()
            } else {
                // Extract priority and reorder: x (A) completion-date rest
                let (priority, rest) =
                    if line.starts_with('(') && line.len() >= 4 && line.chars().nth(2) == Some(')')
                    {
                        (Some(&line[..3]), line[3..].trim_start())
                    } else {
                        (None, *line)
                    };

                priority.map_or_else(
                    || format!("x {today} {line}"),
                    |pri| format!("x {pri} {today} {rest}"),
                )
            };
            completed_line = Some(completed_todo_line);
            continue;
        }
        new_lines.push(line.to_string());
    }

    if let Some(completed_todo) = completed_line {
        let todo_dir = std::path::Path::new(todo_file).parent().unwrap();
        let done_file = todo_dir.join("done.txt");

        debug!("Moving completed todo to done.txt: {completed_todo}");

        let mut done_content = if done_file.exists() {
            fs::read_to_string(&done_file)?
        } else {
            String::new()
        };

        if !done_content.is_empty() && !done_content.ends_with('\n') {
            done_content.push('\n');
        }
        done_content.push_str(&completed_todo);
        done_content.push('\n');

        fs::write(&done_file, done_content)?;

        let new_todo_content = new_lines.join("\n");
        fs::write(todo_file, new_todo_content)?;

        debug!("Successfully moved todo to done.txt and updated todo.txt");
    }

    Ok(())
}

pub fn move_to_file(
    source_file: &str,
    dest_file: &str,
    todo_id: &str,
) -> Result<(), Box<dyn Error>> {
    let content = fs::read_to_string(source_file)?;
    let lines: Vec<&str> = content.lines().collect();
    let mut new_lines = Vec::new();
    let mut moved_line = None;

    for (line_num, line) in lines.iter().enumerate() {
        if line.trim().is_empty() {
            new_lines.push(line.to_string());
            continue;
        }

        let todo = Item::parse(line, line_num + 1);
        if let Some(id) = &todo.id
            && id == todo_id
        {
            moved_line = Some(line.to_string());
            continue;
        }
        new_lines.push(line.to_string());
    }

    if let Some(line) = moved_line {
        append_todo(dest_file, &line)?;

        let new_source_content = new_lines.join("\n");
        fs::write(source_file, new_source_content)?;

        debug!("Moved todo to {dest_file}: {line}");
    }

    Ok(())
}

pub fn has_todo_with_id(file_path: &str, id: &str) -> bool {
    let Ok(content) = fs::read_to_string(file_path) else {
        return false;
    };
    let id_tag = format!("id:{id}");
    content
        .lines()
        .any(|line| line.split_whitespace().any(|word| word == id_tag))
}

pub fn append_todo(file_path: &str, line: &str) -> Result<(), Box<dyn Error>> {
    let mut content = if std::path::Path::new(file_path).exists() {
        fs::read_to_string(file_path)?
    } else {
        String::new()
    };
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(line);
    content.push('\n');
    fs::write(file_path, content)?;
    Ok(())
}

pub fn group_todos_by_project_owned(todos: &[Item]) -> HashMap<String, Vec<Item>> {
    let mut grouped = HashMap::new();
    for todo in todos {
        if todo.projects.is_empty() {
            grouped
                .entry("No Project".to_string())
                .or_insert_with(Vec::new)
                .push(todo.clone());
        } else {
            for project in &todo.projects {
                grouped
                    .entry(project.clone())
                    .or_insert_with(Vec::new)
                    .push(todo.clone());
            }
        }
    }
    grouped
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use std::fs;

    #[test]
    fn test_item_parse_simple_todo() {
        let line = "Buy groceries";
        let item = Item::parse(line, 1);

        assert!(!item.completed);
        assert_eq!(item.priority, None);
        assert_eq!(item.creation_date, None);
        assert_eq!(item.completion_date, None);
        assert_eq!(item.description, "Buy groceries");
        assert!(item.projects.is_empty());
        assert!(item.contexts.is_empty());
        assert_eq!(item.id, None);
    }

    #[test]
    fn test_item_parse_with_priority() {
        let line = "(A) Call Mom";
        let item = Item::parse(line, 1);

        assert!(!item.completed);
        assert_eq!(item.priority, Some('A'));
        assert_eq!(item.description, "Call Mom");
    }

    #[test]
    fn test_item_parse_with_creation_date() {
        let line = "2024-01-15 Review quarterly report";
        let item = Item::parse(line, 1);

        assert!(!item.completed);
        assert_eq!(
            item.creation_date,
            Some(NaiveDate::from_ymd_opt(2024, 1, 15).unwrap())
        );
        assert_eq!(item.description, "Review quarterly report");
    }

    #[test]
    fn test_item_parse_completed_todo() {
        let line = "x 2024-01-20 2024-01-15 Complete project report";
        let item = Item::parse(line, 1);

        assert!(item.completed);
        assert_eq!(
            item.completion_date,
            Some(NaiveDate::from_ymd_opt(2024, 1, 20).unwrap())
        );
        assert_eq!(
            item.creation_date,
            Some(NaiveDate::from_ymd_opt(2024, 1, 15).unwrap())
        );
        assert_eq!(item.priority, None); // Priority is not parsed after completion marker
        assert_eq!(item.description, "Complete project report");
    }

    #[test]
    fn test_item_parse_with_projects_and_contexts() {
        let line = "(C) Buy groceries +personal @errands @shopping";
        let item = Item::parse(line, 1);

        assert_eq!(item.priority, Some('C'));
        assert_eq!(item.description, "Buy groceries");
        assert_eq!(item.projects, vec!["personal"]);
        assert_eq!(item.contexts, vec!["errands", "shopping"]);
    }

    #[test]
    fn test_item_parse_with_id() {
        let line = "Learn Rust programming +learning @coding id:abc123";
        let item = Item::parse(line, 1);

        assert_eq!(item.description, "Learn Rust programming");
        assert_eq!(item.projects, vec!["learning"]);
        assert_eq!(item.contexts, vec!["coding"]);
        assert_eq!(item.id, Some("abc123".to_string()));
    }

    #[test]
    fn test_item_parse_complex_todo() {
        let line = "(A) 2024-01-10 Fix critical bug +work @urgent @coding id:bug-001";
        let item = Item::parse(line, 1);

        assert!(!item.completed);
        assert_eq!(item.priority, Some('A'));
        assert_eq!(
            item.creation_date,
            Some(NaiveDate::from_ymd_opt(2024, 1, 10).unwrap())
        );
        assert_eq!(item.description, "Fix critical bug");
        assert_eq!(item.projects, vec!["work"]);
        assert_eq!(item.contexts, vec!["urgent", "coding"]);
        assert_eq!(item.id, Some("bug-001".to_string()));
    }

    #[test]
    fn test_load_todos_from_content() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_todo.txt");

        let content = r#"(A) Call Mom +family @phone
Buy groceries +personal @errands
x 2024-01-15 (B) Review report +work @office
Learn Rust +learning @coding id:rust-001"#;

        fs::write(&test_file, content).unwrap();

        let todos = load_todos(test_file.to_str().unwrap()).unwrap();

        assert_eq!(todos.len(), 4);

        // Test first todo
        assert_eq!(todos[0].priority, Some('A'));
        assert_eq!(todos[0].description, "Call Mom");
        assert_eq!(todos[0].projects, vec!["family"]);
        assert_eq!(todos[0].contexts, vec!["phone"]);

        // Test completed todo (should be at index 1 after sorting: A, B, then no priority)
        assert!(todos[1].completed);
        assert_eq!(todos[1].priority, Some('B'));
        assert_eq!(todos[1].description, "Review report");

        // Test todo with ID (should be at index 3 after sorting)
        assert_eq!(todos[3].id, Some("rust-001".to_string()));

        fs::remove_file(&test_file).ok();
    }

    #[test]
    fn test_group_todos_by_project() {
        let todos = vec![
            Item {
                completed: false,
                priority: Some('A'),
                creation_date: None,
                completion_date: None,
                description: "Task 1".to_string(),
                projects: vec!["work".to_string()],
                contexts: vec![],
                id: Some("1".to_string()),
                line_number: 1,
            },
            Item {
                completed: false,
                priority: Some('B'),
                creation_date: None,
                completion_date: None,
                description: "Task 2".to_string(),
                projects: vec!["personal".to_string()],
                contexts: vec![],
                id: Some("2".to_string()),
                line_number: 2,
            },
            Item {
                completed: false,
                priority: None,
                creation_date: None,
                completion_date: None,
                description: "Task 3".to_string(),
                projects: vec![],
                contexts: vec![],
                id: Some("3".to_string()),
                line_number: 3,
            },
            Item {
                completed: false,
                priority: None,
                creation_date: None,
                completion_date: None,
                description: "Task 4".to_string(),
                projects: vec!["work".to_string(), "urgent".to_string()],
                contexts: vec![],
                id: Some("4".to_string()),
                line_number: 4,
            },
        ];

        let grouped = group_todos_by_project_owned(&todos);

        assert_eq!(grouped.len(), 4); // work, personal, No Project, urgent
        assert_eq!(grouped.get("work").unwrap().len(), 2); // Task 1 and Task 4
        assert_eq!(grouped.get("personal").unwrap().len(), 1); // Task 2
        assert_eq!(grouped.get("No Project").unwrap().len(), 1); // Task 3
        assert_eq!(grouped.get("urgent").unwrap().len(), 1); // Task 4
    }

    #[test]
    fn test_add_missing_ids() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_add_ids.txt");

        let content = r#"(A) Call Mom +family @phone
Buy groceries +personal @errands id:existing-001
Learn Rust +learning @coding"#;

        fs::write(&test_file, content).unwrap();

        // Add missing IDs
        add_missing_ids(test_file.to_str().unwrap()).unwrap();

        // Read back and verify
        let new_content = fs::read_to_string(&test_file).unwrap();
        let lines: Vec<&str> = new_content.lines().collect();

        assert_eq!(lines.len(), 3);

        // First line should have ID added
        assert!(lines[0].contains("id:"));
        assert!(lines[0].starts_with("(A) Call Mom +family @phone"));

        // Second line should keep existing ID
        assert!(lines[1].contains("id:existing-001"));

        // Third line should have ID added
        assert!(lines[2].contains("id:"));
        assert!(lines[2].starts_with("Learn Rust +learning @coding"));

        fs::remove_file(&test_file).ok();
    }

    #[test]
    fn test_mark_complete() {
        let temp_dir = std::env::temp_dir();
        let todo_file = temp_dir.join("test_complete_todo.txt");

        let content = r#"(A) Call Mom +family @phone id:task-001
Buy groceries +personal @errands id:task-002
Learn Rust +learning @coding id:task-003"#;

        fs::write(&todo_file, content).unwrap();

        // Mark task-002 as complete
        mark_complete(todo_file.to_str().unwrap(), "task-002").unwrap();

        // Check todo.txt - should have 2 remaining tasks
        let remaining_content = fs::read_to_string(&todo_file).unwrap();
        let remaining_lines: Vec<&str> = remaining_content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .collect();
        assert_eq!(remaining_lines.len(), 2);
        assert!(!remaining_content.contains("task-002"));

        // Check done.txt - should have 1 completed task
        let done_file = temp_dir.join("done.txt");
        assert!(done_file.exists(), "done.txt should be created");
        let done_content = fs::read_to_string(&done_file).unwrap();
        assert!(done_content.contains("x"));
        assert!(done_content.contains("Buy groceries +personal @errands id:task-002"));
        assert!(done_content.contains(&chrono::Local::now().format("%Y-%m-%d").to_string()));

        fs::remove_file(&todo_file).ok();
        fs::remove_file(&done_file).ok();
    }

    #[test]
    fn test_append_todo() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_append_todo.txt");

        let content = "(A) Existing task +work id:existing-1";
        fs::write(&test_file, content).unwrap();

        append_todo(
            test_file.to_str().unwrap(),
            "New task +myproject id:new-slug",
        )
        .unwrap();

        let result = fs::read_to_string(&test_file).unwrap();
        assert!(result.contains("Existing task"));
        assert!(result.contains("New task +myproject id:new-slug"));

        fs::remove_file(&test_file).ok();
    }

    #[test]
    fn test_append_todo_to_nonexistent_file() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_append_todo_new.txt");
        fs::remove_file(&test_file).ok();

        append_todo(test_file.to_str().unwrap(), "First task +project id:first").unwrap();

        let result = fs::read_to_string(&test_file).unwrap();
        assert!(result.contains("First task +project id:first"));

        fs::remove_file(&test_file).ok();
    }

    #[test]
    fn test_has_todo_with_id() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_has_todo_id.txt");

        let content = "(A) Task one +work id:task-001\nTask two +personal id:task-002";
        fs::write(&test_file, content).unwrap();

        assert!(has_todo_with_id(test_file.to_str().unwrap(), "task-001"));
        assert!(has_todo_with_id(test_file.to_str().unwrap(), "task-002"));
        assert!(!has_todo_with_id(test_file.to_str().unwrap(), "task-003"));
        assert!(!has_todo_with_id(test_file.to_str().unwrap(), "task-00")); // partial match should not work

        fs::remove_file(&test_file).ok();
    }

    #[test]
    fn test_has_todo_with_id_nonexistent_file() {
        assert!(!has_todo_with_id("/nonexistent/path/todo.txt", "any-id"));
    }

    #[test]
    fn test_move_to_file() {
        let temp_dir = std::env::temp_dir().join("torudo_test_move_to_file");
        fs::create_dir_all(&temp_dir).unwrap();

        let source_file = temp_dir.join("todo.txt");
        let dest_file = temp_dir.join("ref.txt");

        // Clean up before test
        fs::remove_file(&dest_file).ok();

        let content = "(A) Call Mom +family @phone id:task-001\nBuy groceries +personal @errands id:task-002\nLearn Rust +learning @coding id:task-003";
        fs::write(&source_file, content).unwrap();

        move_to_file(
            source_file.to_str().unwrap(),
            dest_file.to_str().unwrap(),
            "task-002",
        )
        .unwrap();

        // source should have 2 remaining lines
        let remaining = fs::read_to_string(&source_file).unwrap();
        let remaining_lines: Vec<&str> =
            remaining.lines().filter(|l| !l.trim().is_empty()).collect();
        assert_eq!(remaining_lines.len(), 2);
        assert!(!remaining.contains("task-002"));
        assert!(remaining.contains("task-001"));
        assert!(remaining.contains("task-003"));

        // dest should have the moved line as-is
        let dest_content = fs::read_to_string(&dest_file).unwrap();
        assert!(dest_content.contains("Buy groceries +personal @errands id:task-002"));

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_move_to_file_dest_not_exists() {
        let temp_dir = std::env::temp_dir().join("torudo_test_move_dest_new");
        fs::create_dir_all(&temp_dir).unwrap();

        let source_file = temp_dir.join("todo.txt");
        let dest_file = temp_dir.join("ref.txt");
        fs::remove_file(&dest_file).ok();

        let content = "Task one id:task-001";
        fs::write(&source_file, content).unwrap();

        move_to_file(
            source_file.to_str().unwrap(),
            dest_file.to_str().unwrap(),
            "task-001",
        )
        .unwrap();

        assert!(dest_file.exists());
        let dest_content = fs::read_to_string(&dest_file).unwrap();
        assert!(dest_content.contains("Task one id:task-001"));

        // source should be empty (no non-empty lines)
        let remaining = fs::read_to_string(&source_file).unwrap();
        let remaining_lines: Vec<&str> =
            remaining.lines().filter(|l| !l.trim().is_empty()).collect();
        assert_eq!(remaining_lines.len(), 0);

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_move_to_file_appends_to_existing_dest() {
        let temp_dir = std::env::temp_dir().join("torudo_test_move_append");
        fs::create_dir_all(&temp_dir).unwrap();

        let source_file = temp_dir.join("todo.txt");
        let dest_file = temp_dir.join("ref.txt");

        fs::write(&source_file, "New item id:task-002").unwrap();
        fs::write(&dest_file, "Existing item id:task-001\n").unwrap();

        move_to_file(
            source_file.to_str().unwrap(),
            dest_file.to_str().unwrap(),
            "task-002",
        )
        .unwrap();

        let dest_content = fs::read_to_string(&dest_file).unwrap();
        assert!(dest_content.contains("Existing item id:task-001"));
        assert!(dest_content.contains("New item id:task-002"));

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_mark_complete_with_priority() {
        let temp_dir = std::env::temp_dir().join("torudo_test_priority");
        fs::create_dir_all(&temp_dir).unwrap();

        let todo_file = temp_dir.join("todo.txt");
        let done_file = temp_dir.join("done.txt");

        // Clean up before test
        fs::remove_file(&done_file).ok();

        let content = "(A) 2024-01-10 Call Mom +family @phone id:task-001";
        fs::write(&todo_file, content).unwrap();

        mark_complete(todo_file.to_str().unwrap(), "task-001").unwrap();

        let done_content = fs::read_to_string(&done_file).unwrap();

        // 正しい順序: x (A) completion-date creation-date description
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        assert!(
            done_content.starts_with(&format!("x (A) {today} 2024-01-10")),
            "Expected format: 'x (A) {today} 2024-01-10...', but got: {}",
            done_content
        );

        fs::remove_dir_all(&temp_dir).ok();
    }
}
