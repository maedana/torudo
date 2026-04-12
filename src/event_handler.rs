use crate::app_state::{AppState, ViewMode};
use crossterm::event::{Event, KeyCode};
use log::debug;
use notify::{Event as NotifyEvent, EventKind};
use std::sync::mpsc;
use std::time::{Duration, Instant};

pub struct EventHandler {
    last_reload_time: Option<Instant>,
    debounce_duration: Duration,
    pending_keys: Vec<char>,
}

impl EventHandler {
    pub const fn new() -> Self {
        Self {
            last_reload_time: None,
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
        let active_file = state.active_file();
        let active_file_path = std::path::Path::new(&active_file);
        while let Ok(event) = file_watcher_rx.try_recv() {
            let is_active_file_event = event
                .paths
                .iter()
                .any(|path| path.file_name() == active_file_path.file_name());

            if is_active_file_event {
                if debug_mode {
                    debug!("Active file event detected: {:?}", event.kind);
                }
                match event.kind {
                    EventKind::Modify(_) => {
                        should_reload = true;
                        if debug_mode {
                            debug!("File change event queued for reload");
                        }
                    }
                    _ => {
                        if debug_mode {
                            debug!("Ignoring file event: {:?}", event.kind);
                        }
                    }
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

            return self.handle_initial_key(key.code, state, todo_file, debug_mode);
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
                state.status_message =
                    Some("cs → p: Plan | i: Impl | Esc: Cancel".to_string());
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
                state.status_message =
                    Some("cg → p: Plans | Esc: Cancel".to_string());
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
                state.status_message =
                    Some("cl → p: Plan | i: Impl | Esc: Cancel".to_string());
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
            ['m', 't'] => {
                if debug_mode {
                    debug!("Switch to todo mode (mt)");
                }
                state.set_view_mode(ViewMode::Todo);
                self.pending_keys.clear();
                state.status_message = None;
            }
            ['m', 'r'] => {
                if debug_mode {
                    debug!("Switch to ref mode (mr)");
                }
                state.set_view_mode(ViewMode::Ref);
                self.pending_keys.clear();
                state.status_message = None;
            }
            ['m', 'i'] => {
                if debug_mode {
                    debug!("Switch to inbox mode (mi)");
                }
                state.set_view_mode(ViewMode::Inbox);
                self.pending_keys.clear();
                state.status_message = None;
            }
            ['m', 's'] => {
                if debug_mode {
                    debug!("Switch to someday mode (ms)");
                }
                state.set_view_mode(ViewMode::Someday);
                self.pending_keys.clear();
                state.status_message = None;
            }
            ['m', 'w'] => {
                if debug_mode {
                    debug!("Switch to waiting mode (mw)");
                }
                state.set_view_mode(ViewMode::Waiting);
                self.pending_keys.clear();
                state.status_message = None;
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
        todo_file: &str,
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
            KeyCode::Char('x') if state.view_mode == ViewMode::Todo => {
                if debug_mode {
                    debug!("Complete todo command received");
                }
                state.handle_complete_todo(todo_file);
            }
            KeyCode::Char('s') => {
                self.pending_keys.push('s');
                state.status_message = Some(build_s_submenu(state));
            }
            KeyCode::Char('m') => {
                self.pending_keys.push('m');
                state.status_message = Some("m → t: Todo | r: Ref | i: Inbox | s: Someday | w: Waiting | Esc: Cancel".to_string());
            }
            KeyCode::Char('c') if state.view_mode == ViewMode::Todo && (state.crmux_available() || state.claude_available()) => {
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

fn build_s_submenu(state: &AppState) -> String {
    let mut parts = vec!["s →".to_string()];
    if state.view_mode != ViewMode::Todo {
        parts.push("t: Todo".to_string());
    }
    if state.view_mode != ViewMode::Ref {
        parts.push("r: Ref".to_string());
    }
    if state.view_mode != ViewMode::Inbox {
        parts.push("i: Inbox".to_string());
    }
    if state.view_mode != ViewMode::Someday {
        parts.push("s: Someday".to_string());
    }
    if state.view_mode != ViewMode::Waiting {
        parts.push("w: Waiting".to_string());
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
            line_number: 1,
        }];
        let mut state = crate::app_state::AppState::new(todos, "/tmp/nvim.sock".to_string(), "/tmp/todotxt".to_string());
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
            line_number: 1,
        }];
        let mut state = crate::app_state::AppState::new(todos, "/tmp/nvim.sock".to_string(), "/tmp/todotxt".to_string());
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
        let quit = handler.handle_keyboard_event(&make_key_event('q'), &mut state, todo_file, false);
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

    #[test]
    fn test_mode_switch_to_inbox() {
        let mut handler = EventHandler::new();
        let mut state = create_test_state_with_crmux();
        let todo_file = "/tmp/dummy.txt";

        handler.handle_keyboard_event(&make_key_event('m'), &mut state, todo_file, false);
        let msg = state.status_message.as_deref().unwrap();
        assert!(msg.contains("i: Inbox"));
        assert!(msg.contains("s: Someday"));
        assert!(msg.contains("w: Waiting"));

        handler.handle_keyboard_event(&make_key_event('i'), &mut state, todo_file, false);
        assert!(handler.pending_keys.is_empty());
        assert_eq!(state.view_mode, ViewMode::Inbox);
    }

    #[test]
    fn test_mode_switch_to_someday() {
        let mut handler = EventHandler::new();
        let mut state = create_test_state_with_crmux();
        let todo_file = "/tmp/dummy.txt";

        handler.handle_keyboard_event(&make_key_event('m'), &mut state, todo_file, false);
        handler.handle_keyboard_event(&make_key_event('s'), &mut state, todo_file, false);
        assert!(handler.pending_keys.is_empty());
        assert_eq!(state.view_mode, ViewMode::Someday);
    }

    #[test]
    fn test_mode_switch_to_waiting() {
        let mut handler = EventHandler::new();
        let mut state = create_test_state_with_crmux();
        let todo_file = "/tmp/dummy.txt";

        handler.handle_keyboard_event(&make_key_event('m'), &mut state, todo_file, false);
        handler.handle_keyboard_event(&make_key_event('w'), &mut state, todo_file, false);
        assert!(handler.pending_keys.is_empty());
        assert_eq!(state.view_mode, ViewMode::Waiting);
    }

    #[test]
    fn test_s_submenu_shows_targets() {
        let mut handler = EventHandler::new();
        let mut state = create_test_state_with_crmux();
        let todo_file = "/tmp/dummy.txt";

        // Todoモードからsを押すと、Todo以外が表示される
        handler.handle_keyboard_event(&make_key_event('s'), &mut state, todo_file, false);
        let msg = state.status_message.as_deref().unwrap();
        assert!(msg.contains("s →"));
        assert!(!msg.contains("t: Todo")); // 現在のモードは表示しない
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
        assert!(msg.contains("t: Todo")); // Inboxモードからはtodoが表示される
        assert!(!msg.contains("i: Inbox")); // 現在のモードは非表示
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
            line_number: 1,
        }];
        let mut state = crate::app_state::AppState::new(todos, "/tmp/nvim.sock".to_string(), "/tmp/todotxt".to_string());
        state.crmux_version = None;
        state.claude_available = false;
        let todo_file = "/tmp/dummy.txt";

        handler.handle_keyboard_event(&make_key_event('c'), &mut state, todo_file, false);
        assert!(handler.pending_keys.is_empty());
        assert!(state.status_message.is_none());
    }
}
