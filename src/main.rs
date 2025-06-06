use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Terminal,
};
use std::{error::Error, io};

fn main() -> Result<(), Box<dyn Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let result = run_app(&mut terminal);
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

fn run_app<B: ratatui::backend::Backend>(terminal: &mut Terminal<B>) -> io::Result<()> {
    loop {
        terminal.draw(|f| {
            let size = f.area();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Min(0),
                ].as_ref())
                .split(size);
            let title = Paragraph::new(vec![
                Line::from(vec![
                    Span::styled("Hello", Style::default().fg(Color::Yellow)),
                    Span::raw(", "),
                    Span::styled("World", Style::default().fg(Color::Green)),
                    Span::raw("!"),
                ]),
            ])
            .block(Block::default().title("Ratatui Hello World").borders(Borders::ALL))
            .alignment(Alignment::Center);
            let instructions = Paragraph::new("Press 'q' to quit")
                .block(Block::default().title("Instructions").borders(Borders::ALL))
                .alignment(Alignment::Center);
            f.render_widget(title, chunks[0]);
            f.render_widget(instructions, chunks[1]);
        })?;
        if let Event::Key(key) = event::read()? {
            if let KeyCode::Char('q') = key.code {
                return Ok(());
            }
        }
    }
}
