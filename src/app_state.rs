use crate::todo::{add_missing_ids, group_todos_by_project_owned, load_todos, mark_complete, Item};
use log::{debug, error};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use std::{collections::HashMap, env, io::Write, os::unix::net::UnixStream, time::Duration};
use tmux_claude_state::claude_state::ClaudeState;
use tmux_claude_state::monitor::MonitorState;

pub struct AppState {
    pub todos: Vec<Item>,
    pub grouped_todos: HashMap<String, Vec<Item>>,
    pub project_names: Vec<String>,
    pub current_column: usize,
    pub selected_in_column: usize,
    pub nvim_socket: String,
    pub monitor_state: Option<Arc<Mutex<MonitorState>>>,
    pub claude_sessions_enabled: bool,
    pub claude_selected_index: usize,
    pub preview_active: bool,
    pub last_preview_update: Option<Instant>,
    terminal_channel: Option<i64>,
}

impl AppState {
    fn build_nvim_rpc_payload(method: &str, params: Vec<rmpv::Value>) -> Vec<u8> {
        let request = rmpv::Value::Array(vec![
            rmpv::Value::Integer(0.into()),  // type = Request
            rmpv::Value::Integer(1.into()),  // msgid
            rmpv::Value::String(method.into()),
            rmpv::Value::Array(params),
        ]);
        let mut buf = Vec::new();
        rmpv::encode::write_value(&mut buf, &request).expect("msgpack encoding should not fail");
        buf
    }

    fn build_nvim_command_payload(cmd: &str) -> Vec<u8> {
        Self::build_nvim_rpc_payload("nvim_command", vec![rmpv::Value::String(cmd.into())])
    }

    fn parse_rpc_result(response: &rmpv::Value) -> Result<rmpv::Value, Box<dyn std::error::Error>> {
        match response {
            rmpv::Value::Array(items) if items.len() >= 4 => {
                if !items[2].is_nil() {
                    return Err(format!("Neovim RPC error: {}", items[2]).into());
                }
                Ok(items[3].clone())
            }
            _ => Err("Invalid RPC response format".into()),
        }
    }

    fn send_nvim_rpc_with_result(&self, payload: &[u8]) -> Result<rmpv::Value, Box<dyn std::error::Error>> {
        let mut stream = UnixStream::connect(&self.nvim_socket)?;
        stream.set_read_timeout(Some(Duration::from_millis(500)))?;
        stream.write_all(payload)?;
        stream.flush()?;
        let response = rmpv::decode::read_value(&mut stream)?;
        Self::parse_rpc_result(&response)
    }

    fn send_nvim_rpc_raw(&self, payload: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        let mut stream = UnixStream::connect(&self.nvim_socket)?;
        stream.set_read_timeout(Some(Duration::from_millis(500)))?;
        stream.write_all(payload)?;
        stream.flush()?;
        // Read the response to confirm command completion before closing the connection
        let _ = rmpv::decode::read_value(&mut stream);
        Ok(())
    }

    fn send_nvim_rpc_command(&self, cmd: &str) -> Result<(), Box<dyn std::error::Error>> {
        let payload = Self::build_nvim_command_payload(cmd);
        self.send_nvim_rpc_raw(&payload)
    }

    fn send_vim_command(&self, todo_id: &str) {
        let home_dir = env::var("HOME").unwrap();
        let todotxt_dir = env::var("TODOTXT_DIR").unwrap_or_else(|_| format!("{home_dir}/todotxt"));
        let file_path = format!("{todotxt_dir}/todos/{todo_id}.md");
        let cmd = format!("e {file_path}");

        match self.send_nvim_rpc_command(&cmd) {
            Ok(()) => debug!("Sent nvim RPC command: {cmd}"),
            Err(e) => debug!("Failed to send nvim RPC command: {e}"),
        }
    }

