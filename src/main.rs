use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use log::{debug, error, info};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::time::Duration;
use std::{env, error::Error, io};

mod app_state;
mod claude;
mod cli;
mod crmux;
mod event_handler;
mod file_watcher;
mod help;
mod md_preview;
mod recurrence;
mod rpc_client;
mod rpc_server;
mod setup;
mod todo;
mod ui;
mod update;
mod url;

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

    /// Path to the todotxt directory
    #[arg(long, env = "TODOTXT_DIR")]
    todotxt_dir: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Print the currently selected todo as JSON to stdout
    Current,
    /// Move the running TUI's cursor onto the item with this id
    Focus {
        /// Id of the item to select (as printed by `torudo search`)
        id: String,
    },
    /// Print one item by id as JSON, detail md included, whatever mode it is in
    Show {
        /// Id of the item to print (as printed by `torudo search`)
        id: String,
    },
    /// Update torudo to the latest version
    Update {
        /// Skip version check and force re-download
        #[arg(long)]
        force: bool,
        /// Check for updates without installing
        #[arg(long)]
        check: bool,
    },
    /// Search every mode for items whose title or detail file contains the query
    Search {
        /// Text to look for (case-insensitive substring, not a regex)
        query: String,
        /// Print the hits as a JSON array instead of one line each
        #[arg(long)]
        json: bool,
        /// Add each hit's detail md (`todos/{id}.md`) as an `md` field. JSON only
        #[arg(long, requires = "json")]
        with_md: bool,
        /// Search only this mode instead of all of them
        #[arg(long, value_enum)]
        mode: Option<cli::ModeName>,
    },
    /// Inbox operations
    Inbox {
        #[command(subcommand)]
        action: cli::ModeAction,
    },
    /// Todo operations
    Todo {
        #[command(subcommand)]
        action: cli::ModeAction,
    },
    /// Waiting operations
    Waiting {
        #[command(subcommand)]
        action: cli::ModeAction,
    },
    /// Ref operations
    Ref {
        #[command(subcommand)]
        action: cli::ModeAction,
    },
    /// Someday operations
    Someday {
        #[command(subcommand)]
        action: cli::ModeAction,
    },
    /// Completed items archive
    Done {
        #[command(subcommand)]
        action: cli::DoneAction,
    },
}

/// Map a mode subcommand to the GTD mode it operates on.
const fn mode_command(command: &Commands) -> Option<(app_state::ViewMode, &cli::ModeAction)> {
    match command {
        Commands::Inbox { action } => Some((app_state::ViewMode::Inbox, action)),
        Commands::Todo { action } => Some((app_state::ViewMode::Todo, action)),
        Commands::Waiting { action } => Some((app_state::ViewMode::Waiting, action)),
        Commands::Ref { action } => Some((app_state::ViewMode::Ref, action)),
        Commands::Someday { action } => Some((app_state::ViewMode::Someday, action)),
        _ => None,
    }
}

fn resolve_todotxt_dir(cli: Option<String>) -> String {
    cli.unwrap_or_else(|| {
        let home_dir = env::var("HOME").unwrap();
        format!("{home_dir}/todotxt")
    })
}

