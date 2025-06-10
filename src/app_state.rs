use std::{collections::HashMap, env, process::Command};
use log::{debug, error};
use crate::todo::{Item, load_todos, add_missing_ids, mark_complete, group_todos_by_project_owned};

fn send_vim_command(todo_id: &str) {
    let home_dir = env::var("HOME").unwrap();
    let todotxt_dir = env::var("TODOTXT_DIR").unwrap_or_else(|_| format!("{home_dir}/todotxt"));
    let file_path = format!("{todotxt_dir}/todos/{todo_id}.md");
    let command = format!(":e {file_path}<CR>");
    let socket_path = env::var("NVIM_LISTEN_ADDRESS")
        .unwrap_or_else(|_| "/tmp/nvim.sock".to_string());

    match Command::new("nvim")
        .arg("--server")
        .arg(&socket_path)
        .arg("--remote-send")
        .arg(&command)
        .output() {
        Ok(_) => debug!("Sent vim command: {}", command),
        Err(e) => debug!("Failed to send vim command: {}", e),
    }
}

pub struct AppState {
    pub todos: Vec<Item>,
    pub grouped_todos: HashMap<String, Vec<Item>>,
    pub project_names: Vec<String>,
    pub current_column: usize,
    pub selected_in_column: usize,
}

impl AppState {
    pub fn new(todos: Vec<Item>) -> Self {
        let grouped_todos = group_todos_by_project_owned(&todos);
        let mut project_names: Vec<String> = grouped_todos.keys().cloned().collect();
        project_names.sort();

        Self {
            todos,
            grouped_todos,
            project_names,
            current_column: 0,
            selected_in_column: 0,
        }
    }

    pub fn reload_todos(&mut self, todo_file: &str) {
        match load_todos(todo_file) {
            Ok(new_todos) => {
                debug!("Reloaded {} todos from file", new_todos.len());
                self.todos = new_todos;
                self.update_derived_state();
            },
            Err(e) => error!("Failed to reload todos: {}", e),
        }
    }

    fn update_derived_state(&mut self) {
        self.grouped_todos = group_todos_by_project_owned(&self.todos);
        self.project_names = self.grouped_todos.keys().cloned().collect();
        self.project_names.sort();

        if self.current_column >= self.project_names.len() {
            self.current_column = self.project_names.len().saturating_sub(1);
        }
        if let Some(current_project_name) = self.project_names.get(self.current_column) {
            if let Some(current_todos) = self.grouped_todos.get(current_project_name) {
                if self.selected_in_column >= current_todos.len() {
                    self.selected_in_column = current_todos.len().saturating_sub(1);
                }
                if let Some(selected_todo) = current_todos.get(self.selected_in_column) {
                    if let Some(todo_id) = &selected_todo.id {
                        send_vim_command(todo_id);
                    }
                }
            }
        }
    }

    pub fn get_current_todo_id(&self) -> Option<&str> {
        let current_project_name = self.project_names.get(self.current_column)?;
        let current_todos = self.grouped_todos.get(current_project_name)?;
        let selected_todo = current_todos.get(self.selected_in_column)?;
        selected_todo.id.as_deref()
    }

    pub fn handle_navigation_key(&mut self, key_char: char) {
        match key_char {
            'k' => {
                if self.selected_in_column > 0 {
                    self.selected_in_column -= 1;
                    if let Some(todo_id) = self.get_current_todo_id() {
                        send_vim_command(todo_id);
                    }
                }
            },
            'j' => {
                if let Some(current_project_name) = self.project_names.get(self.current_column) {
                    if let Some(current_todos) = self.grouped_todos.get(current_project_name) {
                        if self.selected_in_column < current_todos.len().saturating_sub(1) {
                            self.selected_in_column += 1;
                            if let Some(todo_id) = self.get_current_todo_id() {
                                send_vim_command(todo_id);
                            }
                        }
                    }
                }
            },
            'h' => {
                if self.current_column > 0 {
                    self.current_column -= 1;
                    self.selected_in_column = 0;
                    if let Some(todo_id) = self.get_current_todo_id() {
                        send_vim_command(todo_id);
                    }
                }
            },
            'l' => {
                if self.current_column < self.project_names.len().saturating_sub(1) {
                    self.current_column += 1;
                    self.selected_in_column = 0;
                    if let Some(todo_id) = self.get_current_todo_id() {
                        send_vim_command(todo_id);
                    }
                }
            },
            _ => {}
        }
    }

    pub fn handle_complete_todo(&mut self, todo_file: &str) {
        if let Some(todo_id) = self.get_current_todo_id() {
            debug!("Attempting to mark todo as complete: {}", todo_id);
            match mark_complete(todo_file, todo_id) {
                Ok(()) => {
                    debug!("Successfully marked todo as complete: {}", todo_id);
                    self.reload_todos(todo_file);
                },
                Err(e) => error!("Failed to mark todo as complete: {}", e),
            }
        }
    }

    pub fn handle_reload(&mut self, todo_file: &str) {
        debug!("Manual reload requested");
        if let Err(e) = add_missing_ids(todo_file) {
            error!("Failed to add missing IDs during reload: {}", e);
        }
        self.reload_todos(todo_file);
    }

