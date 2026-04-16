use crate::crmux::Plan;
use crate::md_preview::{compute_meta, md_path};
use crate::todo::{
    Item, add_missing_ids, append_todo, delete_todo, group_todos_by_project_owned,
    has_todo_with_id, load_todos, mark_complete, move_to_file, set_priority,
};
use crate::url::{extract_urls, open_urls};
use log::{debug, error};
use std::{
    collections::HashMap, fs, io::Write, os::unix::net::UnixStream, time::Duration,
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
    Inbox,
    Someday,
    Waiting,
}

impl ViewMode {
    pub const ALL: &[Self] = &[
        Self::Inbox,
        Self::Todo,
        Self::Waiting,
        Self::Ref,
        Self::Someday,
    ];
    pub const COUNT: usize = Self::ALL.len();

    pub const fn filename(self) -> &'static str {
        match self {
            Self::Todo => "todo.txt",
            Self::Ref => "ref.txt",
            Self::Inbox => "inbox.txt",
            Self::Someday => "someday.txt",
            Self::Waiting => "waiting.txt",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Todo => "Todo",
            Self::Ref => "Ref",
            Self::Inbox => "Inbox",
            Self::Someday => "Someday",
            Self::Waiting => "Waiting",
        }
    }

    pub const fn shortcut_key(self) -> char {
        match self {
            Self::Todo => 't',
            Self::Ref => 'r',
            Self::Inbox => 'i',
            Self::Someday => 's',
            Self::Waiting => 'w',
        }
    }
}

pub fn count_items_in_file(path: &str) -> usize {
    fs::read_to_string(path)
        .map(|c| c.lines().filter(|l| !l.trim().is_empty()).count())
        .unwrap_or(0)
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
    pub update_available: Option<String>,
    pub view_mode: ViewMode,
    pub mode_counts: [usize; ViewMode::COUNT],
}

