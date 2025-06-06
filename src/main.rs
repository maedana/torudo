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
    widgets::{Block, Borders, List, ListItem, Paragraph, Row, Table},
    Terminal,
};
use std::{error::Error, fs, io};
use chrono::{NaiveDate, Local};
use uuid::Uuid;

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

fn run_app<B: ratatui::backend::Backend>(terminal: &mut Terminal<B>, todos: Vec<TodoItem>) -> io::Result<()> {
    let mut selected = 0;
    
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
            
            let items: Vec<ListItem> = todos
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
                    
                    for project in &todo.projects {
                        spans.push(Span::styled(
                            format!(" +{}", project),
                            Style::default().fg(Color::Magenta)
                        ));
                    }
                    
                    for context in &todo.contexts {
                        spans.push(Span::styled(
                            format!(" @{}", context),
                            Style::default().fg(Color::Cyan)
                        ));
                    }
                    
                    let style = if i == selected {
                        Style::default().bg(Color::DarkGray)
                    } else if todo.completed {
                        Style::default().fg(Color::DarkGray)
                    } else {
                        Style::default()
                    };
                    
                    ListItem::new(Line::from(spans)).style(style)
                })
                .collect();
            
            let list = List::new(items)
                .block(Block::default().title("Tasks").borders(Borders::ALL))
                .highlight_style(Style::default().bg(Color::DarkGray));
            
            let instructions = Paragraph::new("↑↓: Navigate | q: Quit")
                .block(Block::default().title("Instructions").borders(Borders::ALL))
                .alignment(Alignment::Center);
            
            f.render_widget(title, chunks[0]);
            f.render_widget(list, chunks[1]);
            f.render_widget(instructions, chunks[2]);
        })?;
        
        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char('q') => return Ok(()),
                KeyCode::Up => {
                    if selected > 0 {
                        selected -= 1;
                    }
                },
                KeyCode::Down => {
                    if selected < todos.len().saturating_sub(1) {
                        selected += 1;
                    }
                },
                _ => {}
            }
        }
    }
}