fn main() -> Result<(), Box<dyn Error>> {
    let matches = Args::command()
        .after_help(help::cli_help_text())
        .get_matches();
    let args = Args::from_arg_matches(&matches).expect("arg parsing should not fail");

    // Handle subcommands before TUI setup
    if matches!(args.command, Some(Commands::Current)) {
        return rpc_client::run_current();
    }
    if let Some(Commands::Focus { id }) = &args.command {
        return rpc_client::run_focus(id);
    }
    if let Some(Commands::Show { id }) = &args.command {
        // Unlike `focus`, `show` reads the files itself instead of asking the running
        // TUI, so `--todotxt-dir` applies.
        let todotxt_dir = resolve_todotxt_dir(args.todotxt_dir.clone());
        return cli::run_show(id, &todotxt_dir);
    }
    if let Some(Commands::Update { force, check }) = args.command {
        handle_update(force, check);
        return Ok(());
    }
    if let Some(Commands::Search {
        query,
        json,
        with_md,
        mode,
    }) = &args.command
    {
        let todotxt_dir = resolve_todotxt_dir(args.todotxt_dir.clone());
        return cli::run_search(query, *json, *with_md, *mode, &todotxt_dir);
    }
    if let Some(command) = &args.command
        && let Some((mode, action)) = mode_command(command)
    {
        let todotxt_dir = resolve_todotxt_dir(args.todotxt_dir.clone());
        return cli::run(mode, action, &todotxt_dir);
    }
    if let Some(Commands::Done { action }) = &args.command {
        let todotxt_dir = resolve_todotxt_dir(args.todotxt_dir.clone());
        return cli::run_done(action, &todotxt_dir);
    }

    let todotxt_dir = resolve_todotxt_dir(args.todotxt_dir);
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

    let result = run_app(
        &mut terminal,
        todos,
        file_watcher.receiver(),
        &todo_file,
        &todotxt_dir,
        args.debug,
        args.nvim_listen,
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

#[allow(clippy::too_many_arguments)]
fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    todos: Vec<todo::Item>,
    file_watcher_rx: &std::sync::mpsc::Receiver<notify::Event>,
    todo_file: &str,
    todotxt_dir: &str,
    debug_mode: bool,
    nvim_socket: String,
) -> io::Result<()> {
    let mut state = AppState::new(todos, nvim_socket, todotxt_dir.to_string());
    let mut event_handler = EventHandler::new();

    let rpc_server = match rpc_server::RpcServer::new() {
        Ok(server) => Some(server),
        Err(e) => {
            debug!("Failed to start RPC server: {e}");
            None
        }
    };

    // Background version check
    let update_result: std::sync::Arc<std::sync::Mutex<Option<String>>> =
        std::sync::Arc::new(std::sync::Mutex::new(None));
    {
        let update_result = std::sync::Arc::clone(&update_result);
        std::thread::spawn(move || {
            if let Ok(version) = update::fetch_latest_version()
                && let Ok(mut result) = update_result.lock()
            {
                *result = Some(version);
            }
        });
    }

    // Send initial vim command on startup
    state.send_initial_vim_command();

    loop {
        terminal.draw(|f| {
            draw_ui(f, &mut state);
        })?;

        // Handle file watcher events
        event_handler.handle_file_watcher_events(file_watcher_rx, &mut state, debug_mode);

        if let Some(ref server) = rpc_server {
            server.poll(&mut state);
        }

        // Check for background update result
        if state.update_available.is_none()
            && let Ok(mut result) = update_result.try_lock()
            && let Some(version) = result.take()
        {
            let current = env!("CARGO_PKG_VERSION");
            if let update::UpdateStatus::UpdateAvailable(v) =
                update::check_update_needed(current, &version)
            {
                state.update_available = Some(v);
            }
        }

        // Check keyboard events non-blocking
        if event::poll(Duration::from_millis(100))? {
            let event = event::read()?;
            if event_handler.handle_keyboard_event(&event, &mut state, todo_file, debug_mode) {
                return Ok(()); // Quit was requested
            }
        }
    }
}

fn handle_update(force: bool, check: bool) {
    let current = env!("CARGO_PKG_VERSION");
    println!("torudo v{current} - checking for updates...");

    if check {
        match update::fetch_latest_version() {
            Ok(latest) => match update::check_update_needed(current, &latest) {
                update::UpdateStatus::AlreadyLatest(v) => {
                    println!("Already up to date (latest: {v})");
                }
                update::UpdateStatus::UpdateAvailable(v) => {
                    println!("Update available: {v}");
                    println!("Run `torudo update` to install");
                }
            },
            Err(e) => eprintln!("Failed to check for updates: {e}"),
        }
        return;
    }

    let result = if force {
        update::perform_update_force()
    } else {
        update::perform_update()
    };

    match result {
        Ok(status) => println!("{status}"),
        Err(e) => eprintln!("Update failed: {e}"),
    }
}
