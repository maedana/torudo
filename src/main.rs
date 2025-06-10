use clap::Parser;
use log::{debug, info, error};
use std::io::Write;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::time::{Duration, Instant};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Terminal,
};
use std::{error::Error, io, env, sync::mpsc};
use notify::{Watcher, RecursiveMode, RecommendedWatcher, Event as NotifyEvent, EventKind};

mod todo;
mod app_state;
use todo::{Item, load_todos, add_missing_ids};
use app_state::AppState;

#[derive(Parser)]
#[command(name = "torudo")]
#[command(about = "A terminal-based todo.txt viewer and manager")]
struct Args {
    /// Enable debug mode
    #[arg(short, long)]
    debug: bool,
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
        debug!("TODOTXT_DIR: {}", todotxt_dir);
        debug!("Todo file: {}", todo_file);
    }

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
    let (tx, rx) = mpsc::channel();
    let mut watcher = RecommendedWatcher::new(
        move |res: Result<NotifyEvent, notify::Error>| {
            if let Ok(event) = res {
                let _ = tx.send(event);
            }
        },
        notify::Config::default(),
    )?;
    watcher.watch(std::path::Path::new(&todotxt_dir), RecursiveMode::NonRecursive)?;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let todos = load_todos(&todo_file)?;
    if args.debug {
        debug!("Loaded {} todos from file", todos.len());
    }
    let result = run_app(&mut terminal, todos, &rx, &todo_file, args.debug, watcher);

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

fn setup_debug_logging(todotxt_dir: &str) -> Result<(), Box<dyn Error>> {
    let debug_log_path = format!("{todotxt_dir}/debug.log");
    
    env_logger::Builder::from_default_env()
        .target(env_logger::Target::Pipe(Box::new(
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(debug_log_path)?
        )))
        .filter_level(log::LevelFilter::Debug)
        .format(|buf, record| {
            writeln!(buf, "[{}] {} - {}: {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                record.level(),
                record.target(),
                record.args()
            )
        })
        .init();
    
    Ok(())
}


fn create_todo_spans(todo: &Item) -> Vec<Span> {
    let mut spans = Vec::new();
    if todo.completed {
        spans.push(Span::styled("✓ ", Style::default().fg(Color::Green)));
    } else {
        spans.push(Span::raw("  "));
    }
    if let Some(priority) = todo.priority {
        let color = match priority {
            'A' => Color::Red,
            'B' => Color::Yellow,
            'C' => Color::Blue,
            _ => Color::White,
        };
        spans.push(Span::styled(
            format!("({priority}) "),
            Style::default().fg(color).add_modifier(Modifier::BOLD)
        ));
    }
    spans.push(Span::raw(&todo.description));
    for context in &todo.contexts {
        spans.push(Span::styled(
            format!(" @{context}"),
            Style::default().fg(Color::Cyan)
        ));
    }
    spans
}

fn get_todo_styles(is_selected: bool, is_completed: bool) -> (Style, Style) {
    let todo_style = if is_selected {
        Style::default().fg(Color::Yellow)
    } else if is_completed {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::White)
    };

    let background_style = if is_selected {
        Style::default().bg(Color::DarkGray)
    } else {
        Style::default()
    };

    (todo_style, background_style)
}

fn draw_project_column_owned(
    f: &mut ratatui::Frame,
    project_todos: &[Item],
    project_name: &str,
    column_area: ratatui::layout::Rect,
    is_active_column: bool,
    selected_in_column: usize,
) {
    let border_style = if is_active_column {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::White)
    };

    let project_block = Block::default()
        .title(format!("{project_name} ({}))", project_todos.len()))
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner_area = project_block.inner(column_area);
    f.render_widget(project_block, column_area);

    // Calculate dynamic height for each todo based on text length
    let available_width = inner_area.width.saturating_sub(4); // Account for borders
    let todo_constraints: Vec<Constraint> = project_todos.iter()
        .map(|todo| {
            // Create spans to get accurate text length including priority and context
            let spans = create_todo_spans(todo);
            let total_text_len: usize = spans.iter().map(|span| span.content.chars().count()).sum();
            
            let lines_needed = if available_width > 0 && available_width > 10 {
                // More conservative calculation for better text wrapping
                let effective_width = available_width.saturating_sub(2); // Account for padding
                let lines = ((total_text_len as u16 + effective_width - 1) / effective_width).max(1);
                lines + 2 // +2 for borders
            } else {
                4 // Fallback minimum height
            };
            Constraint::Length(lines_needed.min(8)) // Cap at 8 lines to prevent excessive height
        })
        .collect();

    if !todo_constraints.is_empty() {
        let todo_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(todo_constraints)
            .split(inner_area);

        for (todo_idx, todo) in project_todos.iter().enumerate() {
            if todo_idx < todo_layout.len() {
                let spans = create_todo_spans(todo);
                let is_selected = is_active_column && todo_idx == selected_in_column;
                let (todo_style, background_style) = get_todo_styles(is_selected, todo.completed);

                let todo_paragraph = Paragraph::new(Line::from(spans))
                    .block(Block::default()
                        .borders(Borders::ALL)
                        .border_style(todo_style))
                    .style(background_style)
                    .wrap(Wrap { trim: true });

                f.render_widget(todo_paragraph, todo_layout[todo_idx]);
            }
        }
    }
}

