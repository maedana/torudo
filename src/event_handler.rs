use crate::app_state::{AppState, ViewMode};
use crossterm::event::{Event, KeyCode};
use log::debug;
use notify::{Event as NotifyEvent, EventKind};
use std::sync::mpsc;
use std::time::{Duration, Instant};

pub struct EventHandler {
    last_reload_time: Option<Instant>,
    last_md_refresh_time: Option<Instant>,
    debounce_duration: Duration,
    pending_keys: Vec<char>,
}

impl EventHandler {
    pub const fn new() -> Self {
        Self {
            last_reload_time: None,
            last_md_refresh_time: None,
            debounce_duration: Duration::from_millis(200),
            pending_keys: Vec::new(),
        }
    }

    pub fn handle_file_watcher_events(
        &mut self,
        file_watcher_rx: &mpsc::Receiver<NotifyEvent>,
        state: &mut AppState,
        debug_mode: bool,
    ) {
        let mut should_reload = false;
        let mut should_refresh_counts = false;
        let mut should_refresh_md = false;
        let active_file = state.active_file();
        let active_file_path = std::path::Path::new(&active_file);

        while let Ok(event) = file_watcher_rx.try_recv() {
            let is_active_file_event = event
                .paths
                .iter()
                .any(|path| path.file_name() == active_file_path.file_name());
            let is_mode_file_event = event.paths.iter().any(|path| {
                path.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|name| ViewMode::ALL.iter().any(|m| m.filename() == name))
            });
            let is_todos_md_event = event.paths.iter().any(|path| {
                path.extension().and_then(|e| e.to_str()) == Some("md")
                    && path
                        .parent()
                        .and_then(|d| d.file_name())
                        .and_then(|n| n.to_str())
                        == Some("todos")
            });

            if is_active_file_event {
                if debug_mode {
                    debug!("Active file event detected: {:?}", event.kind);
                }
                if let EventKind::Modify(_) = event.kind {
                    should_reload = true;
                    if debug_mode {
                        debug!("File change event queued for reload");
                    }
                }
            } else if is_mode_file_event && matches!(event.kind, EventKind::Modify(_)) {
                should_refresh_counts = true;
                if debug_mode {
                    debug!("Non-active mode file changed, refresh counts only");
                }
            } else if is_todos_md_event
                && matches!(
                    event.kind,
                    EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                )
            {
                should_refresh_md = true;
                if debug_mode {
                    debug!("todos/*.md changed, refresh md previews only");
                }
            }
        }

