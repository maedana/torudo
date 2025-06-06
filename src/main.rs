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
use std::{collections::HashMap, error::Error, fs, io, process::Command, env, sync::mpsc};
use chrono::NaiveDate;
use uuid::Uuid;
use notify::{Watcher, RecursiveMode, RecommendedWatcher, Event as NotifyEvent, EventKind};

#[derive(Debug, Clone)]
struct TodoItem {
    completed: bool,
    priority: Option<char>,
    creation_date: Option<NaiveDate>,
    completion_date: Option<NaiveDate>,
    description: String,
    projects: Vec<String>,
    contexts: Vec<String>,
    id: Option<String>,
}

impl TodoItem {
    fn parse(line: &str) -> Self {
        let mut parts = line.split_whitespace().peekable();
        let mut item = Self {
            completed: false,
            priority: None,
            creation_date: None,
            completion_date: None,
            description: String::new(),
            projects: Vec::new(),
            contexts: Vec::new(),
            id: None,
        };
        let mut desc_parts = Vec::new();
        if parts.peek() == Some(&"x") {
            item.completed = true;
            parts.next();
            if let Some(date_str) = parts.peek() {
                if let Ok(date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                    item.completion_date = Some(date);
                    parts.next();
                }
            }
        }
        if let Some(part) = parts.peek() {
            if part.len() == 3 && part.starts_with('(') && part.ends_with(')') {
                if let Some(c) = part.chars().nth(1) {
                    if c.is_ascii_uppercase() {
                        item.priority = Some(c);
                        parts.next();
                    }
                }
            }
        }
        if let Some(date_str) = parts.peek() {
            if let Ok(date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                item.creation_date = Some(date);
                parts.next();
            }
        }
        for part in parts {
            if let Some(stripped) = part.strip_prefix('+') {
                item.projects.push(stripped.to_string());
            } else if let Some(stripped) = part.strip_prefix('@') {
                item.contexts.push(stripped.to_string());
            } else if let Some(stripped) = part.strip_prefix("id:") {
                item.id = Some(stripped.to_string());
            } else {
                desc_parts.push(part);
            }
        }
        item.description = desc_parts.join(" ");
        item
    }
}

fn load_todos(file_path: &str) -> Result<Vec<TodoItem>, Box<dyn Error>> {
    let content = fs::read_to_string(file_path)?;
    let todos = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(TodoItem::parse)
        .collect();
    Ok(todos)
}

fn add_missing_ids(file_path: &str) -> Result<(), Box<dyn Error>> {
    let content = fs::read_to_string(file_path)?;
    let lines: Vec<&str> = content.lines().collect();
    let mut modified = false;
    let mut new_lines = Vec::new();
    
    for line in lines {
        if line.trim().is_empty() {
            new_lines.push(line.to_string());
            continue;
        }
        
        let todo = TodoItem::parse(line);
        if todo.id.is_none() {
            let new_id = Uuid::new_v4().to_string();
            let new_line = format!("{line} id:{new_id}");
            new_lines.push(new_line);
            modified = true;
        } else {
            new_lines.push(line.to_string());
        }
    }
    
    if modified {
        let new_content = new_lines.join("\n");
        fs::write(file_path, new_content)?;
    }
    
    Ok(())
}

