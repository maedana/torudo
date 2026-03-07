use crate::app_state::AppState;
use crossterm::event::{Event, KeyCode};
use log::debug;
use notify::{Event as NotifyEvent, EventKind};
use std::sync::mpsc;
use std::time::{Duration, Instant};

pub struct EventHandler {
    last_reload_time: Option<Instant>,
    debounce_duration: Duration,
    pending_key: Option<char>,
}

impl EventHandler {
    pub const fn new() -> Self {
        Self {
            last_reload_time: None,
            debounce_duration: Duration::from_millis(200),
            pending_key: None,
        }
    }

    pub fn handle_file_watcher_events(
        &mut self,
        file_watcher_rx: &mpsc::Receiver<NotifyEvent>,
        todo_file: &str,
        state: &mut AppState,
        debug_mode: bool,
    ) {
        let mut should_reload = false;
        while let Ok(event) = file_watcher_rx.try_recv() {
            // Check if event is related to todo.txt
            let todo_file_path = std::path::Path::new(todo_file);
            let is_todo_file_event = event
                .paths
                .iter()
                .any(|path| path.file_name() == todo_file_path.file_name());

            if is_todo_file_event {
                if debug_mode {
                    debug!("todo.txt related event detected: {:?}", event.kind);
                }
                match event.kind {
                    EventKind::Modify(_) => {
                        should_reload = true;
                        if debug_mode {
                            debug!("todo.txt change event queued for reload");
                        }
                    }
                    _ => {
                        if debug_mode {
                            debug!("Ignoring todo.txt event: {:?}", event.kind);
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
                state.handle_reload(todo_file);
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
            // Handle second key of 2-stroke sequence
            if let Some(first) = self.pending_key.take() {
                if first == 's' {
                    let todotxt_dir = std::path::Path::new(todo_file)
                        .parent()
                        .and_then(|p| p.to_str())
                        .unwrap_or(".");
                    match key.code {
                        KeyCode::Char('p') => {
                            if debug_mode {
                                debug!("Send plan prompt requested (sp)");
                            }
                            state.handle_send_plan(todotxt_dir);
                        }
                        KeyCode::Char('i') => {
                            if debug_mode {
                                debug!("Send implement prompt requested (si)");
                            }
                            state.handle_send_implement(todotxt_dir);
                        }
                        _ => {
                            if debug_mode {
                                debug!("Unknown send command: s + {:?}", key.code);
                            }
                        }
                    }
                }
                return false;
            }

            match key.code {
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
                KeyCode::Char('x') => {
                    if debug_mode {
                        debug!("Complete todo command received");
                    }
                    state.handle_complete_todo(todo_file);
                }
                KeyCode::Char('r') => {
                    if debug_mode {
                        debug!("Reload command received");
                    }
                    state.handle_reload(todo_file);
                }
                KeyCode::Char('s') if state.crmux_available => {
                    self.pending_key = Some('s');
                    state.status_message = Some("s-".to_string());
                }
                _ => {}
            }
        }
        false // Continue running
    }
}
