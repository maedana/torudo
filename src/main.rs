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
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Terminal,
};
use std::{collections::HashMap, error::Error, fs, io};
use chrono::NaiveDate;

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
        let mut parts = line.trim().split_whitespace().peekable();
        let mut item = TodoItem {
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
        if let Some(&"x") = parts.peek() {
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
            if part.starts_with('+') {
                item.projects.push(part[1..].to_string());
            } else if part.starts_with('@') {
                item.contexts.push(part[1..].to_string());
            } else if part.starts_with("id:") {
                item.id = Some(part[3..].to_string());
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

fn main() -> Result<(), Box<dyn Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    
    let todos = load_todos("/home/maedana/todotxt/todo.txt")?;
    let result = run_app(&mut terminal, todos);
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

fn create_todo_list_items<'a>(todos: &[&'a TodoItem], selected_in_column: usize) -> Vec<ListItem<'a>> {
    todos
        .iter()
        .enumerate()
        .map(|(i, todo)| {
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
                    format!("({}) ", priority),
                    Style::default().fg(color).add_modifier(Modifier::BOLD)
                ));
            }
            spans.push(Span::raw(&todo.description));
            for context in &todo.contexts {
                spans.push(Span::styled(
                    format!(" @{}", context),
                    Style::default().fg(Color::Cyan)
                ));
            }
            let style = if i == selected_in_column {
                Style::default().bg(Color::DarkGray)
            } else if todo.completed {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(spans)).style(style)
        })
        .collect()
}

fn run_app<B: ratatui::backend::Backend>(terminal: &mut Terminal<B>, todos: Vec<TodoItem>) -> io::Result<()> {
    let grouped_todos = group_todos_by_project(&todos);
    let mut project_names: Vec<String> = grouped_todos.keys().cloned().collect();
    project_names.sort();
    let mut current_column = 0;
    let mut selected_in_column = 0;
    loop {
        terminal.draw(|f| {
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
            let num_columns = project_names.len().min(3);
            if num_columns > 0 {
                let column_constraints: Vec<Constraint> = (0..num_columns)
                    .map(|_| Constraint::Percentage(100 / num_columns as u16))
                    .collect();
                let columns = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints(column_constraints)
                    .split(chunks[1]);
                for (col_idx, project_name) in project_names.iter().take(num_columns).enumerate() {
                    if let Some(project_todos) = grouped_todos.get(project_name) {
                        let is_active_column = col_idx == current_column;
                        let selected_for_this_column = if is_active_column { selected_in_column } else { usize::MAX };
                        let items = create_todo_list_items(project_todos, selected_for_this_column);
                        let border_style = if is_active_column {
                            Style::default().fg(Color::Yellow)
                        } else {
                            Style::default().fg(Color::White)
                        };
                        let list = List::new(items)
                            .block(Block::default()
                                .title(format!("{} ({})", project_name, project_todos.len()))
                                .borders(Borders::ALL)
                                .border_style(border_style))
                            .highlight_style(Style::default().bg(Color::DarkGray));
                        f.render_widget(list, columns[col_idx]);
                    }
                }
            }
            let instructions = Paragraph::new("↑↓: Navigate | ←→: Change Column | q: Quit")
                .block(Block::default().title("Instructions").borders(Borders::ALL))
                .alignment(Alignment::Center);
            
            f.render_widget(title, chunks[0]);
            f.render_widget(instructions, chunks[2]);
        })?;
        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char('q') => return Ok(()),
                KeyCode::Up => {
                    if selected_in_column > 0 {
                        selected_in_column -= 1;
                    }
                },
                KeyCode::Down => {
                    if let Some(current_project_name) = project_names.get(current_column) {
                        if let Some(current_todos) = grouped_todos.get(current_project_name) {
                            if selected_in_column < current_todos.len().saturating_sub(1) {
                                selected_in_column += 1;
                            }
                        }
                    }
                },
                KeyCode::Left => {
                    if current_column > 0 {
                        current_column -= 1;
                        selected_in_column = 0;
                    }
                },
                KeyCode::Right => {
                    if current_column < project_names.len().min(3).saturating_sub(1) {
                        current_column += 1;
                        selected_in_column = 0;
                    }
                },
                _ => {}
            }
        }
    }
}