    pub fn send_initial_vim_command(&self) {
        if let Some(todo_id) = self.get_current_todo_id() {
            send_vim_command(todo_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::todo::Item;
    use std::fs;

    fn create_test_todos() -> Vec<Item> {
        vec![
            Item {
                completed: false,
                priority: Some('A'),
                creation_date: None,
                completion_date: None,
                description: "Task 1".to_string(),
                projects: vec!["work".to_string()],
                contexts: vec!["office".to_string()],
                id: Some("task-1".to_string()),
                line_number: 1,
            },
            Item {
                completed: false,
                priority: Some('B'),
                creation_date: None,
                completion_date: None,
                description: "Task 2".to_string(),
                projects: vec!["personal".to_string()],
                contexts: vec!["home".to_string()],
                id: Some("task-2".to_string()),
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
                id: Some("task-3".to_string()),
                line_number: 3,
            },
            Item {
                completed: false,
                priority: Some('C'),
                creation_date: None,
                completion_date: None,
                description: "Task 4".to_string(),
                projects: vec!["work".to_string(), "urgent".to_string()],
                contexts: vec![],
                id: Some("task-4".to_string()),
                line_number: 4,
            },
        ]
    }

    #[test]
    fn test_app_state_new() {
        let todos = create_test_todos();
        let state = AppState::new(todos.clone());

        assert_eq!(state.todos.len(), 4);
        assert_eq!(state.current_column, 0);
        assert_eq!(state.selected_in_column, 0);
        
        // Should have 4 projects: "No Project", "personal", "urgent", "work" (sorted)
        assert_eq!(state.project_names.len(), 4);
        assert_eq!(state.project_names, vec!["No Project", "personal", "urgent", "work"]);
        
        // Check grouped todos
        assert_eq!(state.grouped_todos.get("work").unwrap().len(), 2); // task-1 and task-4
        assert_eq!(state.grouped_todos.get("personal").unwrap().len(), 1); // task-2
        assert_eq!(state.grouped_todos.get("No Project").unwrap().len(), 1); // task-3
        assert_eq!(state.grouped_todos.get("urgent").unwrap().len(), 1); // task-4
    }

    #[test]
    fn test_get_current_todo_id() {
        let todos = create_test_todos();
        let state = AppState::new(todos);

        // Initial state should select first project ("No Project") and first todo
        assert_eq!(state.get_current_todo_id(), Some("task-3"));
    }

    #[test]
    fn test_handle_navigation_key_vertical() {
        let todos = vec![
            Item {
                completed: false,
                priority: Some('A'),
                creation_date: None,
                completion_date: None,
                description: "Task 1".to_string(),
                projects: vec!["work".to_string()],
                contexts: vec![],
                id: Some("task-1".to_string()),
                line_number: 1,
            },
            Item {
                completed: false,
                priority: Some('B'),
                creation_date: None,
                completion_date: None,
                description: "Task 2".to_string(),
                projects: vec!["work".to_string()],
                contexts: vec![],
                id: Some("task-2".to_string()),
                line_number: 2,
            },
        ];
        
        let mut state = AppState::new(todos);
        
        // Should start at first todo
        assert_eq!(state.selected_in_column, 0);
        assert_eq!(state.get_current_todo_id(), Some("task-1"));
        
        // Move down with 'j'
        state.handle_navigation_key('j');
        assert_eq!(state.selected_in_column, 1);
        assert_eq!(state.get_current_todo_id(), Some("task-2"));
        
        // Try to move down again (should stay at last item)
        state.handle_navigation_key('j');
        assert_eq!(state.selected_in_column, 1);
        
        // Move up with 'k'
        state.handle_navigation_key('k');
        assert_eq!(state.selected_in_column, 0);
        assert_eq!(state.get_current_todo_id(), Some("task-1"));
        
        // Try to move up again (should stay at first item)
        state.handle_navigation_key('k');
        assert_eq!(state.selected_in_column, 0);
    }

    #[test]
    fn test_handle_navigation_key_horizontal() {
        let todos = create_test_todos();
        let mut state = AppState::new(todos);
        
        // Should start at first column ("No Project")
        assert_eq!(state.current_column, 0);
        assert_eq!(state.project_names[state.current_column], "No Project");
        
        // Move right with 'l'
        state.handle_navigation_key('l');
        assert_eq!(state.current_column, 1);
        assert_eq!(state.selected_in_column, 0); // Should reset to first item
        assert_eq!(state.project_names[state.current_column], "personal");
        
        // Move right again
        state.handle_navigation_key('l');
        assert_eq!(state.current_column, 2);
        assert_eq!(state.project_names[state.current_column], "urgent");
        
        // Move left with 'h'
        state.handle_navigation_key('h');
        assert_eq!(state.current_column, 1);
        assert_eq!(state.selected_in_column, 0); // Should reset to first item
        assert_eq!(state.project_names[state.current_column], "personal");
    }

    #[test]
    fn test_handle_navigation_key_boundaries() {
        let todos = create_test_todos();
        let mut state = AppState::new(todos);
        
        // Try to move left from first column (should stay)
        assert_eq!(state.current_column, 0);
        state.handle_navigation_key('h');
        assert_eq!(state.current_column, 0);
        
        // Move to last column
        state.current_column = state.project_names.len() - 1;
        let last_column = state.current_column;
        
        // Try to move right from last column (should stay)
        state.handle_navigation_key('l');
        assert_eq!(state.current_column, last_column);
    }

    #[test]
    fn test_reload_todos_success() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_app_state_reload.txt");
        
        let initial_content = r#"(A) Initial task +work @office id:initial-1"#;
        fs::write(&test_file, initial_content).unwrap();
        
        let initial_todos = vec![
            Item {
                completed: false,
                priority: Some('A'),
                creation_date: None,
                completion_date: None,
                description: "Initial task".to_string(),
                projects: vec!["work".to_string()],
                contexts: vec!["office".to_string()],
                id: Some("initial-1".to_string()),
                line_number: 1,
            }
        ];
        
        let mut state = AppState::new(initial_todos);
        assert_eq!(state.todos.len(), 1);
        
        // Update file content
        let new_content = r#"(A) Initial task +work @office id:initial-1
(B) New task +personal @home id:new-1"#;
        fs::write(&test_file, new_content).unwrap();
        
        // Reload and verify
        state.reload_todos(test_file.to_str().unwrap());
        assert_eq!(state.todos.len(), 2);
        assert_eq!(state.project_names.len(), 2); // "personal", "work"
        
        fs::remove_file(&test_file).ok();
    }

    #[test]
    fn test_handle_complete_todo() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_app_state_complete.txt");
        
        let content = r#"(A) Task to complete +work @office id:complete-me
(B) Other task +work @office id:keep-me"#;
        fs::write(&test_file, content).unwrap();
        
        let todos = vec![
            Item {
                completed: false,
                priority: Some('A'),
                creation_date: None,
                completion_date: None,
                description: "Task to complete".to_string(),
                projects: vec!["work".to_string()],
                contexts: vec!["office".to_string()],
                id: Some("complete-me".to_string()),
                line_number: 1,
            },
            Item {
                completed: false,
                priority: Some('B'),
                creation_date: None,
                completion_date: None,
                description: "Other task".to_string(),
                projects: vec!["work".to_string()],
                contexts: vec!["office".to_string()],
                id: Some("keep-me".to_string()),
                line_number: 2,
            }
        ];
        
        let mut state = AppState::new(todos);
        assert_eq!(state.todos.len(), 2);
        
        // Complete the current todo
        // Since both todos are in "work" project, and "complete-me" should be first
        let current_id = state.get_current_todo_id().expect("Should have a current todo").to_string();
        state.handle_complete_todo(test_file.to_str().unwrap());
        
        // Should reload and have one less todo
        assert_eq!(state.todos.len(), 1);
        
        // Verify the remaining todo is not the one we completed
        let remaining_ids: Vec<String> = state.todos.iter().filter_map(|t| t.id.clone()).collect();
        assert!(!remaining_ids.contains(&current_id));
        
        // Check that done.txt was created
        let done_file = temp_dir.join("done.txt");
        if done_file.exists() {
            let done_content = fs::read_to_string(&done_file).unwrap();
            assert!(done_content.contains(&current_id));
            fs::remove_file(&done_file).ok();
        }
        
        fs::remove_file(&test_file).ok();
    }

    #[test]
    fn test_handle_reload() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_app_state_manual_reload.txt");
        
        let content = r#"(A) Task without ID +work @office
(B) Task with ID +personal @home id:existing-1"#;
        fs::write(&test_file, content).unwrap();
        
        let todos = vec![
            Item {
                completed: false,
                priority: Some('A'),
                creation_date: None,
                completion_date: None,
                description: "Task without ID".to_string(),
                projects: vec!["work".to_string()],
                contexts: vec!["office".to_string()],
                id: None,
                line_number: 1,
            }
        ];
        
        let mut state = AppState::new(todos);
        
        // Manual reload should add missing IDs and reload todos
        state.handle_reload(test_file.to_str().unwrap());
        
        // Should have loaded both todos
        assert_eq!(state.todos.len(), 2);
        
        // Both todos should now have IDs
        assert!(state.todos.iter().all(|todo| todo.id.is_some()));
        
        fs::remove_file(&test_file).ok();
    }

    #[test]
    fn test_update_derived_state_column_bounds() {
        let todos = create_test_todos();
        let mut state = AppState::new(todos);
        
        // Set invalid column index
        state.current_column = 999;
        state.selected_in_column = 999;
        
        // Update derived state should fix bounds
        state.update_derived_state();
        
        assert!(state.current_column < state.project_names.len());
        assert!(state.selected_in_column < state.grouped_todos[&state.project_names[state.current_column]].len());
    }

    #[test]
    fn test_send_initial_vim_command() {
        let todos = create_test_todos();
        let state = AppState::new(todos);
        
        // Should not panic even if vim command fails
        state.send_initial_vim_command();
        
        // Test with empty state
        let empty_state = AppState::new(vec![]);
        empty_state.send_initial_vim_command();
    }
}