impl AppState {
    fn build_nvim_rpc_payload(method: &str, params: Vec<rmpv::Value>) -> Vec<u8> {
        let request = rmpv::Value::Array(vec![
            rmpv::Value::Integer(0.into()), // type = Request
            rmpv::Value::Integer(1.into()), // msgid
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
        let file_path = md_path(&self.todotxt_dir, todo_id);
        let cmd = format!("e {file_path}");

        match self.send_nvim_rpc_command(&cmd) {
            Ok(()) => debug!("Sent nvim RPC command: {cmd}"),
            Err(e) => debug!("Failed to send nvim RPC command: {e}"),
        }
    }

    pub fn new(todos: Vec<Item>, nvim_socket: String, todotxt_dir: String) -> Self {
        let crmux_version = crate::crmux::detect();
        let claude_available = crate::claude::detect();

        let mut state = Self {
            todos,
            grouped_todos: HashMap::new(),
            project_names: Vec::new(),
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
            update_available: None,
            view_mode: ViewMode::Todo,
            mode_counts: [0; ViewMode::COUNT],
        };
        state.update_derived_state();
        state.refresh_mode_counts();
        state
    }

    pub fn refresh_mode_counts(&mut self) {
        for (i, mode) in ViewMode::ALL.iter().enumerate() {
            let path = format!("{}/{}", self.todotxt_dir, mode.filename());
            self.mode_counts[i] = count_items_in_file(&path);
        }
    }

    pub fn reload_todos(&mut self, todo_file: &str) {
        if let Err(e) = add_missing_ids(todo_file) {
            error!("Failed to add missing IDs on reload: {e}");
        }
        match load_todos(todo_file) {
            Ok(new_todos) => {
                debug!("Reloaded {} todos from file", new_todos.len());
                self.todos = new_todos;
                self.update_derived_state();
                self.refresh_mode_counts();
            }
            Err(e) => error!("Failed to reload todos: {e}"),
        }
    }

    pub fn refresh_md_previews(&mut self) {
        self.refresh_md_meta();
        self.grouped_todos = group_todos_by_project_owned(&self.todos);
    }

    fn refresh_md_meta(&mut self) {
        if !matches!(self.view_mode, ViewMode::Todo | ViewMode::Waiting) {
            for t in &mut self.todos {
                t.md_meta = None;
            }
            return;
        }
        for t in &mut self.todos {
            let Some(id) = t.id.as_deref() else { continue };
            t.md_meta = compute_meta(&self.todotxt_dir, id);
        }
    }

    fn update_derived_state(&mut self) {
        self.refresh_md_meta();
        self.grouped_todos = group_todos_by_project_owned(&self.todos);
        self.project_names = self.grouped_todos.keys().cloned().collect();
        self.project_names.sort();

        let visible = &self.project_names;
        if self.current_column >= visible.len() {
            self.current_column = visible.len().saturating_sub(1);
        }
        if let Some(current_project_name) = visible.get(self.current_column)
            && let Some(current_todos) = self.grouped_todos.get(current_project_name)
        {
            if self.selected_in_column >= current_todos.len() {
                self.selected_in_column = current_todos.len().saturating_sub(1);
            }
            if let Some(selected_todo) = current_todos.get(self.selected_in_column)
                && let Some(todo_id) = &selected_todo.id
            {
                self.send_vim_command(todo_id);
            }
        }
    }

    pub fn get_current_todo(&self) -> Option<&Item> {
        let visible = &self.project_names;
        let current_project_name = visible.get(self.current_column)?;
        let current_todos = self.grouped_todos.get(current_project_name)?;
        current_todos.get(self.selected_in_column)
    }

    pub fn get_current_todo_id(&self) -> Option<&str> {
        self.get_current_todo()?.id.as_deref()
    }

    pub fn handle_navigation_key(&mut self, key_char: char) {
        self.status_message = None;
        let visible = &self.project_names;
        match key_char {
            'k' => {
                if let Some(current_project_name) = visible.get(self.current_column)
                    && let Some(current_todos) = self.grouped_todos.get(current_project_name)
                    && !current_todos.is_empty()
                {
                    let len = current_todos.len();
                    let new_idx = (self.selected_in_column + len - 1) % len;
                    if new_idx != self.selected_in_column {
                        self.selected_in_column = new_idx;
                        if let Some(todo_id) = self.get_current_todo_id() {
                            self.send_vim_command(todo_id);
                        }
                    }
                }
            }
            'j' => {
                if let Some(current_project_name) = visible.get(self.current_column)
                    && let Some(current_todos) = self.grouped_todos.get(current_project_name)
                    && !current_todos.is_empty()
                {
                    let len = current_todos.len();
                    let new_idx = (self.selected_in_column + 1) % len;
                    if new_idx != self.selected_in_column {
                        self.selected_in_column = new_idx;
                        if let Some(todo_id) = self.get_current_todo_id() {
                            self.send_vim_command(todo_id);
                        }
                    }
                }
            }
            'h' => {
                if !visible.is_empty() {
                    let len = visible.len();
                    let new_col = (self.current_column + len - 1) % len;
                    if new_col != self.current_column {
                        self.current_column = new_col;
                        self.selected_in_column = 0;
                        self.scroll_offset = 0;
                        if let Some(todo_id) = self.get_current_todo_id() {
                            self.send_vim_command(todo_id);
                        }
                    }
                }
            }
            'l' => {
                if !visible.is_empty() {
                    let new_col = (self.current_column + 1) % visible.len();
                    if new_col != self.current_column {
                        self.current_column = new_col;
                        self.selected_in_column = 0;
                        self.scroll_offset = 0;
                        if let Some(todo_id) = self.get_current_todo_id() {
                            self.send_vim_command(todo_id);
                        }
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

    pub fn handle_delete_todo(&mut self) {
        let file = self.active_file();
        let Some(todo_id) = self.get_current_todo_id().map(str::to_string) else {
            return;
        };
        debug!("Attempting to delete todo: {todo_id}");
        match delete_todo(&file, &todo_id) {
            Ok(true) => {
                let path = md_path(&self.todotxt_dir, &todo_id);
                if let Err(e) = fs::remove_file(&path)
                    && e.kind() != std::io::ErrorKind::NotFound
                {
                    debug!("Failed to remove md file {path}: {e}");
                }
                self.reload_todos(&file);
            }
            Ok(false) => {}
            Err(e) => error!("Failed to delete todo: {e}"),
        }
    }

    pub fn handle_set_priority(&mut self, priority: Option<char>) {
        let file = self.active_file();
        let Some(id) = self.get_current_todo_id().map(str::to_string) else {
            return;
        };
        debug!("Setting priority {priority:?} on {id}");
        match set_priority(&file, &id, priority) {
            Ok(()) => self.reload_todos(&file),
            Err(e) => error!("Failed to set priority: {e}"),
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
                    self.status_message = Some(format!(
                        "Opened {} URL(s), {failures} failed",
                        count - failures
                    ));
                }
            }
        }
    }

    pub const fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }

    pub fn active_file(&self) -> String {
        format!("{}/{}", self.todotxt_dir, self.view_mode.filename())
    }

    pub fn current_mode_index(&self) -> usize {
        ViewMode::ALL
            .iter()
            .position(|m| *m == self.view_mode)
            .unwrap_or(0)
    }

    pub fn next_view_mode(&mut self) {
        let next_idx = (self.current_mode_index() + 1) % ViewMode::COUNT;
        self.set_view_mode(ViewMode::ALL[next_idx]);
    }

    pub fn prev_view_mode(&mut self) {
        let prev_idx = (self.current_mode_index() + ViewMode::COUNT - 1) % ViewMode::COUNT;
        self.set_view_mode(ViewMode::ALL[prev_idx]);
    }

    pub fn set_view_mode(&mut self, mode: ViewMode) {
        if self.view_mode == mode {
            return;
        }
        self.view_mode = mode;
        let file = self.active_file();
        if !std::path::Path::new(&file).exists()
            && let Err(e) = std::fs::write(&file, "")
        {
            error!("Failed to create {file}: {e}");
            return;
        }
        self.reload_todos(&file);
        self.current_column = 0;
        self.selected_in_column = 0;
        self.scroll_offset = 0;
    }

    pub fn handle_send_to(&mut self, target_mode: ViewMode) {
        if target_mode == self.view_mode {
            return;
        }
        let source_file = self.active_file();
        if let Some(todo_id) = self.get_current_todo_id() {
            let target_name = target_mode.filename();
            let target_file = format!("{}/{target_name}", self.todotxt_dir);
            debug!("Attempting to move item to {target_name}: {todo_id}");
            match move_to_file(&source_file, &target_file, todo_id) {
                Ok(()) => {
                    debug!("Successfully moved item to {target_name}: {todo_id}");
                    self.reload_todos(&source_file);
                }
                Err(e) => error!("Failed to move item to {target_name}: {e}"),
            }
        }
    }

    pub fn send_initial_vim_command(&self) {
        if let Some(todo_id) = self.get_current_todo_id() {
            self.send_vim_command(todo_id);
        }
    }

    pub fn get_current_project_name(&self) -> Option<String> {
        self.project_names.get(self.current_column).cloned()
    }

    fn get_current_todo_description(&self) -> Option<String> {
        let project_name = self.project_names.get(self.current_column)?;
        let todos = self.grouped_todos.get(project_name)?;
        let todo = todos.get(self.selected_in_column)?;
        Some(todo.description.clone())
    }

    fn build_prompt(&self, todotxt_dir: &str) -> Option<(String, String)> {
        let project = self.get_current_project_name()?;
        let todo_id = self.get_current_todo_id()?;
        let description = self.get_current_todo_description()?;

        let path = md_path(todotxt_dir, todo_id);
        let md_content = std::fs::read_to_string(&path).unwrap_or_default();
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
            let dest = md_path(todotxt_dir, &plan.slug);
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
        let path = md_path(todotxt_dir, &todo_id);
        let md_content = std::fs::read_to_string(&path).unwrap_or_default();
        let Some(cwd) = parse_frontmatter_cwd(&md_content) else {
            self.status_message = Some(format!("cwd not set in {todo_id}.md frontmatter"));
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
        let mut state = AppState::new(
            todos,
            "/tmp/nvim.sock".to_string(),
            "/tmp/todotxt".to_string(),
        );
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
                key_values: HashMap::new(),
                line_number: 1,
                md_meta: None,
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
                key_values: HashMap::new(),
                line_number: 2,
                md_meta: None,
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
                key_values: HashMap::new(),
                line_number: 3,
                md_meta: None,
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
                key_values: HashMap::new(),
                line_number: 4,
                md_meta: None,
            },
        ]
    }

    #[test]
    fn test_app_state_new() {
        let todos = create_test_todos();
        let state = AppState::new(
            todos,
            "/tmp/nvim.sock".to_string(),
            "/tmp/todotxt".to_string(),
        );

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
                key_values: HashMap::new(),
                line_number: 1,
                md_meta: None,
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
                key_values: HashMap::new(),
                line_number: 2,
                md_meta: None,
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

        // Move down again from last item (should cycle to first)
        state.handle_navigation_key('j');
        assert_eq!(state.selected_in_column, 0);
        assert_eq!(state.get_current_todo_id(), Some("task-1"));

        // Move up with 'k' from first item (should cycle to last)
        state.handle_navigation_key('k');
        assert_eq!(state.selected_in_column, 1);
        assert_eq!(state.get_current_todo_id(), Some("task-2"));

        // Move up with 'k' back to first
        state.handle_navigation_key('k');
        assert_eq!(state.selected_in_column, 0);
        assert_eq!(state.get_current_todo_id(), Some("task-1"));
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
    fn test_handle_navigation_key_horizontal_cycle() {
        let todos = create_test_todos();
        let mut state = create_test_state(todos);

        let last_column = state.project_names.len() - 1;

        // Move left from first column (should cycle to last column)
        assert_eq!(state.current_column, 0);
        state.handle_navigation_key('h');
        assert_eq!(state.current_column, last_column);

        // Move right from last column (should cycle to first column)
        state.handle_navigation_key('l');
        assert_eq!(state.current_column, 0);
    }

    #[test]
    fn test_handle_navigation_key_single_item_noop() {
        let todos = vec![Item {
            completed: false,
            priority: None,
            creation_date: None,
            completion_date: None,
            description: "Only".to_string(),
            projects: vec!["work".to_string()],
            contexts: vec![],
            id: Some("only-1".to_string()),
            key_values: HashMap::new(),
            line_number: 1,
            md_meta: None,
        }];
        let mut state = create_test_state(todos);

        // Single column, single item — j/k/h/l all effectively no-op
        assert_eq!(state.current_column, 0);
        assert_eq!(state.selected_in_column, 0);

        state.handle_navigation_key('j');
        assert_eq!(state.selected_in_column, 0);
        state.handle_navigation_key('k');
        assert_eq!(state.selected_in_column, 0);
        state.handle_navigation_key('h');
        assert_eq!(state.current_column, 0);
        state.handle_navigation_key('l');
        assert_eq!(state.current_column, 0);
    }

    #[test]
    fn test_reload_todos_success() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_app_state_reload.txt");

        let initial_content = r"(A) Initial task +work @office id:initial-1";
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
            key_values: HashMap::new(),
            line_number: 1,
            md_meta: None,
        }];

        let mut state = create_test_state(initial_todos);
        assert_eq!(state.todos.len(), 1);

        // Update file content
        let new_content = r"(A) Initial task +work @office id:initial-1
(B) New task +personal @home id:new-1";
        fs::write(&test_file, new_content).unwrap();

        // Reload and verify
        state.reload_todos(test_file.to_str().unwrap());
        assert_eq!(state.todos.len(), 2);
        assert_eq!(state.project_names.len(), 2); // "personal", "work"

        fs::remove_file(&test_file).ok();
    }

    #[test]
    fn test_reload_todos_assigns_missing_ids() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_app_state_reload_assigns_ids.txt");

        // Start with a file containing a line WITHOUT id:
        let initial_content = "(A) Fresh task +work @office";
        fs::write(&test_file, initial_content).unwrap();

        let mut state = create_test_state(vec![]);
        state.reload_todos(test_file.to_str().unwrap());

        // The file on disk should now contain id:<uuid> for the missing line
        let written = fs::read_to_string(&test_file).unwrap();
        assert!(
            written.contains("id:"),
            "expected file to contain id: tag after reload, got: {written}"
        );

        // And the loaded Item should have Some(id)
        assert_eq!(state.todos.len(), 1);
        assert!(
            state.todos[0].id.is_some(),
            "expected loaded todo to have an id, got: {:?}",
            state.todos[0].id
        );

        fs::remove_file(&test_file).ok();
    }