    pub fn new(todos: Vec<Item>, nvim_socket: String, monitor_state: Option<Arc<Mutex<MonitorState>>>) -> Self {
        let grouped_todos = group_todos_by_project_owned(&todos);
        let mut project_names: Vec<String> = grouped_todos.keys().cloned().collect();
        project_names.sort();
        let claude_sessions_enabled = monitor_state.is_some();

        Self {
            todos,
            grouped_todos,
            project_names,
            current_column: 0,
            selected_in_column: 0,
            nvim_socket,
            monitor_state,
            claude_sessions_enabled,
            claude_selected_index: 0,
            preview_active: false,
            last_preview_update: None,
            terminal_channel: None,
        }
    }

    pub const fn is_on_claude_column(&self) -> bool {
        self.claude_sessions_enabled && self.current_column == self.project_names.len()
    }

    pub const fn total_columns(&self) -> usize {
        self.project_names.len() + self.claude_sessions_enabled as usize
    }

    pub fn reload_todos(&mut self, todo_file: &str) {
        match load_todos(todo_file) {
            Ok(new_todos) => {
                debug!("Reloaded {} todos from file", new_todos.len());
                self.todos = new_todos;
                self.update_derived_state();
            }
            Err(e) => error!("Failed to reload todos: {e}"),
        }
    }

