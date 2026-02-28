use clap::Parser;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use log::{debug, error, info};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::{env, error::Error, io};
use tmux_claude_state::monitor::MonitorState;

mod app_state;
mod event_handler;
mod file_watcher;
mod setup;
mod todo;
mod ui;

use app_state::AppState;
use event_handler::EventHandler;
use file_watcher::FileWatcher;
use setup::{ensure_setup_exists, setup_debug_logging};
use todo::{add_missing_ids, load_todos};
use ui::draw_ui;

#[derive(Parser)]
#[command(name = "torudo")]
#[command(about = "A terminal-based todo.txt viewer and manager")]
#[command(version)]
struct Args {
    /// Enable debug mode
    #[arg(short, long)]
    debug: bool,

    /// Neovim socket path (set by nvim --listen)
    #[arg(long, env = "NVIM_LISTEN_ADDRESS", default_value = "/tmp/nvim.sock")]
    nvim_listen: String,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    let home_dir = env::var("HOME").unwrap();
    let todotxt_dir = env::var("TODOTXT_DIR").unwrap_or_else(|_| format!("{home_dir}/todotxt"));
    let todo_file = format!("{todotxt_dir}/todo.txt");

    // Setup debug mode
    if args.debug {
        setup_debug_logging(&todotxt_dir)?;
        info!("Debug mode enabled");
        debug!("TODOTXT_DIR: {todotxt_dir}");
        debug!("Todo file: {todo_file}");
    }

    // Ensure required directories and files exist
    ensure_setup_exists(&todotxt_dir, &todo_file)?;

    // Add UUIDs to lines without IDs on first startup
    if add_missing_ids(&todo_file).is_err() {
        // Continue even if error occurs
        if args.debug {
            error!("Failed to add missing IDs to todo file");
        }
    } else if args.debug {
        debug!("Added missing IDs to todo file if needed");
    }

    // Setup file watcher
    let mut file_watcher = FileWatcher::new(&todotxt_dir)?;
    file_watcher.start_watching(&todotxt_dir)?;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let todos = load_todos(&todo_file)?;
    if args.debug {
        debug!("Loaded {} todos from file", todos.len());
    }

    let monitor_state = if env::var("TMUX").is_ok() {
        let state = Arc::new(Mutex::new(MonitorState::default()));
        tmux_claude_state::monitor::start_polling(Arc::clone(&state));
        Some(state)
    } else {
        None
    };

    let result = run_app(
        &mut terminal,
        todos,
        file_watcher.receiver(),
        &todo_file,
        args.debug,
        args.nvim_listen,
        monitor_state,
    );

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    if let Err(err) = result {
        println!("{err:?}");
    }
    Ok(())
}

fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    todos: Vec<todo::Item>,
    file_watcher_rx: &std::sync::mpsc::Receiver<notify::Event>,
    todo_file: &str,
    debug_mode: bool,
    nvim_socket: String,
    monitor_state: Option<Arc<Mutex<MonitorState>>>,
) -> io::Result<()> {
    let mut state = AppState::new(todos, nvim_socket, monitor_state);
    let mut event_handler = EventHandler::new();

    // Send initial vim command on startup
    state.send_initial_vim_command();

    loop {
        terminal.draw(|f| {
            draw_ui(f, &state);
        })?;

        // Handle file watcher events
        event_handler.handle_file_watcher_events(
            file_watcher_rx,
            todo_file,
            &mut state,
            debug_mode,
        );

        // Check keyboard events non-blocking
        if event::poll(Duration::from_millis(100))? {
            let event = event::read()?;
            if EventHandler::handle_keyboard_event(&event, &mut state, todo_file, debug_mode) {
                return Ok(()); // Quit was requested
            }
        }

        state.maybe_update_preview();
    }
}
