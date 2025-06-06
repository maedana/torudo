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

fn create_todo_spans(todo: &TodoItem) -> Vec<Span> {
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
    project_todos: &[TodoItem],
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

fn reload_todos(todo_file: &str) -> Option<Vec<TodoItem>> {
    load_todos(todo_file).ok()
}

struct AppState {
    todos: Vec<TodoItem>,
    grouped_todos: HashMap<String, Vec<TodoItem>>,
    project_names: Vec<String>,
    current_column: usize,
    selected_in_column: usize,
}

impl AppState {
    fn new(todos: Vec<TodoItem>) -> Self {
        let grouped_todos = group_todos_by_project_owned(&todos);
        let mut project_names: Vec<String> = grouped_todos.keys().cloned().collect();
        project_names.sort();

        Self {
            todos,
            grouped_todos,
            project_names,
            current_column: 0,
            selected_in_column: 0,
        }
    }

    fn reload_todos(&mut self, todo_file: &str) {
        if let Some(new_todos) = reload_todos(todo_file) {
            self.todos = new_todos;
            self.update_derived_state();
        }
    }

    fn update_derived_state(&mut self) {
        self.grouped_todos = group_todos_by_project_owned(&self.todos);
        self.project_names = self.grouped_todos.keys().cloned().collect();
        self.project_names.sort();

        if self.current_column >= self.project_names.len() {
            self.current_column = self.project_names.len().saturating_sub(1);
        }
        if let Some(current_project_name) = self.project_names.get(self.current_column) {
            if let Some(current_todos) = self.grouped_todos.get(current_project_name) {
                if self.selected_in_column >= current_todos.len() {
                    self.selected_in_column = current_todos.len().saturating_sub(1);
                }
                if let Some(selected_todo) = current_todos.get(self.selected_in_column) {
                    if let Some(todo_id) = &selected_todo.id {
                        send_vim_command(todo_id);
                    }
                }
            }
        }
    }

    fn get_current_todo_id(&self) -> Option<&str> {
        let current_project_name = self.project_names.get(self.current_column)?;
        let current_todos = self.grouped_todos.get(current_project_name)?;
        let selected_todo = current_todos.get(self.selected_in_column)?;
        selected_todo.id.as_deref()
    }
}

fn group_todos_by_project_owned(todos: &[TodoItem]) -> HashMap<String, Vec<TodoItem>> {
    let mut grouped = HashMap::new();
    for todo in todos {
        if todo.projects.is_empty() {
            grouped.entry("No Project".to_string()).or_insert_with(Vec::new).push(todo.clone());
        } else {
            for project in &todo.projects {
                grouped.entry(project.clone()).or_insert_with(Vec::new).push(todo.clone());
            }
        }
    }
    grouped
}

impl AppState {
    fn handle_navigation_key(&mut self, key_char: char) {
        match key_char {
            'k' => {
                if self.selected_in_column > 0 {
                    self.selected_in_column -= 1;
                    if let Some(todo_id) = self.get_current_todo_id() {
                        send_vim_command(todo_id);
                    }
                }
            },
            'j' => {
                if let Some(current_project_name) = self.project_names.get(self.current_column) {
                    if let Some(current_todos) = self.grouped_todos.get(current_project_name) {
                        if self.selected_in_column < current_todos.len().saturating_sub(1) {
                            self.selected_in_column += 1;
                            if let Some(todo_id) = self.get_current_todo_id() {
                                send_vim_command(todo_id);
                            }
                        }
                    }
                }
            },
            'h' => {
                if self.current_column > 0 {
                    self.current_column -= 1;
                    self.selected_in_column = 0;
                    if let Some(todo_id) = self.get_current_todo_id() {
                        send_vim_command(todo_id);
                    }
                }
            },
            'l' => {
                if self.current_column < self.project_names.len().saturating_sub(1) {
                    self.current_column += 1;
                    self.selected_in_column = 0;
                    if let Some(todo_id) = self.get_current_todo_id() {
                        send_vim_command(todo_id);
                    }
                }
            },
            _ => {}
        }
    }

    fn handle_complete_todo(&mut self, todo_file: &str) {
        if let Some(todo_id) = self.get_current_todo_id() {
            if matches!(complete_todo(todo_file, todo_id), Ok(())) {
                self.reload_todos(todo_file);
            }
        }
    }

    fn handle_reload(&mut self, todo_file: &str) {
        if add_missing_ids(todo_file).is_err() {
            // エラーが発生しても継続
        }
        self.reload_todos(todo_file);
    }
}

fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>, 
    todos: Vec<TodoItem>,
    file_watcher_rx: &mpsc::Receiver<NotifyEvent>,
    todo_file: &str
) -> io::Result<()> {
    let mut state = AppState::new(todos);

    // 初期選択時にvimコマンドを送信
    if let Some(todo_id) = state.get_current_todo_id() {
        send_vim_command(todo_id);
    }

    loop {
        terminal.draw(|f| {
            draw_ui(f, &state);
        })?;

        // ファイル監視イベントをチェック
        if let Ok(event) = file_watcher_rx.try_recv() {
            match event.kind {
                EventKind::Modify(_) | EventKind::Create(_) => {
                    state.handle_reload(todo_file);
                }
                _ => {}
            }
        }

        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char('q') => return Ok(()),
                KeyCode::Char(c @ ('k' | 'j' | 'h' | 'l')) => {
                    state.handle_navigation_key(c);
                },
                KeyCode::Char('x') => {
                    state.handle_complete_todo(todo_file);
                },
                KeyCode::Char('r') => {
                    state.handle_reload(todo_file);
                },
                _ => {}
            }
        }
    }
}