    fn update_derived_state(&mut self) {
        self.grouped_todos = group_todos_by_project_owned(&self.todos);
        self.project_names = self.grouped_todos.keys().cloned().collect();
        self.project_names.sort();

        if self.current_column >= self.total_columns() {
            self.current_column = self.total_columns().saturating_sub(1);
        }
        if let Some(current_project_name) = self.project_names.get(self.current_column)
            && let Some(current_todos) = self.grouped_todos.get(current_project_name) {
                if self.selected_in_column >= current_todos.len() {
                    self.selected_in_column = current_todos.len().saturating_sub(1);
                }
                if let Some(selected_todo) = current_todos.get(self.selected_in_column)
                    && let Some(todo_id) = &selected_todo.id {
                        self.send_vim_command(todo_id);
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
                if self.is_on_claude_column() {
                    if self.claude_selected_index > 0 {
                        self.claude_selected_index -= 1;
                        self.update_preview_content();
                    }
                } else if self.selected_in_column > 0 {
                    self.selected_in_column -= 1;
                    if let Some(todo_id) = self.get_current_todo_id() {
                        self.send_vim_command(todo_id);
                    }
                }
            }
            'j' => {
                if self.is_on_claude_column() {
                    let session_count = self.claude_session_count();
                    if self.claude_selected_index < session_count.saturating_sub(1) {
                        self.claude_selected_index += 1;
                        self.update_preview_content();
                    }
                } else if let Some(current_project_name) = self.project_names.get(self.current_column)
                    && let Some(current_todos) = self.grouped_todos.get(current_project_name)
                        && self.selected_in_column < current_todos.len().saturating_sub(1) {
                            self.selected_in_column += 1;
                            if let Some(todo_id) = self.get_current_todo_id() {
                                self.send_vim_command(todo_id);
                            }
                        }
            }
            'h' => {
                if self.current_column > 0 {
                    let was_on_claude = self.is_on_claude_column();
                    self.current_column -= 1;
                    if self.is_on_claude_column() {
                        self.claude_selected_index = 0;
                    } else if was_on_claude && self.preview_active {
                        self.leave_preview_mode();
                    } else {
                        self.selected_in_column = 0;
                        if let Some(todo_id) = self.get_current_todo_id() {
                            self.send_vim_command(todo_id);
                        }
                    }
                }
            }
            'l' => {
                if self.current_column < self.total_columns().saturating_sub(1) {
                    self.current_column += 1;
                    if self.is_on_claude_column() {
                        self.claude_selected_index = 0;
                        self.enter_preview_mode();
                        self.update_preview_content();
                    } else {
                        self.selected_in_column = 0;
                        if let Some(todo_id) = self.get_current_todo_id() {
                            self.send_vim_command(todo_id);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn claude_session_count(&self) -> usize {
        self.monitor_state
            .as_ref()
            .and_then(|ms| ms.lock().ok())
            .map_or(0, |lock| lock.sessions.len())
    }

    pub fn handle_complete_todo(&mut self, todo_file: &str) {
        if let Some(todo_id) = self.get_current_todo_id() {
            debug!("Attempting to mark todo as complete: {todo_id}");
            match mark_complete(todo_file, todo_id) {
                Ok(()) => {
                    debug!("Successfully marked todo as complete: {todo_id}");
                    self.reload_todos(todo_file);
                }
                Err(e) => error!("Failed to mark todo as complete: {e}"),
            }
        }
    }

    pub fn handle_reload(&mut self, todo_file: &str) {
        debug!("Manual reload requested");
        if let Err(e) = add_missing_ids(todo_file) {
            error!("Failed to add missing IDs during reload: {e}");
        }
        self.reload_todos(todo_file);
    }

    pub fn handle_switch_tmux_pane(&self) {
        if !self.is_on_claude_column() {
            return;
        }
        let Some(ref monitor_state) = self.monitor_state else {
            return;
        };
        let Ok(lock) = monitor_state.lock() else {
            return;
        };
        let Some(session) = lock.sessions.get(self.claude_selected_index) else {
            return;
        };
        tmux_claude_state::tmux::switch_to_pane(&session.pane.id);
    }

    pub fn send_initial_vim_command(&self) {
        if let Some(todo_id) = self.get_current_todo_id() {
            self.send_vim_command(todo_id);
        }
    }

    fn enter_preview_mode(&mut self) {
        // Create a new empty buffer first
        if let Err(e) = self.send_nvim_rpc_command("enew") {
            debug!("Failed to create new buffer for preview: {e}");
            return;
        }

        // Open a terminal channel on the current buffer
        let payload = Self::build_nvim_rpc_payload(
            "nvim_open_term",
            vec![
                rmpv::Value::Integer(0.into()), // buffer=0 (current)
                rmpv::Value::Map(vec![]),        // opts={}
            ],
        );
        match self.send_nvim_rpc_with_result(&payload) {
            Ok(rmpv::Value::Integer(chan_id)) => {
                debug!("Entered preview mode with terminal channel {chan_id}");
                self.terminal_channel = Some(chan_id.as_i64().unwrap_or(0));
                self.preview_active = true;
                // Set buffer options
                let _ = self.send_nvim_rpc_command(
                    r"setlocal bufhidden=wipe noswapfile | file [Claude\ Preview]",
                );
            }
            Ok(other) => debug!("Unexpected nvim_open_term result: {other}"),
            Err(e) => debug!("Failed to open terminal buffer: {e}"),
        }
    }

    fn update_preview_content(&mut self) {
        let pane_id = {
            let Some(ref monitor_state) = self.monitor_state else {
                return;
            };
            let Ok(lock) = monitor_state.lock() else {
                return;
            };
            let Some(session) = lock.sessions.get(self.claude_selected_index) else {
                return;
            };
            session.pane.id.clone()
        };

        let Some(chan_id) = self.terminal_channel else {
            return;
        };

        let content = tmux_claude_state::tmux::capture_pane_with_ansi(&pane_id);
        // Clear screen and move cursor to home, then send ANSI content
        let data = format!("\x1b[2J\x1b[H{content}");
        let payload = Self::build_nvim_rpc_payload(
            "nvim_chan_send",
            vec![
                rmpv::Value::Integer(chan_id.into()),
                rmpv::Value::String(data.into()),
            ],
        );
        match self.send_nvim_rpc_raw(&payload) {
            Ok(()) => {
                debug!("Updated preview content for pane {pane_id}");
                self.last_preview_update = Some(Instant::now());
            }
            Err(e) => debug!("Failed to update preview content: {e}"),
        }
    }

    fn leave_preview_mode(&mut self) {
        self.preview_active = false;
        self.last_preview_update = None;
        self.terminal_channel = None;
        if let Some(todo_id) = self.get_current_todo_id() {
            self.send_vim_command(todo_id);
        }
    }

    pub fn maybe_update_preview(&mut self) {
        if !self.preview_active {
            return;
        }
        let is_working = self.monitor_state.as_ref()
            .and_then(|ms| ms.lock().ok())
            .and_then(|lock| lock.sessions.get(self.claude_selected_index).cloned())
            .is_some_and(|s| s.state == ClaudeState::Working);
        if !is_working {
            return;
        }
        if let Some(last) = self.last_preview_update
            && last.elapsed() < Duration::from_secs(2)
        {
            return;
        }
        self.update_preview_content();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::todo::Item;
    use std::fs;

    fn create_test_state(todos: Vec<Item>) -> AppState {
        AppState::new(todos, "/tmp/nvim.sock".to_string(), None)
    }

    fn create_test_state_with_claude(todos: Vec<Item>) -> AppState {
        let monitor_state = Arc::new(Mutex::new(MonitorState::default()));
        AppState::new(todos, "/tmp/nvim.sock".to_string(), Some(monitor_state))
    }

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
        let state = AppState::new(todos.clone(), "/tmp/nvim.sock".to_string(), None);

        assert_eq!(state.todos.len(), 4);
        assert_eq!(state.current_column, 0);
        assert_eq!(state.selected_in_column, 0);
        assert_eq!(state.nvim_socket, "/tmp/nvim.sock");

        // Should have 4 projects: "No Project", "personal", "urgent", "work" (sorted)
        assert_eq!(state.project_names.len(), 4);
        assert_eq!(
            state.project_names,
            vec!["No Project", "personal", "urgent", "work"]
        );

        // Check grouped todos
        assert_eq!(state.grouped_todos.get("work").unwrap().len(), 2); // task-1 and task-4
        assert_eq!(state.grouped_todos.get("personal").unwrap().len(), 1); // task-2
        assert_eq!(state.grouped_todos.get("No Project").unwrap().len(), 1); // task-3
        assert_eq!(state.grouped_todos.get("urgent").unwrap().len(), 1); // task-4
    }

    #[test]
    fn test_get_current_todo_id() {
        let todos = create_test_todos();
        let state = create_test_state(todos);

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

        let mut state = create_test_state(todos);

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
        let mut state = create_test_state(todos);

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
        let mut state = create_test_state(todos);

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

        let initial_todos = vec![Item {
            completed: false,
            priority: Some('A'),
            creation_date: None,
            completion_date: None,
            description: "Initial task".to_string(),
            projects: vec!["work".to_string()],
            contexts: vec!["office".to_string()],
            id: Some("initial-1".to_string()),
            line_number: 1,
        }];

        let mut state = create_test_state(initial_todos);
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
            },
        ];

        let mut state = create_test_state(todos);
        assert_eq!(state.todos.len(), 2);

        // Complete the current todo
        // Since both todos are in "work" project, and "complete-me" should be first
        let current_id = state
            .get_current_todo_id()
            .expect("Should have a current todo")
            .to_string();
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

        let todos = vec![Item {
            completed: false,
            priority: Some('A'),
            creation_date: None,
            completion_date: None,
            description: "Task without ID".to_string(),
            projects: vec!["work".to_string()],
            contexts: vec!["office".to_string()],
            id: None,
            line_number: 1,
        }];

        let mut state = create_test_state(todos);

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
        let mut state = create_test_state(todos);

        // Set invalid column index
        state.current_column = 999;
        state.selected_in_column = 999;

        // Update derived state should fix bounds
        state.update_derived_state();

        assert!(state.current_column < state.project_names.len());
        assert!(
            state.selected_in_column
                < state.grouped_todos[&state.project_names[state.current_column]].len()
        );
    }

    #[test]
    fn test_build_nvim_rpc_payload() {
        let params = vec![rmpv::Value::String("e /path/to/file.md".into())];
        let payload = AppState::build_nvim_rpc_payload("nvim_command", params);

        let mut cursor = std::io::Cursor::new(&payload);
        let decoded = rmpv::decode::read_value(&mut cursor).unwrap();

        if let rmpv::Value::Array(items) = decoded {
            assert_eq!(items.len(), 4);
            assert_eq!(items[0], rmpv::Value::Integer(0.into())); // type=Request
            assert_eq!(items[1], rmpv::Value::Integer(1.into())); // msgid
            assert_eq!(items[2], rmpv::Value::String("nvim_command".into()));
            if let rmpv::Value::Array(params) = &items[3] {
                assert_eq!(params[0], rmpv::Value::String("e /path/to/file.md".into()));
            } else {
                panic!("params should be an array");
            }
        } else {
            panic!("decoded value should be an array");
        }
    }

    #[test]
    fn test_build_nvim_rpc_payload_arbitrary_method() {
        let params = vec![
            rmpv::Value::Integer(0.into()),
            rmpv::Value::Integer(0.into()),
            rmpv::Value::Integer((-1).into()),
            rmpv::Value::Boolean(false),
            rmpv::Value::Array(vec![
                rmpv::Value::String("line1".into()),
                rmpv::Value::String("line2".into()),
            ]),
        ];
        let payload = AppState::build_nvim_rpc_payload("nvim_buf_set_lines", params);

        let mut cursor = std::io::Cursor::new(&payload);
        let decoded = rmpv::decode::read_value(&mut cursor).unwrap();

        if let rmpv::Value::Array(items) = decoded {
            assert_eq!(items[2], rmpv::Value::String("nvim_buf_set_lines".into()));
            if let rmpv::Value::Array(params) = &items[3] {
                assert_eq!(params.len(), 5);
                assert_eq!(params[0], rmpv::Value::Integer(0.into()));
                assert_eq!(params[3], rmpv::Value::Boolean(false));
                if let rmpv::Value::Array(lines) = &params[4] {
                    assert_eq!(lines.len(), 2);
                    assert_eq!(lines[0], rmpv::Value::String("line1".into()));
                } else {
                    panic!("lines should be an array");
                }
            } else {
                panic!("params should be an array");
            }
        } else {
            panic!("decoded value should be an array");
        }
    }

    #[test]
    fn test_build_nvim_command_payload() {
        let payload = AppState::build_nvim_command_payload("e /path/to/file.md");

        let mut cursor = std::io::Cursor::new(&payload);
        let decoded = rmpv::decode::read_value(&mut cursor).unwrap();

        if let rmpv::Value::Array(items) = decoded {
            assert_eq!(items.len(), 4);
            assert_eq!(items[0], rmpv::Value::Integer(0.into())); // type=Request
            assert_eq!(items[1], rmpv::Value::Integer(1.into())); // msgid
            assert_eq!(items[2], rmpv::Value::String("nvim_command".into()));
            if let rmpv::Value::Array(params) = &items[3] {
                assert_eq!(params[0], rmpv::Value::String("e /path/to/file.md".into()));
            } else {
                panic!("params should be an array");
            }
        } else {
            panic!("decoded value should be an array");
        }
    }

    #[test]
    fn test_parse_rpc_result_success() {
        let response = rmpv::Value::Array(vec![
            rmpv::Value::Integer(1.into()), // type=Response
            rmpv::Value::Integer(1.into()), // msgid
            rmpv::Value::Nil,               // no error
            rmpv::Value::Integer(42.into()), // result
        ]);
        let result = AppState::parse_rpc_result(&response).unwrap();
        assert_eq!(result, rmpv::Value::Integer(42.into()));
    }

    #[test]
    fn test_parse_rpc_result_error() {
        let response = rmpv::Value::Array(vec![
            rmpv::Value::Integer(1.into()),
            rmpv::Value::Integer(1.into()),
            rmpv::Value::String("some error".into()),
            rmpv::Value::Nil,
        ]);
        let result = AppState::parse_rpc_result(&response);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Neovim RPC error"));
    }

    #[test]
    fn test_parse_rpc_result_invalid_format() {
        let response = rmpv::Value::Integer(42.into());
        let result = AppState::parse_rpc_result(&response);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid RPC response format"));
    }

    #[test]
    fn test_app_state_total_columns_without_claude() {
        let todos = create_test_todos();
        let state = create_test_state(todos);
        assert_eq!(state.total_columns(), 4); // project_names.len() only
        assert!(!state.claude_sessions_enabled);
    }

    #[test]
    fn test_app_state_total_columns_with_claude() {
        let todos = create_test_todos();
        let state = create_test_state_with_claude(todos);
        assert_eq!(state.total_columns(), 5); // project_names.len() + 1
        assert!(state.claude_sessions_enabled);
    }

    #[test]
    fn test_app_state_is_on_claude_column() {
        let todos = create_test_todos();
        let mut state = create_test_state_with_claude(todos);
        // Initially at column 0, not on Claude column
        assert!(!state.is_on_claude_column());
        // Move to Claude column (index == project_names.len())
        state.current_column = state.project_names.len();
        assert!(state.is_on_claude_column());
    }

    #[test]
    fn test_app_state_is_on_claude_column_disabled() {
        let todos = create_test_todos();
        let mut state = create_test_state(todos);
        // Even at last+1 position, not Claude column when disabled
        state.current_column = state.project_names.len();
        assert!(!state.is_on_claude_column());
    }

    #[test]
    fn test_navigation_to_claude_column() {
        let todos = create_test_todos();
        let mut state = create_test_state_with_claude(todos);
        // 4 project columns + 1 claude = total 5
        // Move to last project column
        state.current_column = 3; // "work"
        // Move right to Claude column
        state.handle_navigation_key('l');
        assert_eq!(state.current_column, 4);
        assert!(state.is_on_claude_column());
        // Move right again, should stay
        state.handle_navigation_key('l');
        assert_eq!(state.current_column, 4);
        // Move left back to project column
        state.handle_navigation_key('h');
        assert_eq!(state.current_column, 3);
        assert!(!state.is_on_claude_column());
    }

    #[test]
    fn test_navigation_without_claude_column() {
        let todos = create_test_todos();
        let mut state = create_test_state(todos);
        // 4 project columns, no claude
        state.current_column = 3; // last project
        state.handle_navigation_key('l');
        assert_eq!(state.current_column, 3); // should stay at last project
    }

    #[test]
    fn test_claude_column_vertical_navigation() {
        use std::time::Instant;
        use tmux_claude_state::claude_state::ClaudeState;
        use tmux_claude_state::monitor::ClaudeSession;
        use tmux_claude_state::tmux::PaneInfo;

        let todos = create_test_todos();
        let monitor_state = Arc::new(Mutex::new(MonitorState {
            sessions: vec![
                ClaudeSession {
                    pane: PaneInfo { id: "0:0.0".to_string(), pid: 1, cwd: "/proj1".to_string(), project_name: "proj1".to_string() },
                    state: ClaudeState::Working,
                    state_changed_at: Instant::now(),
                },
                ClaudeSession {
                    pane: PaneInfo { id: "0:0.1".to_string(), pid: 2, cwd: "/proj2".to_string(), project_name: "proj2".to_string() },
                    state: ClaudeState::Idle,
                    state_changed_at: Instant::now(),
                },
                ClaudeSession {
                    pane: PaneInfo { id: "0:1.0".to_string(), pid: 3, cwd: "/proj3".to_string(), project_name: "proj3".to_string() },
                    state: ClaudeState::WaitingForApproval,
                    state_changed_at: Instant::now(),
                },
            ],
            any_claude_focused: false,
        }));
        let mut state = AppState::new(todos, "/tmp/nvim.sock".to_string(), Some(monitor_state));

        // Move to Claude column
        state.current_column = state.project_names.len();
        assert!(state.is_on_claude_column());
        assert_eq!(state.claude_selected_index, 0);

        // j moves down
        state.handle_navigation_key('j');
        assert_eq!(state.claude_selected_index, 1);

        state.handle_navigation_key('j');
        assert_eq!(state.claude_selected_index, 2);

        // Can't go past last
        state.handle_navigation_key('j');
        assert_eq!(state.claude_selected_index, 2);

        // k moves up
        state.handle_navigation_key('k');
        assert_eq!(state.claude_selected_index, 1);

        state.handle_navigation_key('k');
        assert_eq!(state.claude_selected_index, 0);

        // Can't go past first
        state.handle_navigation_key('k');
        assert_eq!(state.claude_selected_index, 0);
    }

    #[test]
    fn test_send_initial_vim_command() {
        let todos = create_test_todos();
        let state = create_test_state(todos);

        // Should not panic even if vim command fails
        state.send_initial_vim_command();

        // Test with empty state
        let empty_state = create_test_state(vec![]);
        empty_state.send_initial_vim_command();
    }
}