fn complete_todo(todo_file: &str, todo_id: &str) -> Result<(), Box<dyn Error>> {
    let content = fs::read_to_string(todo_file)?;
    let lines: Vec<&str> = content.lines().collect();
    let mut new_lines = Vec::new();
    let mut completed_line = None;
    
    for line in lines {
        if line.trim().is_empty() {
            new_lines.push(line.to_string());
            continue;
        }
        
        let todo = TodoItem::parse(line);
        if let Some(id) = &todo.id {
            if id == todo_id {
                let today = chrono::Local::now().format("%Y-%m-%d").to_string();
                let completed_todo_line = if todo.completed {
                    line.to_string()
                } else {
                    format!("x {today} {line}")
                };
                completed_line = Some(completed_todo_line);
                continue;
            }
        }
        new_lines.push(line.to_string());
    }
    
    if let Some(completed_todo) = completed_line {
        let todo_dir = std::path::Path::new(todo_file).parent().unwrap();
        let done_file = todo_dir.join("done.txt");
        
        let mut done_content = if done_file.exists() {
            fs::read_to_string(&done_file)?
        } else {
            String::new()
        };
        
        if !done_content.is_empty() && !done_content.ends_with('\n') {
            done_content.push('\n');
        }
        done_content.push_str(&completed_todo);
        done_content.push('\n');
        
        fs::write(&done_file, done_content)?;
        
        let new_todo_content = new_lines.join("\n");
        fs::write(todo_file, new_todo_content)?;
    }
    
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let home_dir = env::var("HOME").unwrap();
    let todotxt_dir = env::var("TODOTXT_DIR").unwrap_or_else(|_| format!("{home_dir}/todotxt"));
    let todo_file = format!("{todotxt_dir}/todo.txt");
    
    // 初回起動時にIDがない行にUUIDを付与
    if add_missing_ids(&todo_file).is_err() {
        // エラーが発生しても継続
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
    let result = run_app(&mut terminal, todos, &rx, &todo_file);
    
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

fn group_todos_by_project(todos: &[TodoItem]) -> HashMap<String, Vec<&TodoItem>> {
    let mut grouped = HashMap::new();
    for todo in todos {
        if todo.projects.is_empty() {
            grouped.entry("No Project".to_string()).or_insert_with(Vec::new).push(todo);
        } else {
            for project in &todo.projects {
                grouped.entry(project.clone()).or_insert_with(Vec::new).push(todo);
            }
        }
    }
    grouped
}


fn send_vim_command(todo_id: &str) {
    let home_dir = env::var("HOME").unwrap();
    let todotxt_dir = env::var("TODOTXT_DIR").unwrap_or_else(|_| format!("{home_dir}/todotxt"));
    let file_path = format!("{todotxt_dir}/todos/{todo_id}.md");
    let command = format!(":e {file_path}<CR>");
    let socket_path = env::var("NVIM_LISTEN_ADDRESS")
        .unwrap_or_else(|_| "/tmp/nvim.sock".to_string());

    let _ = Command::new("nvim")
        .arg("--server")
        .arg(&socket_path)
        .arg("--remote-send")
        .arg(&command)
        .output();
}

#[allow(clippy::too_many_lines)]
fn draw_ui(
    f: &mut ratatui::Frame,
    project_names: &[String],
    grouped_todos: &HashMap<String, Vec<&TodoItem>>,
    current_column: usize,
    selected_in_column: usize,
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
    let num_columns = project_names.len();
    if num_columns > 0 {
        let column_constraints: Vec<Constraint> = (0..num_columns)
            .map(|_| Constraint::Percentage(100 / u16::try_from(num_columns).unwrap_or(1)))
            .collect();
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(column_constraints)
            .split(chunks[1]);
        for (col_idx, project_name) in project_names.iter().enumerate() {
            if let Some(project_todos) = grouped_todos.get(project_name) {
                let is_active_column = col_idx == current_column;
                let selected_for_this_column = if is_active_column { selected_in_column } else { usize::MAX };
                
                let border_style = if is_active_column {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default().fg(Color::White)
                };
                
                let project_block = Block::default()
                    .title(format!("{} ({})", project_name, project_todos.len()))
                    .borders(Borders::ALL)
                    .border_style(border_style);
                
                let inner_area = project_block.inner(columns[col_idx]);
                f.render_widget(project_block, columns[col_idx]);
                
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
                            
                            let is_selected = is_active_column && todo_idx == selected_for_this_column;
                            let todo_style = if is_selected {
                                Style::default().fg(Color::Yellow)
                            } else if todo.completed {
                                Style::default().fg(Color::DarkGray)
                            } else {
                                Style::default().fg(Color::White)
                            };
                            
                            let background_style = if is_selected {
                                Style::default().bg(Color::DarkGray)
                            } else {
                                Style::default()
                            };
                            
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
        }
    }
    let instructions = Paragraph::new("jk: Navigate | hl: Change Column | x: Complete | r: Reload | q: Quit")
        .block(Block::default().title("Instructions").borders(Borders::ALL))
        .alignment(Alignment::Center);
    
    f.render_widget(title, chunks[0]);
    f.render_widget(instructions, chunks[2]);
}

fn reload_todos(todo_file: &str) -> Option<Vec<TodoItem>> {
    load_todos(todo_file).ok()
}

fn send_vim_command_for_current_selection(
    project_names: &[String],
    grouped_todos: &HashMap<String, Vec<&TodoItem>>,
    current_column: usize,
    selected_in_column: usize,
) {
    if let Some(current_project_name) = project_names.get(current_column) {
        if let Some(current_todos) = grouped_todos.get(current_project_name) {
            if let Some(selected_todo) = current_todos.get(selected_in_column) {
                if let Some(todo_id) = &selected_todo.id {
                    send_vim_command(todo_id);
                }
            }
        }
    }
}

#[allow(clippy::cognitive_complexity, clippy::too_many_lines)]
fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>, 
    mut todos: Vec<TodoItem>,
    file_watcher_rx: &mpsc::Receiver<NotifyEvent>,
    todo_file: &str
) -> io::Result<()> {
    
    let mut grouped_todos = group_todos_by_project(&todos);
    let mut project_names: Vec<String> = grouped_todos.keys().cloned().collect();
    project_names.sort();
    let mut current_column = 0;
    let mut selected_in_column = 0;
    
    // 初期選択時にvimコマンドを送信
    if let Some(current_project_name) = project_names.get(current_column) {
        if let Some(current_todos) = grouped_todos.get(current_project_name) {
            if let Some(selected_todo) = current_todos.get(selected_in_column) {
                if let Some(todo_id) = &selected_todo.id {
                    send_vim_command(todo_id);
                }
            }
        }
    }
    
    loop {
        terminal.draw(|f| {
            draw_ui(f, &project_names, &grouped_todos, current_column, selected_in_column);
        })?;
        
        // ファイル監視イベントをチェック
        if let Ok(event) = file_watcher_rx.try_recv() {
            match event.kind {
                EventKind::Modify(_) | EventKind::Create(_) => {
                    // ファイルが変更されたらIDがない行にUUIDを付与
                    if add_missing_ids(todo_file).is_err() {
                        // エラーが発生しても継続
                    }
                    // ファイルを再読み込み
                    if let Some(new_todos) = reload_todos(todo_file) {
                        todos = new_todos;
                        grouped_todos = group_todos_by_project(&todos);
                        project_names = grouped_todos.keys().cloned().collect();
                        project_names.sort();
                        
                        if current_column >= project_names.len() {
                            current_column = project_names.len().saturating_sub(1);
                        }
                        if let Some(current_project_name) = project_names.get(current_column) {
                            if let Some(current_todos) = grouped_todos.get(current_project_name) {
                                if selected_in_column >= current_todos.len() {
                                    selected_in_column = current_todos.len().saturating_sub(1);
                                }
                                if let Some(selected_todo) = current_todos.get(selected_in_column) {
                                    if let Some(todo_id) = &selected_todo.id {
                                        send_vim_command(todo_id);
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        
        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char('q') => return Ok(()),
                KeyCode::Char('k') => {
                    if selected_in_column > 0 {
                        selected_in_column -= 1;
                        send_vim_command_for_current_selection(&project_names, &grouped_todos, current_column, selected_in_column);
                    }
                },
                KeyCode::Char('j') => {
                    if let Some(current_project_name) = project_names.get(current_column) {
                        if let Some(current_todos) = grouped_todos.get(current_project_name) {
                            if selected_in_column < current_todos.len().saturating_sub(1) {
                                selected_in_column += 1;
                                send_vim_command_for_current_selection(&project_names, &grouped_todos, current_column, selected_in_column);
                            }
                        }
                    }
                },
                KeyCode::Char('h') => {
                    if current_column > 0 {
                        current_column -= 1;
                        selected_in_column = 0;
                        send_vim_command_for_current_selection(&project_names, &grouped_todos, current_column, selected_in_column);
                    }
                },
                KeyCode::Char('l') => {
                    if current_column < project_names.len().saturating_sub(1) {
                        current_column += 1;
                        selected_in_column = 0;
                        send_vim_command_for_current_selection(&project_names, &grouped_todos, current_column, selected_in_column);
                    }
                },
                KeyCode::Char('x') => {
                    if let Some(current_project_name) = project_names.get(current_column) {
                        if let Some(current_todos) = grouped_todos.get(current_project_name) {
                            if let Some(selected_todo) = current_todos.get(selected_in_column) {
                                if let Some(todo_id) = &selected_todo.id {
                                    if matches!(complete_todo(todo_file, todo_id), Ok(())) {
                                        if let Some(new_todos) = reload_todos(todo_file) {
                                            todos = new_todos;
                                            grouped_todos = group_todos_by_project(&todos);
                                            project_names = grouped_todos.keys().cloned().collect();
                                            project_names.sort();
                                            
                                            if current_column >= project_names.len() {
                                                current_column = project_names.len().saturating_sub(1);
                                            }
                                            if let Some(current_project_name) = project_names.get(current_column) {
                                                if let Some(current_todos) = grouped_todos.get(current_project_name) {
                                                    if selected_in_column >= current_todos.len() {
                                                        selected_in_column = current_todos.len().saturating_sub(1);
                                                    }
                                                    if let Some(selected_todo) = current_todos.get(selected_in_column) {
                                                        if let Some(todo_id) = &selected_todo.id {
                                                            send_vim_command(todo_id);
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
                KeyCode::Char('r') => {
                    if add_missing_ids(todo_file).is_err() {
                        // エラーが発生しても継続
                    }
                    if let Some(new_todos) = reload_todos(todo_file) {
                        todos = new_todos;
                        grouped_todos = group_todos_by_project(&todos);
                        project_names = grouped_todos.keys().cloned().collect();
                        project_names.sort();
                        
                        if current_column >= project_names.len() {
                            current_column = project_names.len().saturating_sub(1);
                        }
                        if let Some(current_project_name) = project_names.get(current_column) {
                            if let Some(current_todos) = grouped_todos.get(current_project_name) {
                                if selected_in_column >= current_todos.len() {
                                    selected_in_column = current_todos.len().saturating_sub(1);
                                }
                                if let Some(selected_todo) = current_todos.get(selected_in_column) {
                                    if let Some(todo_id) = &selected_todo.id {
                                        send_vim_command(todo_id);
                                    }
                                }
                            }
                        }
                    }
                },
                _ => {}
            }
        }
    }
}
