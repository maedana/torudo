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

            // Handle hint mode input (Vimium-like f jump)
            if state.hint.is_some() {
                match key.code {
                    KeyCode::Char(c) if c.is_ascii_lowercase() => {
                        state.type_hint_char(c);
                    }
                    _ => state.exit_hint_mode(),
                }
                return false;
            }

            // Handle multi-stroke sequences
            if !self.pending_keys.is_empty() {
                self.handle_pending_sequence(key.code, state, debug_mode);
                return false;
            }

            return self.handle_initial_key(key.code, state, debug_mode);
        }
        false // Continue running
    }

    #[allow(clippy::too_many_lines)]
    fn handle_pending_sequence(&mut self, code: KeyCode, state: &mut AppState, debug_mode: bool) {
        let KeyCode::Char(c) = code else {
            // Non-char key pressed during sequence - cancel
            self.pending_keys.clear();
            state.status_message = None;
            return;
        };

        self.pending_keys.push(c);

        match self.pending_keys.as_slice() {
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
            ['t', 'i'] => {
                // Intermediate state - wait for third key
                state.status_message =
                    Some("ti → s: short | m: medium | l: long | Esc: Cancel".to_string());
            }
            ['t', 'i', 's'] => {
                if debug_mode {
                    debug!("Set time:short via tis");
                }
                state.handle_set_time("short");
                self.pending_keys.clear();
                state.status_message = None;
            }
            ['t', 'i', 'm'] => {
                if debug_mode {
                    debug!("Set time:medium via tim");
                }
                state.handle_set_time("medium");
                self.pending_keys.clear();
                state.status_message = None;
            }
            ['t', 'i', 'l'] => {
                if debug_mode {
                    debug!("Set time:long via til");
                }
                state.handle_set_time("long");
                self.pending_keys.clear();
                state.status_message = None;
            }
            ['t', 'e'] => {
                // Intermediate state - wait for third key
                state.status_message = Some("te → l: low | h: high | Esc: Cancel".to_string());
            }
            ['t', 'e', 'l'] => {
                if debug_mode {
                    debug!("Set energy:low via tel");
                }
                state.handle_set_energy("low");
                self.pending_keys.clear();
                state.status_message = None;
            }
            ['t', 'e', 'h'] => {
                if debug_mode {
                    debug!("Set energy:high via teh");
                }
                state.handle_set_energy("high");
                self.pending_keys.clear();
                state.status_message = None;
            }
            ['n', 'c'] => {
                if debug_mode {
                    debug!("Clear filter via nc");
                }
                state.handle_clear_filter();
                self.pending_keys.clear();
                state.status_message = None;
            }
            ['n', e @ ('l' | 'h' | 'a')] => {
                state.status_message = Some(build_nt_submenu(*e));
            }
            ['n', e @ ('l' | 'h' | 'a'), t @ ('s' | 'm' | 'l' | 'a')] => {
                let energy = match *e {
                    'l' => Some("low"),
                    'h' => Some("high"),
                    _ => None,
                };
                let time = match *t {
                    's' => Some("short"),
                    'm' => Some("medium"),
                    'l' => Some("long"),
                    _ => None,
                };
                if debug_mode {
                    debug!("Set filter energy={energy:?} time={time:?} via n{e}{t}");
                }
                state.handle_set_filter(energy, time);
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
            KeyCode::Char('t') => {
                self.pending_keys.push('t');
                state.status_message = Some(build_t_submenu());
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
            KeyCode::Char('o') => {
                if debug_mode {
                    debug!("Open URLs command received");
                }
                state.handle_open_urls();
            }
            KeyCode::Char('f') => {
                if debug_mode {
                    debug!("Hint mode requested");
                }
                state.pending_enter_hint = true;
            }
            KeyCode::Char('n') if state.view_mode == ViewMode::Todo => {
                self.pending_keys.push('n');
                state.status_message = Some(build_n_submenu(state.current_filter.is_some()));
            }
            KeyCode::Char('v') if state.view_mode == ViewMode::Todo => {
                if debug_mode {
                    debug!("Toggle todo layout");
                }
                state.handle_toggle_layout();
            }
            KeyCode::Char('z') => {
                if debug_mode {
                    debug!("Toggle deferred visibility");
                }
                state.handle_toggle_deferred();
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

fn build_t_submenu() -> String {
    "t → i: time | e: energy | Esc: Cancel".to_string()
}

fn build_n_submenu(has_filter: bool) -> String {
    let mut parts = String::from("n → l: low | h: high | a: any energy");
    if has_filter {
        parts.push_str(" | c: clear filter");
    }
    parts.push_str(" | Esc: Cancel");
    parts
}

fn build_nt_submenu(energy_key: char) -> String {
    let label = match energy_key {
        'l' => "low",
        'h' => "high",
        _ => "any",
    };
    format!("n{label} → s: short | m: medium | l: long | a: any time | Esc: Cancel")
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

    fn create_test_state() -> crate::app_state::AppState {
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
        crate::app_state::AppState::new(
            todos,
            "/tmp/nvim.sock".to_string(),
            "/tmp/todotxt".to_string(),
        )
    }

    #[test]
    fn test_d_key_shows_intermediate() {
        let mut handler = EventHandler::new();
        let mut state = create_test_state();

        handler.handle_keyboard_event(&make_key_event('d'), &mut state, false);
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
        handler.handle_keyboard_event(&make_key_event('d'), &mut state, false);
        handler.handle_keyboard_event(&make_key_event('d'), &mut state, false);

        assert!(handler.pending_keys.is_empty());
        assert!(state.status_message.is_none());
        let remaining = fs::read_to_string(&todo_file_path).unwrap();
        assert!(!remaining.contains("zap-1"));

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_d_then_other_cancels() {
        let mut handler = EventHandler::new();
        let mut state = create_test_state();

        handler.handle_keyboard_event(&make_key_event('d'), &mut state, false);
        handler.handle_keyboard_event(&make_key_event('x'), &mut state, false);

        assert!(handler.pending_keys.is_empty());
        assert!(state.status_message.is_none());
    }

    #[test]
    fn test_t_key_shows_intermediate() {
        let mut handler = EventHandler::new();
        let mut state = create_test_state();

        handler.handle_keyboard_event(&make_key_event('t'), &mut state, false);
        assert_eq!(handler.pending_keys.as_slice(), &['t']);
        let msg = state.status_message.as_deref().unwrap();
        assert!(msg.contains("time"));
        assert!(msg.contains("energy"));
    }

    #[test]
    fn test_ti_intermediate_state() {
        let mut handler = EventHandler::new();
        let mut state = create_test_state();

        handler.handle_keyboard_event(&make_key_event('t'), &mut state, false);
        handler.handle_keyboard_event(&make_key_event('i'), &mut state, false);
        assert_eq!(handler.pending_keys.as_slice(), &['t', 'i']);
        let msg = state.status_message.as_deref().unwrap();
        assert!(msg.contains("short"));
        assert!(msg.contains("medium"));
        assert!(msg.contains("long"));
    }

    #[test]
    fn test_three_stroke_tis_sets_time_short() {
        use std::fs;
        let temp_dir = std::env::temp_dir().join("torudo_event_tis");
        fs::remove_dir_all(&temp_dir).ok();
        fs::create_dir_all(&temp_dir).unwrap();
        let todo_file_path = temp_dir.join("todo.txt");
        fs::write(&todo_file_path, "Estimate me +proj id:est-1\n").unwrap();

        let todos = crate::todo::load_todos(todo_file_path.to_str().unwrap()).unwrap();
        let mut state = crate::app_state::AppState::new(
            todos,
            String::new(),
            temp_dir.to_str().unwrap().to_string(),
        );

        let mut handler = EventHandler::new();
        for c in ['t', 'i', 's'] {
            handler.handle_keyboard_event(&make_key_event(c), &mut state, false);
        }

        assert!(handler.pending_keys.is_empty());
        assert!(state.status_message.is_none());
        let content = fs::read_to_string(&todo_file_path).unwrap();
        assert!(content.contains("time:short"));

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_three_stroke_tim_sets_time_medium() {
        use std::fs;
        let temp_dir = std::env::temp_dir().join("torudo_event_tim");
        fs::remove_dir_all(&temp_dir).ok();
        fs::create_dir_all(&temp_dir).unwrap();
        let todo_file_path = temp_dir.join("todo.txt");
        fs::write(&todo_file_path, "Estimate me +proj id:est-1 time:short\n").unwrap();

        let todos = crate::todo::load_todos(todo_file_path.to_str().unwrap()).unwrap();
        let mut state = crate::app_state::AppState::new(
            todos,
            String::new(),
            temp_dir.to_str().unwrap().to_string(),
        );

        let mut handler = EventHandler::new();
        for c in ['t', 'i', 'm'] {
            handler.handle_keyboard_event(&make_key_event(c), &mut state, false);
        }

        let content = fs::read_to_string(&todo_file_path).unwrap();
        assert!(content.contains("time:medium"));
        assert!(!content.contains("time:short"));

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_three_stroke_til_sets_time_long() {
        use std::fs;
        let temp_dir = std::env::temp_dir().join("torudo_event_til");
        fs::remove_dir_all(&temp_dir).ok();
        fs::create_dir_all(&temp_dir).unwrap();
        let todo_file_path = temp_dir.join("todo.txt");
        fs::write(&todo_file_path, "Task +proj id:x-1\n").unwrap();

        let todos = crate::todo::load_todos(todo_file_path.to_str().unwrap()).unwrap();
        let mut state = crate::app_state::AppState::new(
            todos,
            String::new(),
            temp_dir.to_str().unwrap().to_string(),
        );

        let mut handler = EventHandler::new();
        for c in ['t', 'i', 'l'] {
            handler.handle_keyboard_event(&make_key_event(c), &mut state, false);
        }

        let content = fs::read_to_string(&todo_file_path).unwrap();
        assert!(content.contains("time:long"));

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_te_intermediate_state() {
        let mut handler = EventHandler::new();
        let mut state = create_test_state();

        handler.handle_keyboard_event(&make_key_event('t'), &mut state, false);
        handler.handle_keyboard_event(&make_key_event('e'), &mut state, false);
        assert_eq!(handler.pending_keys.as_slice(), &['t', 'e']);
        let msg = state.status_message.as_deref().unwrap();
        assert!(msg.contains("low"));
        assert!(msg.contains("high"));
    }

    #[test]
    fn test_three_stroke_tel_sets_energy_low() {
        use std::fs;
        let temp_dir = std::env::temp_dir().join("torudo_event_tel");
        fs::remove_dir_all(&temp_dir).ok();
        fs::create_dir_all(&temp_dir).unwrap();
        let todo_file_path = temp_dir.join("todo.txt");
        fs::write(&todo_file_path, "Estimate me +proj id:est-1\n").unwrap();

        let todos = crate::todo::load_todos(todo_file_path.to_str().unwrap()).unwrap();
        let mut state = crate::app_state::AppState::new(
            todos,
            String::new(),
            temp_dir.to_str().unwrap().to_string(),
        );

        let mut handler = EventHandler::new();
        for c in ['t', 'e', 'l'] {
            handler.handle_keyboard_event(&make_key_event(c), &mut state, false);
        }

        assert!(handler.pending_keys.is_empty());
        assert!(state.status_message.is_none());
        let content = fs::read_to_string(&todo_file_path).unwrap();
        assert!(content.contains("energy:low"));

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_three_stroke_teh_sets_energy_high() {
        use std::fs;
        let temp_dir = std::env::temp_dir().join("torudo_event_teh");
        fs::remove_dir_all(&temp_dir).ok();
        fs::create_dir_all(&temp_dir).unwrap();
        let todo_file_path = temp_dir.join("todo.txt");
        fs::write(&todo_file_path, "Estimate me +proj id:est-1 energy:low\n").unwrap();

        let todos = crate::todo::load_todos(todo_file_path.to_str().unwrap()).unwrap();
        let mut state = crate::app_state::AppState::new(
            todos,
            String::new(),
            temp_dir.to_str().unwrap().to_string(),
        );

        let mut handler = EventHandler::new();
        for c in ['t', 'e', 'h'] {
            handler.handle_keyboard_event(&make_key_event(c), &mut state, false);
        }

        let content = fs::read_to_string(&todo_file_path).unwrap();
        assert!(content.contains("energy:high"));
        assert!(!content.contains("energy:low"));

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_te_then_invalid_cancels() {
        let mut handler = EventHandler::new();
        let mut state = create_test_state();

        handler.handle_keyboard_event(&make_key_event('t'), &mut state, false);
        handler.handle_keyboard_event(&make_key_event('e'), &mut state, false);
        handler.handle_keyboard_event(&make_key_event('q'), &mut state, false);

        assert!(handler.pending_keys.is_empty());
        assert!(state.status_message.is_none());
    }

    #[test]
    fn test_ti_then_invalid_cancels() {
        let mut handler = EventHandler::new();
        let mut state = create_test_state();

        handler.handle_keyboard_event(&make_key_event('t'), &mut state, false);
        handler.handle_keyboard_event(&make_key_event('i'), &mut state, false);
        handler.handle_keyboard_event(&make_key_event('q'), &mut state, false);

        assert!(handler.pending_keys.is_empty());
        assert!(state.status_message.is_none());
    }

    #[test]
    fn test_question_mark_toggles_help() {
        let mut handler = EventHandler::new();
        let mut state = create_test_state();

        assert!(!state.show_help);
        handler.handle_keyboard_event(&make_key_event('?'), &mut state, false);
        assert!(state.show_help);
        handler.handle_keyboard_event(&make_key_event('?'), &mut state, false);
        assert!(!state.show_help);
    }

    #[test]
    fn test_help_overlay_blocks_other_keys() {
        let mut handler = EventHandler::new();
        let mut state = create_test_state();

        state.show_help = true;
        let initial_column = state.selected_in_column;
        handler.handle_keyboard_event(&make_key_event('j'), &mut state, false);
        assert_eq!(state.selected_in_column, initial_column);
        assert!(state.show_help);
    }

    #[test]
    fn test_q_closes_help() {
        let mut handler = EventHandler::new();
        let mut state = create_test_state();

        state.show_help = true;
        let quit = handler.handle_keyboard_event(&make_key_event('q'), &mut state, false);
        assert!(!quit);
        assert!(!state.show_help);
    }

    #[test]
    fn test_esc_closes_help() {
        let mut handler = EventHandler::new();
        let mut state = create_test_state();

        state.show_help = true;
        let esc_event = Event::Key(KeyEvent {
            code: KeyCode::Esc,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        });
        let quit = handler.handle_keyboard_event(&esc_event, &mut state, false);
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
        let mut state = create_test_state();

        // ALL order: Inbox, Todo, Waiting, Ref, Someday. Initial mode is Todo
        assert_eq!(state.view_mode, ViewMode::Todo);
        handler.handle_keyboard_event(&make_tab_event(), &mut state, false);
        assert_eq!(state.view_mode, ViewMode::Waiting);
    }

    #[test]
    fn test_backtab_switches_to_prev_mode() {
        let mut handler = EventHandler::new();
        let mut state = create_test_state();

        assert_eq!(state.view_mode, ViewMode::Todo);
        handler.handle_keyboard_event(&make_backtab_event(), &mut state, false);
        assert_eq!(state.view_mode, ViewMode::Inbox);
    }

    #[test]
    fn test_s_submenu_shows_targets() {
        let mut handler = EventHandler::new();
        let mut state = create_test_state();

        // Pressing s in Todo mode shows all modes except Todo
        handler.handle_keyboard_event(&make_key_event('s'), &mut state, false);
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
        let mut state = create_test_state();
        state.view_mode = ViewMode::Inbox;

        handler.handle_keyboard_event(&make_key_event('s'), &mut state, false);
        let msg = state.status_message.as_deref().unwrap();
        assert!(msg.contains("t: Todo")); // From Inbox mode, Todo is shown
        assert!(!msg.contains("i: Inbox")); // Current mode is hidden
    }

    #[test]
    fn test_s_submenu_order_matches_tab_order_from_todo() {
        // Tab order (ViewMode::ALL) is Inbox, Todo, Waiting, Ref, Someday
        // Pressing s in Todo mode yields the order without Todo = Inbox, Waiting, Ref, Someday
        let mut handler = EventHandler::new();
        let mut state = create_test_state();
        assert_eq!(state.view_mode, ViewMode::Todo);

        handler.handle_keyboard_event(&make_key_event('s'), &mut state, false);
        assert_eq!(
            state.status_message.as_deref().unwrap(),
            "s → | i: Inbox | w: Waiting | r: Ref | s: Someday | Esc: Cancel"
        );
    }

    #[test]
    fn test_s_submenu_order_matches_tab_order_from_inbox() {
        // Pressing s in Inbox mode yields the order without Inbox = Todo, Waiting, Ref, Someday
        let mut handler = EventHandler::new();
        let mut state = create_test_state();
        state.view_mode = ViewMode::Inbox;

        handler.handle_keyboard_event(&make_key_event('s'), &mut state, false);
        assert_eq!(
            state.status_message.as_deref().unwrap(),
            "s → | t: Todo | w: Waiting | r: Ref | s: Someday | Esc: Cancel"
        );
    }

    #[test]
    fn test_p_prefix_shows_submenu() {
        let mut handler = EventHandler::new();
        let mut state = create_test_state();

        handler.handle_keyboard_event(&make_key_event('p'), &mut state, false);

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

        handler.handle_keyboard_event(&make_key_event('p'), &mut state, false);
        handler.handle_keyboard_event(&make_key_event('a'), &mut state, false);

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

        handler.handle_keyboard_event(&make_key_event('p'), &mut state, false);
        handler.handle_keyboard_event(&make_key_event('x'), &mut state, false);

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
        let mut state = create_test_state();

        handler.handle_keyboard_event(&make_key_event('p'), &mut state, false);
        handler.handle_keyboard_event(&make_key_event('z'), &mut state, false);

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
        handler.handle_keyboard_event(&make_key_event('x'), &mut state, false);

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
    fn test_x_in_waiting_mode_leaves_todo_file_untouched() {
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
        handler.handle_keyboard_event(&make_key_event('x'), &mut state, false);

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
        handler.handle_keyboard_event(&make_key_event('x'), &mut state, false);

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

    #[test]
    fn test_f_key_requests_hint_enter() {
        let mut handler = EventHandler::new();
        let mut state = create_test_state();

        handler.handle_keyboard_event(&make_key_event('f'), &mut state, false);
        assert!(
            state.pending_enter_hint,
            "f should flag pending_enter_hint for next frame"
        );
        assert!(state.hint.is_none(), "hint is actually entered by ui.rs");
    }

    #[test]
    fn test_char_during_hint_mode_types_into_hint() {
        let mut handler = EventHandler::new();
        let mut state = create_test_state();

        let cells: Vec<(usize, usize)> = (0..30).map(|i| (0, i)).collect();
        state.enter_hint_mode(&cells);

        handler.handle_keyboard_event(&make_key_event('a'), &mut state, false);
        let hint = state
            .hint
            .as_ref()
            .expect("hint should remain after prefix input");
        assert_eq!(hint.buffer, "a");
    }

    #[test]
    fn test_hint_mode_exact_match_moves_selection() {
        let mut handler = EventHandler::new();
        let mut state = create_test_state();

        let cells = vec![(0, 0), (0, 1), (0, 2)];
        state.enter_hint_mode(&cells);
        handler.handle_keyboard_event(&make_key_event('c'), &mut state, false);

        assert!(state.hint.is_none(), "exact match exits hint mode");
        assert_eq!(state.selected_in_column, 2);
    }

    #[test]
    fn test_esc_during_hint_mode_cancels() {
        use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
        let mut handler = EventHandler::new();
        let mut state = create_test_state();

        state.enter_hint_mode(&[(0, 0), (0, 1)]);
        let esc = Event::Key(KeyEvent {
            code: KeyCode::Esc,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        });
        handler.handle_keyboard_event(&esc, &mut state, false);

        assert!(state.hint.is_none(), "Esc exits hint mode");
        assert!(state.status_message.is_none());
    }

    #[test]
    fn test_non_prefix_key_during_hint_mode_cancels() {
        let mut handler = EventHandler::new();
        let mut state = create_test_state();

        // only 1 label 'a' — pressing 'z' is not a prefix of anything, cancels
        state.enter_hint_mode(&[(0, 0)]);
        handler.handle_keyboard_event(&make_key_event('z'), &mut state, false);

        assert!(state.hint.is_none(), "non-prefix char exits hint mode");
    }

    fn make_filter_test_state() -> (crate::app_state::AppState, tempfile::TempDir) {
        use std::fs;
        let temp_dir = tempfile::tempdir().expect("tempdir");
        fs::write(
            temp_dir.path().join("todo.txt"),
            "A +p1 id:a energy:low time:short\n\
             B +p1 id:b energy:high time:long\n\
             C +p2 id:c\n",
        )
        .unwrap();
        let todos =
            crate::todo::load_todos(temp_dir.path().join("todo.txt").to_str().unwrap()).unwrap();
        let state = crate::app_state::AppState::new(
            todos,
            String::new(),
            temp_dir.path().to_str().unwrap().to_string(),
        );
        (state, temp_dir)
    }

    #[test]
    fn test_n_key_shows_energy_submenu() {
        let (mut state, _dir) = make_filter_test_state();
        let mut handler = EventHandler::new();
        handler.handle_keyboard_event(&make_key_event('n'), &mut state, false);
        assert_eq!(handler.pending_keys.as_slice(), &['n']);
        let msg = state.status_message.as_deref().unwrap();
        assert!(msg.contains("low"));
        assert!(msg.contains("high"));
        assert!(msg.contains("any"));
    }

    #[test]
    fn test_nl_shows_time_submenu() {
        let (mut state, _dir) = make_filter_test_state();
        let mut handler = EventHandler::new();
        for c in ['n', 'l'] {
            handler.handle_keyboard_event(&make_key_event(c), &mut state, false);
        }
        assert_eq!(handler.pending_keys.as_slice(), &['n', 'l']);
        let msg = state.status_message.as_deref().unwrap();
        assert!(msg.contains("short"));
        assert!(msg.contains("medium"));
        assert!(msg.contains("long"));
        assert!(msg.contains("any"));
    }

    #[test]
    fn test_three_stroke_nls_filters_low_short() {
        let (mut state, _dir) = make_filter_test_state();
        let mut handler = EventHandler::new();
        for c in ['n', 'l', 's'] {
            handler.handle_keyboard_event(&make_key_event(c), &mut state, false);
        }
        assert!(handler.pending_keys.is_empty());
        assert!(state.status_message.is_none());
        assert!(state.current_filter.is_some());
        let total: usize = state.grouped_todos.values().map(Vec::len).sum();
        assert_eq!(total, 1, "only the low+short todo should remain");
    }

    #[test]
    fn test_three_stroke_nas_filters_time_only() {
        let (mut state, _dir) = make_filter_test_state();
        let mut handler = EventHandler::new();
        for c in ['n', 'a', 's'] {
            handler.handle_keyboard_event(&make_key_event(c), &mut state, false);
        }
        let total: usize = state.grouped_todos.values().map(Vec::len).sum();
        assert_eq!(total, 1, "only time:short remains regardless of energy");
    }

    #[test]
    fn test_three_stroke_naa_clears_filter() {
        let (mut state, _dir) = make_filter_test_state();
        let mut handler = EventHandler::new();
        // First apply a filter
        for c in ['n', 'l', 's'] {
            handler.handle_keyboard_event(&make_key_event(c), &mut state, false);
        }
        assert!(state.current_filter.is_some());
        // Then n→a→a should clear
        for c in ['n', 'a', 'a'] {
            handler.handle_keyboard_event(&make_key_event(c), &mut state, false);
        }
        assert!(state.current_filter.is_none());
        let total: usize = state.grouped_todos.values().map(Vec::len).sum();
        assert_eq!(total, 3);
    }

    #[test]
    fn test_nc_clears_filter() {
        let (mut state, _dir) = make_filter_test_state();
        let mut handler = EventHandler::new();
        for c in ['n', 'l', 's'] {
            handler.handle_keyboard_event(&make_key_event(c), &mut state, false);
        }
        assert!(state.current_filter.is_some());

        for c in ['n', 'c'] {
            handler.handle_keyboard_event(&make_key_event(c), &mut state, false);
        }
        assert!(state.current_filter.is_none());
        assert!(handler.pending_keys.is_empty());
        assert!(state.status_message.is_none());
    }

    #[test]
    fn test_n_submenu_includes_clear_when_filter_active() {
        let (mut state, _dir) = make_filter_test_state();
        let mut handler = EventHandler::new();
        for c in ['n', 'l', 's'] {
            handler.handle_keyboard_event(&make_key_event(c), &mut state, false);
        }
        // filter is now active — press n again; submenu should include "clear"
        handler.handle_keyboard_event(&make_key_event('n'), &mut state, false);
        let msg = state.status_message.as_deref().unwrap();
        assert!(msg.contains("clear"));
    }

    #[test]
    fn test_n_submenu_no_clear_when_no_filter() {
        let (mut state, _dir) = make_filter_test_state();
        let mut handler = EventHandler::new();
        handler.handle_keyboard_event(&make_key_event('n'), &mut state, false);
        let msg = state.status_message.as_deref().unwrap();
        assert!(!msg.contains("clear"));
    }

    #[test]
    fn test_n_then_invalid_cancels() {
        let (mut state, _dir) = make_filter_test_state();
        let mut handler = EventHandler::new();
        handler.handle_keyboard_event(&make_key_event('n'), &mut state, false);
        handler.handle_keyboard_event(&make_key_event('q'), &mut state, false);
        assert!(handler.pending_keys.is_empty());
        assert!(state.status_message.is_none());
        assert!(state.current_filter.is_none());
    }

    #[test]
    fn v_key_toggles_layout_in_todo_mode() {
        let (mut state, _dir) = make_filter_test_state();
        let mut handler = EventHandler::new();
        assert_eq!(state.todo_layout, crate::app_state::TodoLayout::Project);
        handler.handle_keyboard_event(&make_key_event('v'), &mut state, false);
        assert_eq!(state.todo_layout, crate::app_state::TodoLayout::Pick);
        handler.handle_keyboard_event(&make_key_event('v'), &mut state, false);
        assert_eq!(state.todo_layout, crate::app_state::TodoLayout::Project);
    }

    #[test]
    fn z_key_toggles_deferred_visibility() {
        let mut state = create_test_state();
        let mut handler = EventHandler::new();
        assert!(state.show_deferred);
        handler.handle_keyboard_event(&make_key_event('z'), &mut state, false);
        assert!(!state.show_deferred);
        handler.handle_keyboard_event(&make_key_event('z'), &mut state, false);
        assert!(state.show_deferred);
        assert!(handler.pending_keys.is_empty());
    }

    #[test]
    fn v_key_ignored_in_ref_mode() {
        let (mut state, dir) = make_filter_test_state();
        std::fs::write(dir.path().join("ref.txt"), "R +p id:r\n").unwrap();
        state.set_view_mode(ViewMode::Ref);
        let mut handler = EventHandler::new();
        handler.handle_keyboard_event(&make_key_event('v'), &mut state, false);
        // Layout stays on default; no pending key buffered
        assert_eq!(state.todo_layout, crate::app_state::TodoLayout::Project);
        assert!(handler.pending_keys.is_empty());
    }

    #[test]
    fn test_n_noop_outside_todo_mode() {
        let (mut state, dir) = make_filter_test_state();
        // Create waiting.txt so set_view_mode has a file to load
        std::fs::write(dir.path().join("waiting.txt"), "W +p id:w\n").unwrap();
        state.set_view_mode(ViewMode::Waiting);
        let mut handler = EventHandler::new();
        handler.handle_keyboard_event(&make_key_event('n'), &mut state, false);
        assert!(handler.pending_keys.is_empty());
        assert!(state.status_message.is_none());
    }
}
