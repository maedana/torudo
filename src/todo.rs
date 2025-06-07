use chrono::NaiveDate;
use std::{collections::HashMap, error::Error, fs};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Item {
    pub completed: bool,
    pub priority: Option<char>,
    pub creation_date: Option<NaiveDate>,
    pub completion_date: Option<NaiveDate>,
    pub description: String,
    pub projects: Vec<String>,
    pub contexts: Vec<String>,
    pub id: Option<String>,
}

impl Item {
    pub fn parse(line: &str) -> Self {
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
        };
        let mut desc_parts = Vec::new();
        if parts.peek() == Some(&"x") {
            item.completed = true;
            parts.next();
            if let Some(date_str) = parts.peek() {
                if let Ok(date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                    item.completion_date = Some(date);
                    parts.next();
                }
            }
        }
        if let Some(part) = parts.peek() {
            if part.len() == 3 && part.starts_with('(') && part.ends_with(')') {
                if let Some(c) = part.chars().nth(1) {
                    if c.is_ascii_uppercase() {
                        item.priority = Some(c);
                        parts.next();
                    }
                }
            }
        }
        if let Some(date_str) = parts.peek() {
            if let Ok(date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                item.creation_date = Some(date);
                parts.next();
            }
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
    let todos = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(Item::parse)
        .collect();
    Ok(todos)
}

pub fn add_missing_ids(file_path: &str) -> Result<(), Box<dyn Error>> {
    let content = fs::read_to_string(file_path)?;
    let lines: Vec<&str> = content.lines().collect();
    let mut modified = false;
    let mut new_lines = Vec::new();
    
    for line in lines {
        if line.trim().is_empty() {
            new_lines.push(line.to_string());
            continue;
        }
        
        let todo = Item::parse(line);
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
        fs::write(file_path, new_content)?;
    }
    
    Ok(())
}

pub fn mark_complete(todo_file: &str, todo_id: &str) -> Result<(), Box<dyn Error>> {
    let content = fs::read_to_string(todo_file)?;
    let lines: Vec<&str> = content.lines().collect();
    let mut new_lines = Vec::new();
    let mut completed_line = None;
    
    for line in lines {
        if line.trim().is_empty() {
            new_lines.push(line.to_string());
            continue;
        }
        
        let todo = Item::parse(line);
        if let Some(id) = &todo.id {
            if id == todo_id {
                let today = chrono::Local::now().format("%Y-%m-%d").to_string();
                let completed_todo_line = if todo.completed {
                    line.to_string()
                } else {
                    format!("x {today} {line}")
                };
                completed_line = Some(completed_todo_line);
                continue;
            }
        }
        new_lines.push(line.to_string());
    }
    
    if let Some(completed_todo) = completed_line {
        let todo_dir = std::path::Path::new(todo_file).parent().unwrap();
        let done_file = todo_dir.join("done.txt");
        
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
    }
    
    Ok(())
}

pub fn group_todos_by_project_owned(todos: &[Item]) -> HashMap<String, Vec<Item>> {
    let mut grouped = HashMap::new();
    for todo in todos {
        if todo.projects.is_empty() {
            grouped.entry("No Project".to_string()).or_insert_with(Vec::new).push(todo.clone());
        } else {
            for project in &todo.projects {
                grouped.entry(project.clone()).or_insert_with(Vec::new).push(todo.clone());
            }
        }
    }
    grouped
}