fn draw_ui(
    f: &mut ratatui::Frame,
    state: &AppState,
) {
    let size = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ].as_ref())
        .split(size);

    let title = Paragraph::new("Todo.txt Viewer")
        .block(Block::default().title("Torudo").borders(Borders::ALL))
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::Cyan));

    let num_columns = state.project_names.len();
    if num_columns > 0 {
        let column_constraints: Vec<Constraint> = (0..num_columns)
            .map(|_| Constraint::Percentage(100 / u16::try_from(num_columns).unwrap_or(1)))
            .collect();
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(column_constraints)
            .split(chunks[1]);

        for (col_idx, project_name) in state.project_names.iter().enumerate() {
            if let Some(project_todos) = state.grouped_todos.get(project_name) {
                let is_active_column = col_idx == state.current_column;
                let selected_for_this_column = if is_active_column { state.selected_in_column } else { usize::MAX };

                draw_project_column_owned(
                    f,
                    project_todos,
                    project_name,
                    columns[col_idx],
                    is_active_column,
                    selected_for_this_column,
                );
            }
        }
    }

    let instructions = Paragraph::new("jk: Navigate | hl: Change Column | x: Complete | r: Reload | q: Quit")
        .block(Block::default().title("Instructions").borders(Borders::ALL))
        .alignment(Alignment::Center);

    f.render_widget(title, chunks[0]);
    f.render_widget(instructions, chunks[2]);
}


fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>, 
    todos: Vec<Item>,
    file_watcher_rx: &mpsc::Receiver<NotifyEvent>,
    todo_file: &str,
    debug_mode: bool,
    _watcher: RecommendedWatcher
) -> io::Result<()> {
    let mut state = AppState::new(todos);
    let mut last_reload_time: Option<Instant> = None;
    let debounce_duration = Duration::from_millis(200);

    // Send initial vim command on startup
    state.send_initial_vim_command();

    loop {
        terminal.draw(|f| {
            draw_ui(f, &state);
        })?;

        // Check file watcher events
        let mut should_reload = false;
        while let Ok(event) = file_watcher_rx.try_recv() {
            // Check if event is related to todo.txt
            let todo_file_path = std::path::Path::new(todo_file);
            let is_todo_file_event = event.paths.iter().any(|path| {
                path.file_name() == todo_file_path.file_name()
            });
            
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
            let should_perform_reload = match last_reload_time {
                None => true,
                Some(last_time) => now.duration_since(last_time) >= debounce_duration,
            };
            
            if should_perform_reload {
                if debug_mode {
                    debug!("Executing debounced reload of todos");
                }
                state.handle_reload(todo_file);
                last_reload_time = Some(now);
            } else {
                if debug_mode {
                    debug!("Skipping reload due to debounce (too recent)");
                }
            }
        }

        // Check keyboard events non-blocking
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => {
                        if debug_mode {
                            debug!("Quit command received");
                        }
                        return Ok(());
                    },
                    KeyCode::Char(c @ ('k' | 'j' | 'h' | 'l')) => {
                        if debug_mode {
                            debug!("Navigation key pressed: {}", c);
                        }
                        state.handle_navigation_key(c);
                    },
                    KeyCode::Char('x') => {
                        if debug_mode {
                            debug!("Complete todo command received");
                        }
                        state.handle_complete_todo(todo_file);
                    },
                    KeyCode::Char('r') => {
                        if debug_mode {
                            debug!("Reload command received");
                        }
                        state.handle_reload(todo_file);
                    },
                    _ => {}
                }
            }
        }
    }
}