        // Debounce functionality: execute reload after certain time since last reload
        if should_reload {
            let now = Instant::now();
            let should_perform_reload = match self.last_reload_time {
                None => true,
                Some(last_time) => now.duration_since(last_time) >= self.debounce_duration,
            };

            if should_perform_reload {
                if debug_mode {
                    debug!("Executing debounced reload of todos");
                }
                state.reload_todos(&active_file);
                self.last_reload_time = Some(now);
            } else if debug_mode {
                debug!("Skipping reload due to debounce (too recent)");
            }
        } else if should_refresh_counts {
            state.refresh_mode_counts();
        } else if should_refresh_md {
            let now = Instant::now();
            let should = self
                .last_md_refresh_time
                .is_none_or(|t| now.duration_since(t) >= self.debounce_duration);
            if should {
                state.refresh_md_previews();
                self.last_md_refresh_time = Some(now);
            } else if debug_mode {
                debug!("Skipping md refresh due to debounce");
            }
        }
    }

    pub fn handle_keyboard_event(
        &mut self,
        event: &Event,
        state: &mut AppState,
        todo_file: &str,
        debug_mode: bool,
    ) -> bool {
        if let Event::Key(key) = *event {
            // Handle help overlay keys when help is shown
            if state.show_help {
                match key.code {
                    KeyCode::Char('?' | 'q') | KeyCode::Esc => {
                        state.show_help = false;
                    }
                    _ => {}
                }
                return false;
            }

            // Handle plan modal keys when modal is open
            if state.plan_modal.is_some() {
                match key.code {
                    KeyCode::Char(c @ ('j' | 'k' | ' ' | 'q')) => {
                        state.handle_plan_modal_key(c, todo_file);
                    }
                    KeyCode::Enter => {
                        state.handle_plan_modal_key('\r', todo_file);
                    }
                    _ => {}
                }
                return false;
            }

            // Handle multi-stroke sequences
            if !self.pending_keys.is_empty() {
                self.handle_pending_sequence(key.code, state, todo_file, debug_mode);
                return false;
            }

            return self.handle_initial_key(key.code, state, debug_mode);
        }
        false // Continue running
    }

    #[allow(clippy::too_many_lines)]
    fn handle_pending_sequence(
        &mut self,
        code: KeyCode,
        state: &mut AppState,
        todo_file: &str,
        debug_mode: bool,
    ) {
        let KeyCode::Char(c) = code else {
            // Non-char key pressed during sequence - cancel
            self.pending_keys.clear();
            state.status_message = None;
            return;
        };

        self.pending_keys.push(c);
        let todotxt_dir = std::path::Path::new(todo_file)
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or(".");

        match self.pending_keys.as_slice() {
            ['c', 's'] => {
                // Intermediate state - wait for third key
                state.status_message = Some("cs → p: Plan | i: Impl | Esc: Cancel".to_string());
            }
            ['c', 's', 'p'] => {
                if debug_mode {
                    debug!("Send plan prompt requested (csp)");
                }
                state.handle_send_plan(todotxt_dir);
                self.pending_keys.clear();
            }
            ['c', 's', 'i'] => {
                if debug_mode {
                    debug!("Send implement prompt requested (csi)");
                }
                state.handle_send_implement(todotxt_dir);
                self.pending_keys.clear();
            }
            ['c', 'g'] => {
                // Intermediate state - wait for third key
                state.status_message = Some("cg → p: Plans | Esc: Cancel".to_string());
            }
            ['c', 'g', 'p'] if state.crmux_supports_get_plans() => {
                if debug_mode {
                    debug!("Get plans requested (cgp)");
                }
                state.handle_open_plan_modal();
                self.pending_keys.clear();
            }
            ['c', 'l'] => {
                // Intermediate state - wait for third key
                state.status_message = Some("cl → p: Plan | i: Impl | Esc: Cancel".to_string());
            }
            ['c', 'l', 'p'] => {
                if debug_mode {
                    debug!("Launch plan requested (clp)");
                }
                state.handle_launch_plan(todotxt_dir);
                self.pending_keys.clear();
            }
            ['c', 'l', 'i'] => {
                if debug_mode {
                    debug!("Launch implement requested (cli)");
                }
                state.handle_launch_implement(todotxt_dir);
                self.pending_keys.clear();
            }
            ['s', 't'] => {
                if debug_mode {
                    debug!("Send to todo (st)");
                }
                state.handle_send_to(ViewMode::Todo);
                self.pending_keys.clear();
                state.status_message = None;
            }
            ['s', 'r'] => {
                if debug_mode {
                    debug!("Send to ref (sr)");
                }
                state.handle_send_to(ViewMode::Ref);
                self.pending_keys.clear();
                state.status_message = None;
            }
            ['s', 'i'] => {
                if debug_mode {
                    debug!("Send to inbox (si)");
                }
                state.handle_send_to(ViewMode::Inbox);
                self.pending_keys.clear();
                state.status_message = None;
            }
            ['s', 's'] => {
                if debug_mode {
                    debug!("Send to someday (ss)");
                }
                state.handle_send_to(ViewMode::Someday);
                self.pending_keys.clear();
                state.status_message = None;
            }
            ['s', 'w'] => {
                if debug_mode {
                    debug!("Send to waiting (sw)");
                }
                state.handle_send_to(ViewMode::Waiting);
                self.pending_keys.clear();
                state.status_message = None;
            }
            ['p', c @ ('a'..='e')] => {
                let priority = c.to_ascii_uppercase();
                if debug_mode {
                    debug!("Set priority ({priority}) via p{c}");
                }
                state.handle_set_priority(Some(priority));
                self.pending_keys.clear();
                state.status_message = None;
            }
            ['p', 'x'] => {
                if debug_mode {
                    debug!("Clear priority (px)");
                }
                state.handle_set_priority(None);
                self.pending_keys.clear();
                state.status_message = None;
            }
            ['d', 'd'] => {
                if debug_mode {
                    debug!("Delete todo requested (dd)");
                }
                state.handle_delete_todo();
                self.pending_keys.clear();
                state.status_message = None;
            }
            _ => {
                if debug_mode {
                    debug!("Unknown key sequence: {:?}", self.pending_keys);
                }
                self.pending_keys.clear();
                state.status_message = None;
            }
        }
    }

    fn handle_initial_key(
        &mut self,
        code: KeyCode,
        state: &mut AppState,
        debug_mode: bool,
    ) -> bool {
        match code {
            KeyCode::Char('q') => {
                if debug_mode {
                    debug!("Quit command received");
                }
                return true; // Signal to quit
            }
            KeyCode::Char(c @ ('k' | 'j' | 'h' | 'l')) => {
                if debug_mode {
                    debug!("Navigation key pressed: {c}");
                }
                state.handle_navigation_key(c);
            }
            KeyCode::Char('x') if matches!(state.view_mode, ViewMode::Todo | ViewMode::Waiting) => {
                if debug_mode {
                    debug!("Complete todo command received");
                }
                let file = state.active_file();
                state.handle_complete_todo(&file);
            }
            KeyCode::Char('d') => {
                self.pending_keys.push('d');
                state.status_message = Some(build_d_submenu());
            }
            KeyCode::Char('s') => {
                self.pending_keys.push('s');
                state.status_message = Some(build_s_submenu(state));
            }
            KeyCode::Char('p') => {
                self.pending_keys.push('p');
                state.status_message = Some(build_p_submenu());
            }
            KeyCode::Tab => {
                if debug_mode {
                    debug!("Tab: next mode");
                }
                state.next_view_mode();
            }
            KeyCode::BackTab => {
                if debug_mode {
                    debug!("Shift+Tab: previous mode");
                }
                state.prev_view_mode();
            }
            KeyCode::Char('c')
                if state.view_mode == ViewMode::Todo
                    && (state.crmux_available() || state.claude_available()) =>
            {
                self.pending_keys.push('c');
                state.status_message = Some(build_c_submenu(state));
            }
            KeyCode::Char('o') => {
                if debug_mode {
                    debug!("Open URLs command received");
                }
                state.handle_open_urls();
            }
            KeyCode::Char('?') => {
                state.toggle_help();
            }
            _ => {}
        }
        false
    }
}

