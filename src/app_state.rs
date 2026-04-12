use crate::crmux::Plan;
use crate::url::{extract_urls, open_urls};
use crate::todo::{
    add_missing_ids, append_todo, group_todos_by_project_owned, has_todo_with_id, load_todos,
    mark_complete, move_to_file, Item,
};
use log::{debug, error};
use std::{
    collections::{HashMap, HashSet},
    io::Write, os::unix::net::UnixStream, time::Duration,
    time::SystemTime,
};

fn parse_frontmatter_cwd(content: &str) -> Option<String> {
    let content = content.trim();
    if !content.starts_with("---") {
        return None;
    }
    let after_first = &content[3..].trim_start_matches('\n');
    let end = after_first.find("\n---")?;
    let frontmatter = &after_first[..end];
    for line in frontmatter.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("cwd:") {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn strip_frontmatter(content: &str) -> &str {
    let trimmed = content.trim();
    if !trimmed.starts_with("---") {
        return content;
    }
    let after_first = &trimmed[3..].trim_start_matches('\n');
    after_first.find("\n---").map_or(content, |end| {
        let rest = &after_first[end + 4..];
        rest.trim_start_matches('\n')
    })
}

fn sort_plans_by_mtime(plans: &mut [Plan]) {
    plans.sort_by(|a, b| {
        let mtime_a = std::fs::metadata(&a.path)
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        let mtime_b = std::fs::metadata(&b.path)
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        mtime_b.cmp(&mtime_a)
    });
}

pub struct PlanModal {
    pub plans: Vec<Plan>,
    pub selected: usize,
    pub checked: Vec<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewMode {
    Todo,
    Ref,
}

pub struct AppState {
    pub todos: Vec<Item>,
    pub grouped_todos: HashMap<String, Vec<Item>>,
    pub project_names: Vec<String>,
    pub current_column: usize,
    pub selected_in_column: usize,
    pub scroll_offset: usize,
    pub nvim_socket: String,
    pub todotxt_dir: String,
    pub crmux_version: Option<(u32, u32, u32)>,
    pub claude_available: bool,
    pub status_message: Option<String>,
    pub plan_modal: Option<PlanModal>,
    pub show_help: bool,
    hidden_projects: HashSet<String>,
    pub update_available: Option<String>,
    pub view_mode: ViewMode,
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

    fn send_nvim_rpc_command(&self, cmd: &str) -> Result<(), Box<dyn std::error::Error>> {
        let payload = Self::build_nvim_command_payload(cmd);
        let mut stream = UnixStream::connect(&self.nvim_socket)?;
        stream.set_read_timeout(Some(Duration::from_millis(500)))?;
        stream.write_all(&payload)?;
        stream.flush()?;
        // Read the response to confirm command completion before closing the connection
        let _ = rmpv::decode::read_value(&mut stream);
        Ok(())
    }

    fn send_vim_command(&self, todo_id: &str) {
        let file_path = format!("{}/todos/{todo_id}.md", self.todotxt_dir);
        let cmd = format!("e {file_path}");

        match self.send_nvim_rpc_command(&cmd) {
            Ok(()) => debug!("Sent nvim RPC command: {cmd}"),
            Err(e) => debug!("Failed to send nvim RPC command: {e}"),
        }
    }

    pub fn new(todos: Vec<Item>, nvim_socket: String, hidden_projects: HashSet<String>, todotxt_dir: String) -> Self {
        let grouped_todos = group_todos_by_project_owned(&todos);
        let mut project_names: Vec<String> = grouped_todos.keys().cloned().collect();
        project_names.sort();

        let crmux_version = crate::crmux::detect();
        let claude_available = crate::claude::detect();

        Self {
            todos,
            grouped_todos,
            project_names,
            current_column: 0,
            selected_in_column: 0,
            scroll_offset: 0,
            nvim_socket,
            todotxt_dir,
            crmux_version,
            claude_available,
            status_message: None,
            plan_modal: None,
            show_help: false,
            hidden_projects,
            update_available: None,
            view_mode: ViewMode::Todo,
        }
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

        let visible = self.visible_project_names();
        if self.current_column >= visible.len() {
            self.current_column = visible.len().saturating_sub(1);
        }
        if let Some(current_project_name) = visible.get(self.current_column)
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

    pub fn get_current_todo(&self) -> Option<&Item> {
        let visible = self.visible_project_names();
        let current_project_name = visible.get(self.current_column)?;
        let current_todos = self.grouped_todos.get(current_project_name)?;
        current_todos.get(self.selected_in_column)
    }

    pub fn get_current_todo_id(&self) -> Option<&str> {
        self.get_current_todo()?.id.as_deref()
    }

    pub fn handle_navigation_key(&mut self, key_char: char) {
        self.status_message = None;
        let visible = self.visible_project_names();
        match key_char {
            'k' => {
                if self.selected_in_column > 0 {
                    self.selected_in_column -= 1;
                    if let Some(todo_id) = self.get_current_todo_id() {
                        self.send_vim_command(todo_id);
                    }
                }
            }
            'j' => {
                if let Some(current_project_name) = visible.get(self.current_column)
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
                    self.current_column -= 1;
                    self.selected_in_column = 0;
                    self.scroll_offset = 0;
                    if let Some(todo_id) = self.get_current_todo_id() {
                        self.send_vim_command(todo_id);
                    }
                }
            }
            'l' => {
                if self.current_column < visible.len().saturating_sub(1) {
                    self.current_column += 1;
                    self.selected_in_column = 0;
                    self.scroll_offset = 0;
                    if let Some(todo_id) = self.get_current_todo_id() {
                        self.send_vim_command(todo_id);
                    }
                }
            }
            _ => {}
        }
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

    pub fn handle_open_urls(&mut self) {
        if let Some(todo) = self.get_current_todo() {
            let urls = extract_urls(&todo.description);
            if urls.is_empty() {
                self.status_message = Some("No URLs found".to_string());
            } else {
                let count = urls.len();
                let failures = open_urls(&urls);
                if failures == 0 {
                    self.status_message = Some(format!("Opened {count} URL(s)"));
                } else {
                    self.status_message =
                        Some(format!("Opened {} URL(s), {failures} failed", count - failures));
                }
            }
        }
    }

    pub const fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }

    pub fn visible_project_names(&self) -> Vec<String> {
        self.project_names
            .iter()
            .filter(|name| !self.hidden_projects.contains(name.as_str()))
            .cloned()
            .collect()
    }

    pub fn hide_current_project(&mut self) {
        let visible = self.visible_project_names();
        if let Some(project_name) = visible.get(self.current_column) {
            self.hidden_projects.insert(project_name.clone());
            let new_visible_len = visible.len() - 1;
            if self.current_column >= new_visible_len {
                self.current_column = new_visible_len.saturating_sub(1);
            }
            self.selected_in_column = 0;
        }
    }

    pub fn show_all_projects(&mut self) {
        self.hidden_projects.clear();
    }

    pub fn hidden_projects_display(&self) -> Option<String> {
        if self.hidden_projects.is_empty() {
            return None;
        }
        let mut names: Vec<&String> = self.hidden_projects.iter().collect();
        names.sort();
        Some(format!("Hidden: {}", names.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")))
    }

    pub fn active_file(&self) -> String {
        match self.view_mode {
            ViewMode::Todo => format!("{}/todo.txt", self.todotxt_dir),
            ViewMode::Ref => format!("{}/ref.txt", self.todotxt_dir),
        }
    }

    pub fn handle_toggle_mode(&mut self) {
        self.view_mode = match self.view_mode {
            ViewMode::Todo => ViewMode::Ref,
            ViewMode::Ref => ViewMode::Todo,
        };
        let file = self.active_file();
        // Create file if it doesn't exist
        if !std::path::Path::new(&file).exists()
            && let Err(e) = std::fs::write(&file, "")
        {
            error!("Failed to create {file}: {e}");
            return;
        }
        if let Err(e) = add_missing_ids(&file) {
            error!("Failed to add missing IDs: {e}");
        }
        self.reload_todos(&file);
        self.current_column = 0;
        self.selected_in_column = 0;
        self.scroll_offset = 0;
    }

    pub fn handle_move_to_ref(&mut self, todo_file: &str) {
        if self.view_mode != ViewMode::Todo {
            return;
        }
        if let Some(todo_id) = self.get_current_todo_id() {
            let ref_file = format!("{}/ref.txt", self.todotxt_dir);
            debug!("Attempting to move todo to ref.txt: {todo_id}");
            match move_to_file(todo_file, &ref_file, todo_id) {
                Ok(()) => {
                    debug!("Successfully moved todo to ref.txt: {todo_id}");
                    self.reload_todos(todo_file);
                }
                Err(e) => error!("Failed to move todo to ref.txt: {e}"),
            }
        }
    }

    pub fn send_initial_vim_command(&self) {
        if let Some(todo_id) = self.get_current_todo_id() {
            self.send_vim_command(todo_id);
        }
    }

    pub fn get_current_project_name(&self) -> Option<String> {
        let visible = self.visible_project_names();
        visible.get(self.current_column).cloned()
    }

    fn get_current_todo_description(&self) -> Option<String> {
        let visible = self.visible_project_names();
        let project_name = visible.get(self.current_column)?;
        let todos = self.grouped_todos.get(project_name)?;
        let todo = todos.get(self.selected_in_column)?;
        Some(todo.description.clone())
    }

    fn build_prompt(&self, todotxt_dir: &str) -> Option<(String, String)> {
        let project = self.get_current_project_name()?;
        let todo_id = self.get_current_todo_id()?;
        let description = self.get_current_todo_description()?;

        let md_path = format!("{todotxt_dir}/todos/{todo_id}.md");
        let md_content = std::fs::read_to_string(&md_path).unwrap_or_default();
        let md_content = strip_frontmatter(&md_content).trim();

        let text = if md_content.is_empty() {
            format!("# Task: {description}")
        } else {
            format!("# Task: {description}\n\n## Details\n{md_content}")
        };

        Some((project, text))
    }

    pub const fn crmux_available(&self) -> bool {
        self.crmux_version.is_some()
    }

    pub fn crmux_supports_get_plans(&self) -> bool {
        self.crmux_version
            .is_some_and(crate::crmux::version_supports_get_plans)
    }

    fn send_to_crmux(&mut self, todotxt_dir: &str, mode: Option<&str>, label: &str) {
        if !self.crmux_available() {
            return;
        }
        if let Some((project, text)) = self.build_prompt(todotxt_dir) {
            debug!("{label} prompt for project '{project}':\n{text}");
            match crate::crmux::send_text(&project, &text, mode) {
                Ok(()) => {
                    self.status_message = Some(format!("Sent {label} prompt -> {project}"));
                    debug!("Sent {label} prompt to crmux project: {project}");
                }
                Err(e) => {
                    self.status_message = Some(format!("Failed to send {label}: {e}"));
                    error!("Failed to send {label} prompt: {e}");
                }
            }
        }
    }

    pub fn handle_open_plan_modal(&mut self) {
        match crate::crmux::get_plans() {
            Ok(mut plans) => {
                if plans.is_empty() {
                    self.status_message = Some("No plans found".to_string());
                } else {
                    sort_plans_by_mtime(&mut plans);
                    let checked = vec![false; plans.len()];
                    self.plan_modal = Some(PlanModal {
                        plans,
                        selected: 0,
                        checked,
                    });
                }
            }
            Err(e) => {
                self.status_message = Some(format!("Failed to get plans: {e}"));
                error!("Failed to get plans: {e}");
            }
        }
    }

    pub fn handle_plan_modal_key(&mut self, key_char: char, todo_file: &str) {
        let Some(modal) = &mut self.plan_modal else {
            return;
        };
        match key_char {
            'j' => {
                if modal.selected < modal.plans.len().saturating_sub(1) {
                    modal.selected += 1;
                }
            }
            'k' => {
                if modal.selected > 0 {
                    modal.selected -= 1;
                }
            }
            ' ' => {
                let idx = modal.selected;
                modal.checked[idx] = !modal.checked[idx];
            }
            _ => {}
        }
        if key_char == 'q' {
            self.plan_modal = None;
        } else if key_char == '\r' {
            self.import_selected_plans(todo_file);
        }
    }

    fn import_selected_plans(&mut self, todo_file: &str) {
        let Some(modal) = self.plan_modal.take() else {
            return;
        };

        let todotxt_dir = std::path::Path::new(todo_file)
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or(".");

        let mut imported = 0u32;
        let mut skipped = 0u32;

        for (i, plan) in modal.plans.iter().enumerate() {
            if !modal.checked[i] {
                continue;
            }

            if has_todo_with_id(todo_file, &plan.slug) {
                skipped += 1;
                debug!("Skipping already existing plan: {}", plan.slug);
                continue;
            }

            let line = format!("{} +{} id:{}", plan.title, plan.project_name, plan.slug);
            if let Err(e) = append_todo(todo_file, &line) {
                error!("Failed to append todo: {e}");
                continue;
            }

            // Copy plan file to todos directory
            let dest = format!("{todotxt_dir}/todos/{}.md", plan.slug);
            if let Ok(content) = std::fs::read_to_string(&plan.path)
                && let Err(e) = std::fs::write(&dest, content)
            {
                error!("Failed to copy plan file: {e}");
            }

            imported += 1;
        }

        self.status_message = Some(format!("Imported {imported} plans (skipped {skipped})"));
        self.reload_todos(todo_file);
    }

    pub const fn claude_available(&self) -> bool {
        self.claude_available
    }

    pub fn handle_send_plan(&mut self, todotxt_dir: &str) {
        self.send_to_crmux(todotxt_dir, Some("plan-mode"), "plan");
    }

    pub fn handle_send_implement(&mut self, todotxt_dir: &str) {
        self.send_to_crmux(todotxt_dir, Some("accept-edits"), "implement");
    }

    pub fn handle_launch_plan(&mut self, todotxt_dir: &str) {
        self.launch_claude(todotxt_dir, "plan", "Launch Plan");
    }

    pub fn handle_launch_implement(&mut self, todotxt_dir: &str) {
        self.launch_claude(todotxt_dir, "auto", "Launch Implement");
    }

    fn launch_claude(&mut self, todotxt_dir: &str, permission_mode: &str, label: &str) {
        if !self.claude_available {
            self.status_message = Some("claude CLI not found".to_string());
            return;
        }
        let Some((_project, text)) = self.build_prompt(todotxt_dir) else {
            return;
        };
        let Some(todo_id) = self.get_current_todo_id().map(str::to_string) else {
            return;
        };
        let md_path = format!("{todotxt_dir}/todos/{todo_id}.md");
        let md_content = std::fs::read_to_string(&md_path).unwrap_or_default();
        let Some(cwd) = parse_frontmatter_cwd(&md_content) else {
            self.status_message =
                Some(format!("cwd not set in {todo_id}.md frontmatter"));
            return;
        };
        match crate::claude::launch(&text, permission_mode, &todo_id, &cwd) {
            Ok(()) => {
                self.status_message = Some(format!("Launched {label} session -> {todo_id}"));
                debug!("Launched {label} session for todo: {todo_id}");
            }
            Err(e) => {
                self.status_message = Some(format!("Failed to launch {label}: {e}"));
                error!("Failed to launch {label}: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::todo::Item;
    use std::fs;

    fn create_test_state(todos: Vec<Item>) -> AppState {
        let mut state = AppState::new(todos, "/tmp/nvim.sock".to_string(), HashSet::new(), "/tmp/todotxt".to_string());
        state.crmux_version = None;
        state.claude_available = false;
        state
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
        let state = AppState::new(todos.clone(), "/tmp/nvim.sock".to_string(), HashSet::new(), "/tmp/todotxt".to_string());

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
    fn test_handle_toggle_mode() {
        let temp_dir = std::env::temp_dir().join("torudo_test_toggle_mode");
        fs::create_dir_all(&temp_dir).unwrap();

        let todo_file = temp_dir.join("todo.txt");
        let ref_file = temp_dir.join("ref.txt");

        fs::write(&todo_file, "(A) Todo item +work id:todo-1").unwrap();
        fs::write(&ref_file, "Ref item +misc id:ref-1").unwrap();

        let todos = load_todos(todo_file.to_str().unwrap()).unwrap();
        let mut state = AppState::new(
            todos,
            String::new(),
            HashSet::new(),
            temp_dir.to_str().unwrap().to_string(),
        );

        assert_eq!(state.view_mode, ViewMode::Todo);
        assert_eq!(state.todos.len(), 1);
        assert_eq!(state.todos[0].description, "Todo item");

        // Toggle to ref mode
        state.handle_toggle_mode();
        assert_eq!(state.view_mode, ViewMode::Ref);
        assert_eq!(state.todos.len(), 1);
        assert_eq!(state.todos[0].description, "Ref item");
        assert_eq!(state.current_column, 0);
        assert_eq!(state.selected_in_column, 0);

        // Toggle back to todo mode
        state.handle_toggle_mode();
        assert_eq!(state.view_mode, ViewMode::Todo);
        assert_eq!(state.todos.len(), 1);
        assert_eq!(state.todos[0].description, "Todo item");

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_handle_toggle_mode_creates_ref_file() {
        let temp_dir = std::env::temp_dir().join("torudo_test_toggle_create");
        fs::create_dir_all(&temp_dir).unwrap();

        let todo_file = temp_dir.join("todo.txt");
        let ref_file = temp_dir.join("ref.txt");
        fs::remove_file(&ref_file).ok();

        fs::write(&todo_file, "Todo item +work id:todo-1").unwrap();

        let todos = load_todos(todo_file.to_str().unwrap()).unwrap();
        let mut state = AppState::new(
            todos,
            String::new(),
            HashSet::new(),
            temp_dir.to_str().unwrap().to_string(),
        );

        // Toggle to ref mode - should create ref.txt
        state.handle_toggle_mode();
        assert!(ref_file.exists());
        assert_eq!(state.view_mode, ViewMode::Ref);
        assert_eq!(state.todos.len(), 0);

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_active_file() {
        let todos = create_test_todos();
        let mut state = create_test_state(todos);
        state.todotxt_dir = "/tmp/todotxt".to_string();

        assert_eq!(state.active_file(), "/tmp/todotxt/todo.txt");

        state.view_mode = ViewMode::Ref;
        assert_eq!(state.active_file(), "/tmp/todotxt/ref.txt");
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
    fn test_get_current_project_name() {
        let todos = create_test_todos();
        let mut state = create_test_state(todos);

        // First column is "No Project"
        assert_eq!(state.get_current_project_name(), Some("No Project".to_string()));

        // Navigate to "personal"
        state.handle_navigation_key('l');
        assert_eq!(state.get_current_project_name(), Some("personal".to_string()));
    }

    #[test]
    fn test_build_prompt_without_md_file() {
        let temp_dir = std::env::temp_dir().join("test_build_prompt_no_md");
        fs::create_dir_all(temp_dir.join("todos")).unwrap();

        let todos = vec![Item {
            completed: false,
            priority: Some('A'),
            creation_date: None,
            completion_date: None,
            description: "Design auth module".to_string(),
            projects: vec!["myapp".to_string()],
            contexts: vec![],
            id: Some("abc-123".to_string()),
            line_number: 1,
        }];

        let state = create_test_state(todos);
        let todotxt_dir = temp_dir.to_str().unwrap();

        let (project, text) = state.build_prompt(todotxt_dir).unwrap();
        assert_eq!(project, "myapp");
        assert!(text.contains("Design auth module"));
        assert!(!text.contains("## Details"));

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_build_prompt_with_md_file() {
        let temp_dir = std::env::temp_dir().join("test_build_prompt_with_md");
        fs::create_dir_all(temp_dir.join("todos")).unwrap();

        let todos = vec![Item {
            completed: false,
            priority: Some('A'),
            creation_date: None,
            completion_date: None,
            description: "Design auth module".to_string(),
            projects: vec!["myapp".to_string()],
            contexts: vec![],
            id: Some("abc-456".to_string()),
            line_number: 1,
        }];

        // Create the md file
        let md_path = temp_dir.join("todos/abc-456.md");
        fs::write(&md_path, "## Requirements\n- OAuth2 support\n- JWT tokens\n").unwrap();

        let state = create_test_state(todos);
        let todotxt_dir = temp_dir.to_str().unwrap();

        let (project, text) = state.build_prompt(todotxt_dir).unwrap();
        assert_eq!(project, "myapp");
        assert!(text.contains("Design auth module"));
        assert!(text.contains("## Details"));
        assert!(text.contains("OAuth2 support"));

        fs::remove_dir_all(&temp_dir).ok();
    }

    fn create_test_plan_modal() -> PlanModal {
        PlanModal {
            plans: vec![
                Plan {
                    title: "Plan A".to_string(),
                    project_name: "proj1".to_string(),
                    slug: "slug-a".to_string(),
                    path: "/tmp/plan-a.md".to_string(),
                },
                Plan {
                    title: "Plan B".to_string(),
                    project_name: "proj2".to_string(),
                    slug: "slug-b".to_string(),
                    path: "/tmp/plan-b.md".to_string(),
                },
                Plan {
                    title: "Plan C".to_string(),
                    project_name: "proj3".to_string(),
                    slug: "slug-c".to_string(),
                    path: "/tmp/plan-c.md".to_string(),
                },
            ],
            selected: 0,
            checked: vec![false, false, false],
        }
    }

    #[test]
    fn test_plan_modal_navigation() {
        let todos = create_test_todos();
        let mut state = create_test_state(todos);
        state.plan_modal = Some(create_test_plan_modal());

        let todo_file = "/tmp/dummy.txt";

        // Move down
        state.handle_plan_modal_key('j', todo_file);
        assert_eq!(state.plan_modal.as_ref().unwrap().selected, 1);

        state.handle_plan_modal_key('j', todo_file);
        assert_eq!(state.plan_modal.as_ref().unwrap().selected, 2);

        // Should stay at last
        state.handle_plan_modal_key('j', todo_file);
        assert_eq!(state.plan_modal.as_ref().unwrap().selected, 2);

        // Move up
        state.handle_plan_modal_key('k', todo_file);
        assert_eq!(state.plan_modal.as_ref().unwrap().selected, 1);

        state.handle_plan_modal_key('k', todo_file);
        assert_eq!(state.plan_modal.as_ref().unwrap().selected, 0);

        // Should stay at first
        state.handle_plan_modal_key('k', todo_file);
        assert_eq!(state.plan_modal.as_ref().unwrap().selected, 0);
    }

    #[test]
    fn test_plan_modal_toggle() {
        let todos = create_test_todos();
        let mut state = create_test_state(todos);
        state.plan_modal = Some(create_test_plan_modal());

        let todo_file = "/tmp/dummy.txt";

        // Toggle first item
        state.handle_plan_modal_key(' ', todo_file);
        assert!(state.plan_modal.as_ref().unwrap().checked[0]);

        // Toggle again to uncheck
        state.handle_plan_modal_key(' ', todo_file);
        assert!(!state.plan_modal.as_ref().unwrap().checked[0]);

        // Move and toggle second
        state.handle_plan_modal_key('j', todo_file);
        state.handle_plan_modal_key(' ', todo_file);
        assert!(state.plan_modal.as_ref().unwrap().checked[1]);
    }

    #[test]
    fn test_plan_modal_cancel() {
        let todos = create_test_todos();
        let mut state = create_test_state(todos);
        state.plan_modal = Some(create_test_plan_modal());

        let todo_file = "/tmp/dummy.txt";
        state.handle_plan_modal_key('q', todo_file);
        assert!(state.plan_modal.is_none());
    }

    #[test]
    fn test_import_selected_plans() {
        let temp_dir = std::env::temp_dir().join("torudo_test_import");
        fs::create_dir_all(temp_dir.join("todos")).unwrap();

        let todo_file = temp_dir.join("todo.txt");
        fs::write(&todo_file, "(A) Existing task +work id:existing-1\n").unwrap();

        // Create plan source files
        let plan_file = temp_dir.join("plan-a.md");
        fs::write(&plan_file, "# Plan A details\n").unwrap();

        let mut state = create_test_state(vec![]);
        state.plan_modal = Some(PlanModal {
            plans: vec![
                Plan {
                    title: "Plan A".to_string(),
                    project_name: "proj1".to_string(),
                    slug: "slug-a".to_string(),
                    path: plan_file.to_str().unwrap().to_string(),
                },
                Plan {
                    title: "Plan B".to_string(),
                    project_name: "proj2".to_string(),
                    slug: "slug-b".to_string(),
                    path: "/nonexistent/plan-b.md".to_string(),
                },
            ],
            selected: 0,
            checked: vec![true, true],
        });

        state.import_selected_plans(todo_file.to_str().unwrap());

        let content = fs::read_to_string(&todo_file).unwrap();
        assert!(content.contains("Plan A +proj1 id:slug-a"));
        assert!(content.contains("Plan B +proj2 id:slug-b"));

        // Check md file was copied
        let md_dest = temp_dir.join("todos/slug-a.md");
        assert!(md_dest.exists());
        assert_eq!(
            fs::read_to_string(&md_dest).unwrap(),
            "# Plan A details\n"
        );

        assert!(state.plan_modal.is_none());
        assert!(state
            .status_message
            .as_ref()
            .unwrap()
            .contains("Imported 2"));

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_import_selected_plans_skips_existing() {
        let temp_dir = std::env::temp_dir().join("torudo_test_import_skip");
        fs::create_dir_all(temp_dir.join("todos")).unwrap();

        let todo_file = temp_dir.join("todo.txt");
        fs::write(
            &todo_file,
            "Existing plan +proj1 id:slug-a\n",
        )
        .unwrap();

        let mut state = create_test_state(vec![]);
        state.plan_modal = Some(PlanModal {
            plans: vec![Plan {
                title: "Plan A".to_string(),
                project_name: "proj1".to_string(),
                slug: "slug-a".to_string(),
                path: "/tmp/plan-a.md".to_string(),
            }],
            selected: 0,
            checked: vec![true],
        });

        state.import_selected_plans(todo_file.to_str().unwrap());

        // Should not duplicate
        let content = fs::read_to_string(&todo_file).unwrap();
        let count = content.matches("id:slug-a").count();
        assert_eq!(count, 1);
        assert!(state
            .status_message
            .as_ref()
            .unwrap()
            .contains("skipped 1"));

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_claude_available() {
        let mut state = create_test_state(vec![]);

        state.claude_available = false;
        assert!(!state.claude_available());

        state.claude_available = true;
        assert!(state.claude_available());
    }

    #[test]
    fn test_handle_launch_plan_sets_status_when_unavailable() {
        let todos = vec![Item {
            completed: false,
            priority: Some('A'),
            creation_date: None,
            completion_date: None,
            description: "Test task".to_string(),
            projects: vec!["proj".to_string()],
            contexts: vec![],
            id: Some("test-id".to_string()),
            line_number: 1,
        }];
        let mut state = create_test_state(todos);
        state.claude_available = false;

        state.handle_launch_plan("/tmp");
        assert_eq!(
            state.status_message.as_deref(),
            Some("claude CLI not found")
        );
    }

    #[test]
    fn test_handle_launch_implement_sets_status_when_unavailable() {
        let todos = vec![Item {
            completed: false,
            priority: Some('A'),
            creation_date: None,
            completion_date: None,
            description: "Test task".to_string(),
            projects: vec!["proj".to_string()],
            contexts: vec![],
            id: Some("test-id".to_string()),
            line_number: 1,
        }];
        let mut state = create_test_state(todos);
        state.claude_available = false;

        state.handle_launch_implement("/tmp");
        assert_eq!(
            state.status_message.as_deref(),
            Some("claude CLI not found")
        );
    }

    #[test]
    fn test_crmux_available() {
        let mut state = create_test_state(vec![]);

        // None means not available
        state.crmux_version = None;
        assert!(!state.crmux_available());

        // Any version means available
        state.crmux_version = Some((0, 10, 0));
        assert!(state.crmux_available());
    }

    #[test]
    fn test_crmux_supports_get_plans() {
        let mut state = create_test_state(vec![]);

        // None
        state.crmux_version = None;
        assert!(!state.crmux_supports_get_plans());

        // 0.10.x — available but no get-plans
        state.crmux_version = Some((0, 10, 0));
        assert!(!state.crmux_supports_get_plans());

        state.crmux_version = Some((0, 10, 9));
        assert!(!state.crmux_supports_get_plans());

        // 0.11.0 — supports get-plans
        state.crmux_version = Some((0, 11, 0));
        assert!(state.crmux_supports_get_plans());

        // Above 0.11.0
        state.crmux_version = Some((0, 12, 0));
        assert!(state.crmux_supports_get_plans());

        state.crmux_version = Some((1, 0, 0));
        assert!(state.crmux_supports_get_plans());
    }

    #[test]
    fn test_strip_frontmatter() {
        let content = "---\ncwd: /path/to/repo\n---\n# Content\nBody";
        assert_eq!(strip_frontmatter(content), "# Content\nBody");
    }

    #[test]
    fn test_strip_frontmatter_no_frontmatter() {
        let content = "# Just content";
        assert_eq!(strip_frontmatter(content), "# Just content");
    }

    #[test]
    fn test_build_prompt_strips_frontmatter() {
        let temp_dir = std::env::temp_dir().join("test_build_prompt_strip_fm");
        fs::create_dir_all(temp_dir.join("todos")).unwrap();

        let todos = vec![Item {
            completed: false,
            priority: Some('A'),
            creation_date: None,
            completion_date: None,
            description: "Test task".to_string(),
            projects: vec!["myapp".to_string()],
            contexts: vec![],
            id: Some("fm-test-1".to_string()),
            line_number: 1,
        }];

        let md_path = temp_dir.join("todos/fm-test-1.md");
        fs::write(&md_path, "---\ncwd: /path/to/repo\n---\n# Details here\n").unwrap();

        let state = create_test_state(todos);
        let todotxt_dir = temp_dir.to_str().unwrap();

        let (_project, text) = state.build_prompt(todotxt_dir).unwrap();
        assert!(!text.contains("cwd:"));
        assert!(!text.contains("---"));
        assert!(text.contains("# Details here"));

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_launch_claude_without_cwd_shows_error() {
        let temp_dir = std::env::temp_dir().join("test_launch_no_cwd");
        fs::create_dir_all(temp_dir.join("todos")).unwrap();

        let todos = vec![Item {
            completed: false,
            priority: Some('A'),
            creation_date: None,
            completion_date: None,
            description: "Test task".to_string(),
            projects: vec!["proj".to_string()],
            contexts: vec![],
            id: Some("no-cwd-1".to_string()),
            line_number: 1,
        }];

        // Create md file without cwd in frontmatter
        let md_path = temp_dir.join("todos/no-cwd-1.md");
        fs::write(&md_path, "# Just content\n").unwrap();

        let mut state = create_test_state(todos);
        state.claude_available = true;

        state.launch_claude(temp_dir.to_str().unwrap(), "plan", "clp");
        assert!(state
            .status_message
            .as_deref()
            .unwrap()
            .contains("cwd not set"));

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_parse_frontmatter_cwd_valid() {
        let content = "---\ncwd: /path/to/repo\n---\n# Content";
        assert_eq!(
            parse_frontmatter_cwd(content),
            Some("/path/to/repo".to_string())
        );
    }

    #[test]
    fn test_parse_frontmatter_cwd_missing() {
        let content = "---\ntitle: My Task\n---\n# Content";
        assert_eq!(parse_frontmatter_cwd(content), None);
    }

    #[test]
    fn test_parse_frontmatter_cwd_no_frontmatter() {
        let content = "# Just a heading\nSome content";
        assert_eq!(parse_frontmatter_cwd(content), None);
    }

    #[test]
    fn test_parse_frontmatter_cwd_empty() {
        assert_eq!(parse_frontmatter_cwd(""), None);
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

    #[test]
    fn test_sort_plans_by_mtime() {
        let temp_dir = std::env::temp_dir().join("torudo_test_sort_mtime");
        fs::create_dir_all(&temp_dir).unwrap();

        // Create files with different mtimes (write order gives ascending mtime)
        let path_old = temp_dir.join("old.md");
        let path_new = temp_dir.join("new.md");

        fs::write(&path_old, "old plan").unwrap();
        // Ensure different mtime by setting old file's mtime to the past
        let old_time = filetime::FileTime::from_unix_time(1000, 0);
        filetime::set_file_mtime(&path_old, old_time).unwrap();

        fs::write(&path_new, "new plan").unwrap();

        let mut plans = vec![
            Plan {
                title: "Old Plan".to_string(),
                project_name: "proj".to_string(),
                slug: "old".to_string(),
                path: path_old.to_str().unwrap().to_string(),
            },
            Plan {
                title: "New Plan".to_string(),
                project_name: "proj".to_string(),
                slug: "new".to_string(),
                path: path_new.to_str().unwrap().to_string(),
            },
        ];

        sort_plans_by_mtime(&mut plans);

        // New plan should come first (descending mtime)
        assert_eq!(plans[0].title, "New Plan");
        assert_eq!(plans[1].title, "Old Plan");

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_sort_plans_by_mtime_missing_file() {
        let mut plans = vec![
            Plan {
                title: "Missing".to_string(),
                project_name: "proj".to_string(),
                slug: "missing".to_string(),
                path: "/nonexistent/path.md".to_string(),
            },
            Plan {
                title: "Also Missing".to_string(),
                project_name: "proj".to_string(),
                slug: "also-missing".to_string(),
                path: "/also/nonexistent.md".to_string(),
            },
        ];

        // Should not panic
        sort_plans_by_mtime(&mut plans);
        assert_eq!(plans.len(), 2);
    }

    #[test]
    fn test_hide_project() {
        let todos = create_test_todos();
        let mut state = create_test_state(todos);

        // Initially all 4 projects visible
        assert_eq!(state.visible_project_names().len(), 4);

        // Hide "No Project"
        assert_eq!(state.current_column, 0);
        assert_eq!(state.project_names[state.current_column], "No Project");
        state.hide_current_project();

        // Should now show 3 projects
        let visible = state.visible_project_names();
        assert_eq!(visible.len(), 3);
        assert!(!visible.contains(&"No Project".to_string()));

        // hidden_projects should contain "No Project"
        assert!(state.hidden_projects.contains("No Project"));
    }

    #[test]
    fn test_show_all_projects() {
        let todos = create_test_todos();
        let mut state = create_test_state(todos);

        // Hide a project
        state.hide_current_project();
        assert_eq!(state.visible_project_names().len(), 3);

        // Show all
        state.show_all_projects();
        assert_eq!(state.visible_project_names().len(), 4);
        assert!(state.hidden_projects.is_empty());
    }

    #[test]
    fn test_hide_project_adjusts_column() {
        let todos = create_test_todos();
        let mut state = create_test_state(todos);

        // Navigate to last column
        state.current_column = state.project_names.len() - 1;
        state.hide_current_project();

        // current_column should be adjusted
        let visible = state.visible_project_names();
        assert!(state.current_column < visible.len());
    }

    #[test]
    fn test_hidden_projects_display() {
        let todos = create_test_todos();
        let mut state = create_test_state(todos);

        // No hidden projects
        assert_eq!(state.hidden_projects_display(), None);

        // Hide "No Project"
        state.hide_current_project();
        let display = state.hidden_projects_display().unwrap();
        assert!(display.contains("No Project"));
    }

    #[test]
    fn test_new_with_initial_hidden() {
        let todos = create_test_todos();
        let hidden: HashSet<String> = vec!["No Project".to_string()].into_iter().collect();
        let mut state = AppState::new(todos, "/tmp/nvim.sock".to_string(), hidden, "/tmp/todotxt".to_string());
        state.crmux_version = None;
        state.claude_available = false;

        let visible = state.visible_project_names();
        assert_eq!(visible.len(), 3);
        assert!(!visible.contains(&"No Project".to_string()));
    }
}