    #[test]
    fn test_handle_complete_todo() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_app_state_complete.txt");

        let content = r"(A) Task to complete +work @office id:complete-me
(B) Other task +work @office id:keep-me";
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
                key_values: HashMap::new(),
                line_number: 1,
                md_meta: None,
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
                key_values: HashMap::new(),
                line_number: 2,
                md_meta: None,
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
        assert!(
            !state
                .todos
                .iter()
                .filter_map(|t| t.id.as_ref())
                .any(|id| id == &current_id)
        );

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
    fn test_handle_delete_todo_removes_line_and_md() {
        let temp_dir = std::env::temp_dir().join("torudo_test_delete_md");
        fs::remove_dir_all(&temp_dir).ok();
        fs::create_dir_all(temp_dir.join("todos")).unwrap();
        let todo_file = temp_dir.join("todo.txt");
        fs::write(
            &todo_file,
            "Task one +work id:del-a\nTask two +work id:del-b\n",
        )
        .unwrap();
        let md_path = temp_dir.join("todos/del-a.md");
        fs::write(&md_path, "# Detail for del-a\n").unwrap();

        let todos = load_todos(todo_file.to_str().unwrap()).unwrap();
        let mut state = AppState::new(todos, String::new(), temp_dir.to_str().unwrap().to_string());

        let current = state.get_current_todo_id().unwrap().to_string();
        assert_eq!(current, "del-a");
        state.handle_delete_todo();

        let remaining = fs::read_to_string(&todo_file).unwrap();
        assert!(!remaining.contains("del-a"));
        assert!(remaining.contains("del-b"));
        assert!(!md_path.exists(), "md file should be deleted");
        assert_eq!(state.todos.len(), 1);

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_handle_delete_todo_without_md_succeeds() {
        let temp_dir = std::env::temp_dir().join("torudo_test_delete_no_md");
        fs::remove_dir_all(&temp_dir).ok();
        fs::create_dir_all(&temp_dir).unwrap();
        let todo_file = temp_dir.join("todo.txt");
        fs::write(&todo_file, "Solo task +misc id:only-1\n").unwrap();

        let todos = load_todos(todo_file.to_str().unwrap()).unwrap();
        let mut state = AppState::new(todos, String::new(), temp_dir.to_str().unwrap().to_string());

        state.handle_delete_todo();

        let remaining = fs::read_to_string(&todo_file).unwrap();
        assert!(!remaining.contains("only-1"));
        assert_eq!(state.todos.len(), 0);

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_handle_delete_todo_operates_on_active_file() {
        let temp_dir = std::env::temp_dir().join("torudo_test_delete_ref");
        fs::remove_dir_all(&temp_dir).ok();
        fs::create_dir_all(&temp_dir).unwrap();
        let todo_file = temp_dir.join("todo.txt");
        let ref_file = temp_dir.join("ref.txt");
        fs::write(&todo_file, "Keep me +work id:keep-todo\n").unwrap();
        fs::write(&ref_file, "Ref item +misc id:ref-kill\n").unwrap();

        let todos = load_todos(todo_file.to_str().unwrap()).unwrap();
        let mut state = AppState::new(todos, String::new(), temp_dir.to_str().unwrap().to_string());
        state.set_view_mode(ViewMode::Ref);

        state.handle_delete_todo();

        let ref_after = fs::read_to_string(&ref_file).unwrap();
        assert!(!ref_after.contains("ref-kill"));
        let todo_after = fs::read_to_string(&todo_file).unwrap();
        assert!(
            todo_after.contains("keep-todo"),
            "todo.txt must not be touched"
        );

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_handle_set_priority_sets_priority() {
        let temp_dir = std::env::temp_dir().join("torudo_test_set_priority_app");
        fs::create_dir_all(&temp_dir).unwrap();
        let todo_file = temp_dir.join("todo.txt");
        fs::write(&todo_file, "Task one +work id:task-1\n").unwrap();

        let todos = load_todos(todo_file.to_str().unwrap()).unwrap();
        let mut state = AppState::new(todos, String::new(), temp_dir.to_str().unwrap().to_string());

        state.handle_set_priority(Some('C'));

        let current = state.get_current_todo().expect("should have current todo");
        assert_eq!(current.priority, Some('C'));

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_handle_set_priority_clears_priority() {
        let temp_dir = std::env::temp_dir().join("torudo_test_clear_priority_app");
        fs::create_dir_all(&temp_dir).unwrap();
        let todo_file = temp_dir.join("todo.txt");
        fs::write(&todo_file, "(A) Task one +work id:task-1\n").unwrap();

        let todos = load_todos(todo_file.to_str().unwrap()).unwrap();
        let mut state = AppState::new(todos, String::new(), temp_dir.to_str().unwrap().to_string());

        state.handle_set_priority(None);

        let current = state.get_current_todo().expect("should have current todo");
        assert_eq!(current.priority, None);

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_set_view_mode() {
        let temp_dir = std::env::temp_dir().join("torudo_test_toggle_mode");
        fs::create_dir_all(&temp_dir).unwrap();

        let todo_file = temp_dir.join("todo.txt");
        let ref_file = temp_dir.join("ref.txt");

        fs::write(&todo_file, "(A) Todo item +work id:todo-1").unwrap();
        fs::write(&ref_file, "Ref item +misc id:ref-1").unwrap();

        let todos = load_todos(todo_file.to_str().unwrap()).unwrap();
        let mut state = AppState::new(todos, String::new(), temp_dir.to_str().unwrap().to_string());

        assert_eq!(state.view_mode, ViewMode::Todo);
        assert_eq!(state.todos.len(), 1);
        assert_eq!(state.todos[0].description, "Todo item");

        // Switch to ref mode
        state.set_view_mode(ViewMode::Ref);
        assert_eq!(state.view_mode, ViewMode::Ref);
        assert_eq!(state.todos.len(), 1);
        assert_eq!(state.todos[0].description, "Ref item");
        assert_eq!(state.current_column, 0);
        assert_eq!(state.selected_in_column, 0);

        // Switch back to todo mode
        state.set_view_mode(ViewMode::Todo);
        assert_eq!(state.view_mode, ViewMode::Todo);
        assert_eq!(state.todos.len(), 1);
        assert_eq!(state.todos[0].description, "Todo item");

        // Same mode is no-op
        state.set_view_mode(ViewMode::Todo);
        assert_eq!(state.view_mode, ViewMode::Todo);

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_set_view_mode_creates_ref_file() {
        let temp_dir = std::env::temp_dir().join("torudo_test_toggle_create");
        fs::create_dir_all(&temp_dir).unwrap();

        let todo_file = temp_dir.join("todo.txt");
        let ref_file = temp_dir.join("ref.txt");
        fs::remove_file(&ref_file).ok();

        fs::write(&todo_file, "Todo item +work id:todo-1").unwrap();

        let todos = load_todos(todo_file.to_str().unwrap()).unwrap();
        let mut state = AppState::new(todos, String::new(), temp_dir.to_str().unwrap().to_string());

        // Switch to ref mode - should create ref.txt
        state.set_view_mode(ViewMode::Ref);
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

        state.view_mode = ViewMode::Inbox;
        assert_eq!(state.active_file(), "/tmp/todotxt/inbox.txt");

        state.view_mode = ViewMode::Someday;
        assert_eq!(state.active_file(), "/tmp/todotxt/someday.txt");

        state.view_mode = ViewMode::Waiting;
        assert_eq!(state.active_file(), "/tmp/todotxt/waiting.txt");
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
        assert_eq!(
            state.get_current_project_name(),
            Some("No Project".to_string())
        );

        // Navigate to "personal"
        state.handle_navigation_key('l');
        assert_eq!(
            state.get_current_project_name(),
            Some("personal".to_string())
        );
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
            key_values: HashMap::new(),
            line_number: 1,
            md_meta: None,
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
            key_values: HashMap::new(),
            line_number: 1,
            md_meta: None,
        }];

        // Create the md file
        let md_path = temp_dir.join("todos/abc-456.md");
        fs::write(
            &md_path,
            "## Requirements\n- OAuth2 support\n- JWT tokens\n",
        )
        .unwrap();

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
        assert_eq!(fs::read_to_string(&md_dest).unwrap(), "# Plan A details\n");

        assert!(state.plan_modal.is_none());
        assert!(
            state
                .status_message
                .as_ref()
                .unwrap()
                .contains("Imported 2")
        );

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_import_selected_plans_skips_existing() {
        let temp_dir = std::env::temp_dir().join("torudo_test_import_skip");
        fs::create_dir_all(temp_dir.join("todos")).unwrap();

        let todo_file = temp_dir.join("todo.txt");
        fs::write(&todo_file, "Existing plan +proj1 id:slug-a\n").unwrap();

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
        assert!(state.status_message.as_ref().unwrap().contains("skipped 1"));

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
            key_values: HashMap::new(),
            line_number: 1,
            md_meta: None,
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
            key_values: HashMap::new(),
            line_number: 1,
            md_meta: None,
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
            key_values: HashMap::new(),
            line_number: 1,
            md_meta: None,
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
            key_values: HashMap::new(),
            line_number: 1,
            md_meta: None,
        }];

        // Create md file without cwd in frontmatter
        let md_path = temp_dir.join("todos/no-cwd-1.md");
        fs::write(&md_path, "# Just content\n").unwrap();

        let mut state = create_test_state(todos);
        state.claude_available = true;

        state.launch_claude(temp_dir.to_str().unwrap(), "plan", "clp");
        assert!(
            state
                .status_message
                .as_deref()
                .unwrap()
                .contains("cwd not set")
        );

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
    fn test_next_view_mode_cycles() {
        let temp_dir = std::env::temp_dir().join("torudo_test_next_mode");
        fs::create_dir_all(&temp_dir).unwrap();
        fs::write(temp_dir.join("todo.txt"), "").unwrap();

        let todos = load_todos(temp_dir.join("todo.txt").to_str().unwrap()).unwrap();
        let mut state = AppState::new(todos, String::new(), temp_dir.to_str().unwrap().to_string());

        // ALL order: Inbox, Todo, Waiting, Ref, Someday
        assert_eq!(state.view_mode, ViewMode::Todo);
        state.next_view_mode();
        assert_eq!(state.view_mode, ViewMode::Waiting);
        state.next_view_mode();
        assert_eq!(state.view_mode, ViewMode::Ref);
        state.next_view_mode();
        assert_eq!(state.view_mode, ViewMode::Someday);
        state.next_view_mode();
        assert_eq!(state.view_mode, ViewMode::Inbox); // wraps around
        state.next_view_mode();
        assert_eq!(state.view_mode, ViewMode::Todo);

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_prev_view_mode_cycles() {
        let temp_dir = std::env::temp_dir().join("torudo_test_prev_mode");
        fs::create_dir_all(&temp_dir).unwrap();
        fs::write(temp_dir.join("todo.txt"), "").unwrap();

        let todos = load_todos(temp_dir.join("todo.txt").to_str().unwrap()).unwrap();
        let mut state = AppState::new(todos, String::new(), temp_dir.to_str().unwrap().to_string());

        assert_eq!(state.view_mode, ViewMode::Todo);
        state.prev_view_mode();
        assert_eq!(state.view_mode, ViewMode::Inbox);
        state.prev_view_mode();
        assert_eq!(state.view_mode, ViewMode::Someday); // wraps around
        state.prev_view_mode();
        assert_eq!(state.view_mode, ViewMode::Ref);

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_view_mode_all_and_label() {
        assert_eq!(ViewMode::ALL.len(), 5);
        assert_eq!(ViewMode::Todo.label(), "Todo");
        assert_eq!(ViewMode::Ref.label(), "Ref");
        assert_eq!(ViewMode::Inbox.label(), "Inbox");
        assert_eq!(ViewMode::Someday.label(), "Someday");
        assert_eq!(ViewMode::Waiting.label(), "Waiting");
    }

    #[test]
    fn test_count_items_in_file() {
        let temp_dir = std::env::temp_dir().join("torudo_test_count");
        fs::create_dir_all(&temp_dir).unwrap();

        let file = temp_dir.join("test.txt");
        fs::write(
            &file,
            "(A) Item one +proj id:1\n\n(B) Item two +proj id:2\n",
        )
        .unwrap();
        assert_eq!(count_items_in_file(file.to_str().unwrap()), 2);

        let empty_file = temp_dir.join("empty.txt");
        fs::write(&empty_file, "").unwrap();
        assert_eq!(count_items_in_file(empty_file.to_str().unwrap()), 0);

        assert_eq!(count_items_in_file("/nonexistent/path.txt"), 0);

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_refresh_mode_counts() {
        let temp_dir = std::env::temp_dir().join("torudo_test_mode_counts");
        fs::create_dir_all(&temp_dir).unwrap();

        fs::write(
            temp_dir.join("todo.txt"),
            "(A) task +proj id:1\n(B) task2 +proj id:2\n",
        )
        .unwrap();
        fs::write(temp_dir.join("ref.txt"), "ref item +misc id:3\n").unwrap();
        fs::write(
            temp_dir.join("inbox.txt"),
            "idea1 id:4\nidea2 id:5\nidea3 id:6\n",
        )
        .unwrap();
        // someday.txt and waiting.txt don't exist

        let todos = load_todos(temp_dir.join("todo.txt").to_str().unwrap()).unwrap();
        let mut state = AppState::new(todos, String::new(), temp_dir.to_str().unwrap().to_string());
        state.refresh_mode_counts();

        let count_of = |mode: ViewMode| {
            let i = ViewMode::ALL.iter().position(|m| *m == mode).unwrap();
            state.mode_counts[i]
        };
        assert_eq!(count_of(ViewMode::Inbox), 3);
        assert_eq!(count_of(ViewMode::Todo), 2);
        assert_eq!(count_of(ViewMode::Waiting), 0);
        assert_eq!(count_of(ViewMode::Ref), 1);
        assert_eq!(count_of(ViewMode::Someday), 0);

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_refresh_md_meta_populates_in_waiting_mode() {
        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path().to_str().unwrap().to_string();

        fs::write(
            format!("{dir_path}/waiting.txt"),
            "Task waiting on Bob +proj id:wait-1\n",
        )
        .unwrap();
        fs::create_dir_all(format!("{dir_path}/todos")).unwrap();
        fs::write(
            format!("{dir_path}/todos/wait-1.md"),
            "- [x] done\n- [ ] open\n",
        )
        .unwrap();

        let mut state = AppState::new(vec![], String::new(), dir_path);
        state.set_view_mode(ViewMode::Waiting);

        let todo = state
            .todos
            .iter()
            .find(|t| t.id.as_deref() == Some("wait-1"))
            .expect("waiting todo loaded");
        let meta = todo
            .md_meta
            .as_ref()
            .expect("md_meta should be set in Waiting mode");
        assert_eq!(meta.stats, Some((1, 2)));
    }

    #[test]
    fn test_refresh_md_meta_cleared_for_inbox_mode() {
        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path().to_str().unwrap().to_string();

        fs::write(
            format!("{dir_path}/inbox.txt"),
            "Captured idea +proj id:inb-1\n",
        )
        .unwrap();
        fs::create_dir_all(format!("{dir_path}/todos")).unwrap();
        fs::write(
            format!("{dir_path}/todos/inb-1.md"),
            "- [x] done\n- [ ] open\n",
        )
        .unwrap();

        let mut state = AppState::new(vec![], String::new(), dir_path);
        state.set_view_mode(ViewMode::Inbox);

        let todo = state
            .todos
            .iter()
            .find(|t| t.id.as_deref() == Some("inb-1"))
            .expect("inbox todo loaded");
        assert!(
            todo.md_meta.is_none(),
            "md_meta should be None in non-Todo/Waiting modes"
        );
    }
}