fn build_p_submenu() -> String {
    "p → a/b/c/d/e: Set (A-E) | x: Clear | Esc: Cancel".to_string()
}

fn build_d_submenu() -> String {
    "d → d: Delete | Esc: Cancel".to_string()
}

fn build_s_submenu(state: &AppState) -> String {
    let mut parts = vec!["s →".to_string()];
    for mode in ViewMode::ALL {
        if *mode != state.view_mode {
            parts.push(format!("{}: {}", mode.shortcut_key(), mode.label()));
        }
    }
    parts.push("Esc: Cancel".to_string());
    parts.join(" | ")
}

fn build_c_submenu(state: &AppState) -> String {
    let mut parts = vec!["c →".to_string()];
    if state.crmux_available() {
        parts.push("s: Send…".to_string());
    }
    if state.crmux_supports_get_plans() {
        parts.push("g: Get…".to_string());
    }
    if state.claude_available() {
        parts.push("l: Launch…".to_string());
    }
    parts.push("Esc: Cancel".to_string());
    parts.join(" | ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::todo::Item;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    use std::collections::HashMap;

    fn make_key_event(c: char) -> Event {
        Event::Key(KeyEvent {
            code: KeyCode::Char(c),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
    }

    fn create_test_state_with_crmux() -> crate::app_state::AppState {
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
        let mut state = crate::app_state::AppState::new(
            todos,
            "/tmp/nvim.sock".to_string(),
            "/tmp/todotxt".to_string(),
        );
        state.crmux_version = Some((0, 11, 0));
        state.claude_available = false;
        state
    }

    fn create_test_state_with_claude() -> crate::app_state::AppState {
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
        let mut state = crate::app_state::AppState::new(
            todos,
            "/tmp/nvim.sock".to_string(),
            "/tmp/todotxt".to_string(),
        );
        state.crmux_version = None;
        state.claude_available = true;
        state
    }

    #[test]
    fn test_cs_intermediate_state() {
        let mut handler = EventHandler::new();
        let mut state = create_test_state_with_crmux();
        let todo_file = "/tmp/dummy.txt";

        handler.handle_keyboard_event(&make_key_event('c'), &mut state, todo_file, false);
        assert!(state.status_message.is_some());

        handler.handle_keyboard_event(&make_key_event('s'), &mut state, todo_file, false);
        assert_eq!(handler.pending_keys.as_slice(), &['c', 's']);
    }

    #[test]
    fn test_three_stroke_csp_sends_plan() {
        let mut handler = EventHandler::new();
        let mut state = create_test_state_with_crmux();
        let todo_file = "/tmp/dummy.txt";

        handler.handle_keyboard_event(&make_key_event('c'), &mut state, todo_file, false);
        handler.handle_keyboard_event(&make_key_event('s'), &mut state, todo_file, false);
        handler.handle_keyboard_event(&make_key_event('p'), &mut state, todo_file, false);
        assert!(handler.pending_keys.is_empty());
    }

    #[test]
    fn test_three_stroke_csi_sends_implement() {
        let mut handler = EventHandler::new();
        let mut state = create_test_state_with_crmux();
        let todo_file = "/tmp/dummy.txt";

        handler.handle_keyboard_event(&make_key_event('c'), &mut state, todo_file, false);
        handler.handle_keyboard_event(&make_key_event('s'), &mut state, todo_file, false);
        handler.handle_keyboard_event(&make_key_event('i'), &mut state, todo_file, false);
        assert!(handler.pending_keys.is_empty());
    }

    #[test]
    fn test_cg_intermediate_state() {
        let mut handler = EventHandler::new();
        let mut state = create_test_state_with_crmux();
        let todo_file = "/tmp/dummy.txt";

        handler.handle_keyboard_event(&make_key_event('c'), &mut state, todo_file, false);
        handler.handle_keyboard_event(&make_key_event('g'), &mut state, todo_file, false);
        assert_eq!(handler.pending_keys.as_slice(), &['c', 'g']);
    }

    #[test]
    fn test_three_stroke_cgp_gets_plans() {
        let mut handler = EventHandler::new();
        let mut state = create_test_state_with_crmux();
        let todo_file = "/tmp/dummy.txt";

        handler.handle_keyboard_event(&make_key_event('c'), &mut state, todo_file, false);
        handler.handle_keyboard_event(&make_key_event('g'), &mut state, todo_file, false);
        handler.handle_keyboard_event(&make_key_event('p'), &mut state, todo_file, false);
        assert!(handler.pending_keys.is_empty());
    }

    #[test]
    fn test_cl_intermediate_state() {
        let mut handler = EventHandler::new();
        let mut state = create_test_state_with_crmux();
        state.claude_available = true;
        let todo_file = "/tmp/dummy.txt";

        handler.handle_keyboard_event(&make_key_event('c'), &mut state, todo_file, false);
        assert!(state.status_message.is_some());

        handler.handle_keyboard_event(&make_key_event('l'), &mut state, todo_file, false);
        assert_eq!(handler.pending_keys.as_slice(), &['c', 'l']);
    }

    #[test]
    fn test_three_stroke_clp_triggers_launch_plan() {
        let mut handler = EventHandler::new();
        let mut state = create_test_state_with_crmux();
        state.claude_available = false;
        let todo_file = "/tmp/dummy.txt";

        handler.handle_keyboard_event(&make_key_event('c'), &mut state, todo_file, false);
        handler.handle_keyboard_event(&make_key_event('l'), &mut state, todo_file, false);
        handler.handle_keyboard_event(&make_key_event('p'), &mut state, todo_file, false);

        assert!(handler.pending_keys.is_empty());
        assert_eq!(
            state.status_message.as_deref(),
            Some("claude CLI not found")
        );
    }

    #[test]
    fn test_three_stroke_cli_triggers_launch_implement() {
        let mut handler = EventHandler::new();
        let mut state = create_test_state_with_crmux();
        state.claude_available = false;
        let todo_file = "/tmp/dummy.txt";

        handler.handle_keyboard_event(&make_key_event('c'), &mut state, todo_file, false);
        handler.handle_keyboard_event(&make_key_event('l'), &mut state, todo_file, false);
        handler.handle_keyboard_event(&make_key_event('i'), &mut state, todo_file, false);

        assert!(handler.pending_keys.is_empty());
        assert_eq!(
            state.status_message.as_deref(),
            Some("claude CLI not found")
        );
    }

    #[test]
    fn test_d_key_shows_intermediate() {
        let mut handler = EventHandler::new();
        let mut state = create_test_state_with_crmux();
        let todo_file = "/tmp/dummy.txt";

        handler.handle_keyboard_event(&make_key_event('d'), &mut state, todo_file, false);
        assert_eq!(handler.pending_keys.as_slice(), &['d']);
        let msg = state.status_message.as_deref().unwrap();
        assert!(msg.contains("Delete"));
    }

    #[test]
    fn test_dd_sequence_deletes_todo() {
        use std::fs;
        let temp_dir = std::env::temp_dir().join("torudo_event_dd");
        fs::remove_dir_all(&temp_dir).ok();
        fs::create_dir_all(&temp_dir).unwrap();
        let todo_file_path = temp_dir.join("todo.txt");
        fs::write(&todo_file_path, "Zap me +proj id:zap-1\n").unwrap();

        let todos = crate::todo::load_todos(todo_file_path.to_str().unwrap()).unwrap();
        let mut state = crate::app_state::AppState::new(
            todos,
            String::new(),
            temp_dir.to_str().unwrap().to_string(),
        );

        let mut handler = EventHandler::new();
        handler.handle_keyboard_event(
            &make_key_event('d'),
            &mut state,
            todo_file_path.to_str().unwrap(),
            false,
        );
        handler.handle_keyboard_event(
            &make_key_event('d'),
            &mut state,
            todo_file_path.to_str().unwrap(),
            false,
        );

        assert!(handler.pending_keys.is_empty());
        assert!(state.status_message.is_none());
        let remaining = fs::read_to_string(&todo_file_path).unwrap();
        assert!(!remaining.contains("zap-1"));

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_d_then_other_cancels() {
        let mut handler = EventHandler::new();
        let mut state = create_test_state_with_crmux();
        let todo_file = "/tmp/dummy.txt";

        handler.handle_keyboard_event(&make_key_event('d'), &mut state, todo_file, false);
        handler.handle_keyboard_event(&make_key_event('x'), &mut state, todo_file, false);

        assert!(handler.pending_keys.is_empty());
        assert!(state.status_message.is_none());
    }

    #[test]
    fn test_invalid_sequence_clears() {
        let mut handler = EventHandler::new();
        let mut state = create_test_state_with_crmux();
        state.claude_available = true;
        let todo_file = "/tmp/dummy.txt";

        handler.handle_keyboard_event(&make_key_event('c'), &mut state, todo_file, false);
        handler.handle_keyboard_event(&make_key_event('l'), &mut state, todo_file, false);
        handler.handle_keyboard_event(&make_key_event('q'), &mut state, todo_file, false);

        assert!(handler.pending_keys.is_empty());
        assert!(state.status_message.is_none());
    }

    #[test]
    fn test_c_key_shows_submenu() {
        let mut handler = EventHandler::new();
        let mut state = create_test_state_with_crmux();
        let todo_file = "/tmp/dummy.txt";

        handler.handle_keyboard_event(&make_key_event('c'), &mut state, todo_file, false);
        let msg = state.status_message.as_deref().unwrap();
        assert!(msg.contains("c →"));
        assert!(msg.contains("s: Send…"));
        assert!(msg.contains("g: Get…"));
        assert_eq!(handler.pending_keys.as_slice(), &['c']);
    }

    #[test]
    fn test_c_key_available_with_claude_only() {
        let mut handler = EventHandler::new();
        let mut state = create_test_state_with_claude();
        let todo_file = "/tmp/dummy.txt";

        handler.handle_keyboard_event(&make_key_event('c'), &mut state, todo_file, false);
        let msg = state.status_message.as_deref().unwrap();
        assert!(msg.contains("c →"));
        assert!(msg.contains("l: Launch…"));
        assert_eq!(handler.pending_keys.as_slice(), &['c']);
    }

    #[test]
    fn test_question_mark_toggles_help() {
        let mut handler = EventHandler::new();
        let mut state = create_test_state_with_crmux();
        let todo_file = "/tmp/dummy.txt";

        assert!(!state.show_help);
        handler.handle_keyboard_event(&make_key_event('?'), &mut state, todo_file, false);
        assert!(state.show_help);
        handler.handle_keyboard_event(&make_key_event('?'), &mut state, todo_file, false);
        assert!(!state.show_help);
    }

    #[test]
    fn test_help_overlay_blocks_other_keys() {
        let mut handler = EventHandler::new();
        let mut state = create_test_state_with_crmux();
        let todo_file = "/tmp/dummy.txt";

        state.show_help = true;
        let initial_column = state.selected_in_column;
        handler.handle_keyboard_event(&make_key_event('j'), &mut state, todo_file, false);
        assert_eq!(state.selected_in_column, initial_column);
        assert!(state.show_help);
    }

    #[test]
    fn test_q_closes_help() {
        let mut handler = EventHandler::new();
        let mut state = create_test_state_with_crmux();
        let todo_file = "/tmp/dummy.txt";

        state.show_help = true;
        let quit =
            handler.handle_keyboard_event(&make_key_event('q'), &mut state, todo_file, false);
        assert!(!quit);
        assert!(!state.show_help);
    }

    #[test]
    fn test_esc_closes_help() {
        let mut handler = EventHandler::new();
        let mut state = create_test_state_with_crmux();
        let todo_file = "/tmp/dummy.txt";

        state.show_help = true;
        let esc_event = Event::Key(KeyEvent {
            code: KeyCode::Esc,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        });
        let quit = handler.handle_keyboard_event(&esc_event, &mut state, todo_file, false);
        assert!(!quit);
        assert!(!state.show_help);
    }

    fn make_tab_event() -> Event {
        Event::Key(KeyEvent {
            code: KeyCode::Tab,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
    }

    fn make_backtab_event() -> Event {
        Event::Key(KeyEvent {
            code: KeyCode::BackTab,
            modifiers: KeyModifiers::SHIFT,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
    }

    #[test]
    fn test_tab_switches_to_next_mode() {
        let mut handler = EventHandler::new();
        let mut state = create_test_state_with_crmux();
        let todo_file = "/tmp/dummy.txt";

        // ALL order: Inbox, Todo, Waiting, Ref, Someday. Initial mode is Todo
        assert_eq!(state.view_mode, ViewMode::Todo);
        handler.handle_keyboard_event(&make_tab_event(), &mut state, todo_file, false);
        assert_eq!(state.view_mode, ViewMode::Waiting);
    }

    #[test]
    fn test_backtab_switches_to_prev_mode() {
        let mut handler = EventHandler::new();
        let mut state = create_test_state_with_crmux();
        let todo_file = "/tmp/dummy.txt";

        assert_eq!(state.view_mode, ViewMode::Todo);
        handler.handle_keyboard_event(&make_backtab_event(), &mut state, todo_file, false);
        assert_eq!(state.view_mode, ViewMode::Inbox);
    }

    #[test]
    fn test_s_submenu_shows_targets() {
        let mut handler = EventHandler::new();
        let mut state = create_test_state_with_crmux();
        let todo_file = "/tmp/dummy.txt";

        // Pressing s in Todo mode shows all modes except Todo
        handler.handle_keyboard_event(&make_key_event('s'), &mut state, todo_file, false);
        let msg = state.status_message.as_deref().unwrap();
        assert!(msg.contains("s →"));
        assert!(!msg.contains("t: Todo")); // Current mode is not shown
        assert!(msg.contains("r: Ref"));
        assert!(msg.contains("i: Inbox"));
        assert!(msg.contains("s: Someday"));
        assert!(msg.contains("w: Waiting"));
    }

    #[test]
    fn test_s_submenu_from_inbox_mode() {
        let mut handler = EventHandler::new();
        let mut state = create_test_state_with_crmux();
        state.view_mode = ViewMode::Inbox;
        let todo_file = "/tmp/dummy.txt";

        handler.handle_keyboard_event(&make_key_event('s'), &mut state, todo_file, false);
        let msg = state.status_message.as_deref().unwrap();
        assert!(msg.contains("t: Todo")); // From Inbox mode, Todo is shown
        assert!(!msg.contains("i: Inbox")); // Current mode is hidden
    }

    #[test]
    fn test_s_submenu_order_matches_tab_order_from_todo() {
        // Tab order (ViewMode::ALL) is Inbox, Todo, Waiting, Ref, Someday
        // Pressing s in Todo mode yields the order without Todo = Inbox, Waiting, Ref, Someday
        let mut handler = EventHandler::new();
        let mut state = create_test_state_with_crmux();
        assert_eq!(state.view_mode, ViewMode::Todo);
        let todo_file = "/tmp/dummy.txt";

        handler.handle_keyboard_event(&make_key_event('s'), &mut state, todo_file, false);
        assert_eq!(
            state.status_message.as_deref().unwrap(),
            "s → | i: Inbox | w: Waiting | r: Ref | s: Someday | Esc: Cancel"
        );
    }

    #[test]
    fn test_s_submenu_order_matches_tab_order_from_inbox() {
        // Pressing s in Inbox mode yields the order without Inbox = Todo, Waiting, Ref, Someday
        let mut handler = EventHandler::new();
        let mut state = create_test_state_with_crmux();
        state.view_mode = ViewMode::Inbox;
        let todo_file = "/tmp/dummy.txt";

        handler.handle_keyboard_event(&make_key_event('s'), &mut state, todo_file, false);
        assert_eq!(
            state.status_message.as_deref().unwrap(),
            "s → | t: Todo | w: Waiting | r: Ref | s: Someday | Esc: Cancel"
        );
    }

    #[test]
    fn test_p_prefix_shows_submenu() {
        let mut handler = EventHandler::new();
        let mut state = create_test_state_with_crmux();
        let todo_file = "/tmp/dummy.txt";

        handler.handle_keyboard_event(&make_key_event('p'), &mut state, todo_file, false);

        assert_eq!(handler.pending_keys.as_slice(), &['p']);
        let msg = state.status_message.as_deref().unwrap();
        assert!(msg.contains("p →"));
        assert!(msg.contains("a/b/c/d/e"));
        assert!(msg.contains("x: Clear"));
    }

    #[test]
    fn test_pa_sets_priority_a() {
        let temp_dir = std::env::temp_dir().join("torudo_test_event_pa");
        std::fs::create_dir_all(&temp_dir).unwrap();
        let todo_file = temp_dir.join("todo.txt");
        std::fs::write(&todo_file, "Task +proj id:t1\n").unwrap();

        let todos = crate::todo::load_todos(todo_file.to_str().unwrap()).unwrap();
        let mut state = crate::app_state::AppState::new(
            todos,
            String::new(),
            temp_dir.to_str().unwrap().to_string(),
        );
        let mut handler = EventHandler::new();
        let todo_file_str = todo_file.to_str().unwrap();

        handler.handle_keyboard_event(&make_key_event('p'), &mut state, todo_file_str, false);
        handler.handle_keyboard_event(&make_key_event('a'), &mut state, todo_file_str, false);

        assert!(handler.pending_keys.is_empty());
        let content = std::fs::read_to_string(&todo_file).unwrap();
        assert!(
            content.starts_with("(A) "),
            "expected (A) prefix, got: {content}"
        );

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_px_clears_priority() {
        let temp_dir = std::env::temp_dir().join("torudo_test_event_px");
        std::fs::create_dir_all(&temp_dir).unwrap();
        let todo_file = temp_dir.join("todo.txt");
        std::fs::write(&todo_file, "(A) Task +proj id:t1\n").unwrap();

        let todos = crate::todo::load_todos(todo_file.to_str().unwrap()).unwrap();
        let mut state = crate::app_state::AppState::new(
            todos,
            String::new(),
            temp_dir.to_str().unwrap().to_string(),
        );
        let mut handler = EventHandler::new();
        let todo_file_str = todo_file.to_str().unwrap();

        handler.handle_keyboard_event(&make_key_event('p'), &mut state, todo_file_str, false);
        handler.handle_keyboard_event(&make_key_event('x'), &mut state, todo_file_str, false);

        assert!(handler.pending_keys.is_empty());
        let content = std::fs::read_to_string(&todo_file).unwrap();
        assert!(
            !content.starts_with('('),
            "expected priority cleared, got: {content}"
        );
        assert!(content.starts_with("Task "));

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_p_unknown_key_cancels() {
        let mut handler = EventHandler::new();
        let mut state = create_test_state_with_crmux();
        let todo_file = "/tmp/dummy.txt";

        handler.handle_keyboard_event(&make_key_event('p'), &mut state, todo_file, false);
        handler.handle_keyboard_event(&make_key_event('z'), &mut state, todo_file, false);

        assert!(handler.pending_keys.is_empty());
        assert!(state.status_message.is_none());
    }

    #[test]
    fn test_c_key_unavailable_without_crmux_or_claude() {
        let mut handler = EventHandler::new();
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
        let mut state = crate::app_state::AppState::new(
            todos,
            "/tmp/nvim.sock".to_string(),
            "/tmp/todotxt".to_string(),
        );
        state.crmux_version = None;
        state.claude_available = false;
        let todo_file = "/tmp/dummy.txt";

        handler.handle_keyboard_event(&make_key_event('c'), &mut state, todo_file, false);
        assert!(handler.pending_keys.is_empty());
        assert!(state.status_message.is_none());
    }

    #[test]
    fn test_x_completes_in_waiting_mode() {
        use std::fs;
        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path().to_str().unwrap().to_string();

        let waiting_path = format!("{dir_path}/waiting.txt");
        fs::write(&waiting_path, "Bob handles this +proj id:wait-x\n").unwrap();

        let mut state = crate::app_state::AppState::new(vec![], String::new(), dir_path.clone());
        state.set_view_mode(ViewMode::Waiting);

        let mut handler = EventHandler::new();
        let todo_file = format!("{dir_path}/todo.txt");
        handler.handle_keyboard_event(&make_key_event('x'), &mut state, &todo_file, false);

        let waiting_content = fs::read_to_string(&waiting_path).unwrap();
        assert!(
            !waiting_content.contains("wait-x"),
            "wait-x should be removed from waiting.txt: {waiting_content}"
        );

        let done_path = format!("{dir_path}/done.txt");
        let done_content = fs::read_to_string(&done_path).unwrap();
        assert!(
            done_content.contains("wait-x"),
            "wait-x should be in done.txt: {done_content}"
        );
        assert!(done_content.starts_with("x "));
    }

    #[test]
    fn test_x_uses_active_file_not_todo_file_arg() {
        use std::fs;
        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path().to_str().unwrap().to_string();

        let todo_path = format!("{dir_path}/todo.txt");
        let waiting_path = format!("{dir_path}/waiting.txt");
        fs::write(&todo_path, "Todo task +proj id:tod-1\n").unwrap();
        fs::write(&waiting_path, "Waiting task +proj id:wait-1\n").unwrap();

        let mut state = crate::app_state::AppState::new(vec![], String::new(), dir_path);
        state.set_view_mode(ViewMode::Waiting);

        let mut handler = EventHandler::new();
        handler.handle_keyboard_event(&make_key_event('x'), &mut state, &todo_path, false);

        let todo_content = fs::read_to_string(&todo_path).unwrap();
        assert!(
            todo_content.contains("tod-1"),
            "todo.txt must be untouched while in Waiting mode: {todo_content}"
        );
        let waiting_content = fs::read_to_string(&waiting_path).unwrap();
        assert!(
            !waiting_content.contains("wait-1"),
            "waiting.txt should have lost wait-1: {waiting_content}"
        );
    }

    #[test]
    fn test_x_ignored_in_inbox_mode() {
        use std::fs;
        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path().to_str().unwrap().to_string();

        let inbox_path = format!("{dir_path}/inbox.txt");
        fs::write(&inbox_path, "Inbox idea +proj id:inb-x\n").unwrap();

        let mut state = crate::app_state::AppState::new(vec![], String::new(), dir_path.clone());
        state.set_view_mode(ViewMode::Inbox);

        let mut handler = EventHandler::new();
        handler.handle_keyboard_event(&make_key_event('x'), &mut state, &inbox_path, false);

        let content = fs::read_to_string(&inbox_path).unwrap();
        assert!(
            content.contains("inb-x"),
            "x in Inbox should be no-op: {content}"
        );
        let done_path = format!("{dir_path}/done.txt");
        assert!(
            !std::path::Path::new(&done_path).exists(),
            "done.txt should not be created from Inbox"
        );
    }
}
