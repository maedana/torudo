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