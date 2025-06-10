use clap::Parser;
use log::{debug, info, error};
use std::io::Write;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
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
    
    // デバッグモードの設定
    if args.debug {
        setup_debug_logging(&todotxt_dir)?;
        info!("Debug mode enabled");
        debug!("TODOTXT_DIR: {}", todotxt_dir);
        debug!("Todo file: {}", todo_file);
    }

    // 初回起動時にIDがない行にUUIDを付与
    if add_missing_ids(&todo_file).is_err() {
        // エラーが発生しても継続
        if args.debug {
            error!("Failed to add missing IDs to todo file");
        }
    } else if args.debug {
        debug!("Added missing IDs to todo file if needed");
    }

    // ファイル監視の設定
    let (tx, rx) = mpsc::channel();
    let mut watcher = RecommendedWatcher::new(
        move |res: Result<NotifyEvent, notify::Error>| {
            if let Ok(event) = res {
                let _ = tx.send(event);
            }
        },
        notify::Config::default(),
    )?;
    watcher.watch(std::path::Path::new(&todo_file), RecursiveMode::NonRecursive)?;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let todos = load_todos(&todo_file)?;
    if args.debug {
        debug!("Loaded {} todos from file", todos.len());
    }
    let result = run_app(&mut terminal, todos, &rx, &todo_file, args.debug);

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

    let todo_height = 3;
    let todo_constraints: Vec<Constraint> = project_todos.iter()
        .map(|_| Constraint::Length(todo_height))
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
    debug_mode: bool
) -> io::Result<()> {
    let mut state = AppState::new(todos);

    // 初期選択時にvimコマンドを送信
    state.send_initial_vim_command();

    loop {
        terminal.draw(|f| {
            draw_ui(f, &state);
        })?;

        // ファイル監視イベントをチェック
        if let Ok(event) = file_watcher_rx.try_recv() {
            match event.kind {
                EventKind::Modify(_) | EventKind::Create(_) => {
                    if debug_mode {
                        debug!("File change detected, reloading todos");
                    }
                    state.handle_reload(todo_file);
                }
                _ => {}
            }
        }

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
