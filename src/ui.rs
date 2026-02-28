use crate::app_state::AppState;
use crate::todo::Item;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use std::time::Instant;
use tmux_claude_state::claude_state::ClaudeState;
use tmux_claude_state::monitor::ClaudeSession;

pub fn format_elapsed(since: Instant) -> String {
    let secs = since.elapsed().as_secs();
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else {
        format!("{}h", secs / 3600)
    }
}

pub fn create_todo_spans(todo: &Item) -> Vec<Span<'_>> {
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
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ));
    }
    spans.push(Span::raw(&todo.description));
    for context in &todo.contexts {
        spans.push(Span::styled(
            format!(" @{context}"),
            Style::default().fg(Color::Cyan),
        ));
    }
    spans
}

pub fn get_todo_styles(is_selected: bool, is_completed: bool) -> (Style, Style) {
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

pub fn draw_project_column(
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
    let todo_constraints: Vec<Constraint> = project_todos
        .iter()
        .map(|todo| {
            // Create spans to get accurate text length including priority and context
            let spans = create_todo_spans(todo);
            let total_text_len: usize = spans.iter().map(|span| span.content.chars().count()).sum();

            let lines_needed = if available_width > 10 {
                // More conservative calculation for better text wrapping
                let effective_width = available_width.saturating_sub(2); // Account for padding
                let lines =
                    u16::try_from(total_text_len).unwrap_or(u16::MAX).div_ceil(effective_width).max(1);
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
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_style(todo_style),
                    )
                    .style(background_style)
                    .wrap(Wrap { trim: true });

                f.render_widget(todo_paragraph, todo_layout[todo_idx]);
            }
        }
    }
}

const fn state_color(state: &ClaudeState) -> Color {
    match state {
        ClaudeState::Working => Color::Blue,
        ClaudeState::WaitingForApproval => Color::LightRed,
        ClaudeState::Idle => Color::White,
    }
}

const fn state_label(state: &ClaudeState) -> &'static str {
    match state {
        ClaudeState::Working => "Running",
        ClaudeState::WaitingForApproval => "Approval",
        ClaudeState::Idle => "Idle",
    }
}

pub fn draw_claude_sessions_column(
    f: &mut ratatui::Frame,
    sessions: &[ClaudeSession],
    column_area: ratatui::layout::Rect,
    is_active_column: bool,
    selected_index: usize,
) {
    let border_style = if is_active_column {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::White)
    };

    let block = Block::default()
        .title(format!("Claude Sessions ({})", sessions.len()))
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner_area = block.inner(column_area);
    f.render_widget(block, column_area);

    if sessions.is_empty() {
        return;
    }

    let constraints: Vec<Constraint> = sessions
        .iter()
        .map(|_| Constraint::Length(3))
        .collect();

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner_area);

    for (idx, session) in sessions.iter().enumerate() {
        if idx >= layout.len() {
            break;
        }
        let is_selected = is_active_column && idx == selected_index;
        let color = state_color(&session.state);
        let elapsed = format_elapsed(session.state_changed_at);
        let label = state_label(&session.state);

        let text_color = if is_selected { Color::Yellow } else { color };
        let spans = vec![
            Span::styled(
                &session.pane.project_name,
                Style::default().fg(text_color).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(label, Style::default().fg(text_color)),
            Span::raw(" "),
            Span::styled(elapsed, Style::default().fg(text_color)),
        ];

        let border_style = if is_selected {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(color)
        };

        let bg_style = if is_selected {
            Style::default().bg(Color::DarkGray)
        } else {
            Style::default()
        };

        let paragraph = Paragraph::new(Line::from(spans))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(border_style),
            )
            .style(bg_style);

        f.render_widget(paragraph, layout[idx]);
    }
}

pub fn draw_ui(f: &mut ratatui::Frame, state: &AppState) {
    let size = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(
            [
                Constraint::Length(3),
                Constraint::Min(0),
                Constraint::Length(3),
            ]
            .as_ref(),
        )
        .split(size);

    let title = Paragraph::new("Todo.txt Viewer")
        .block(Block::default().title("Torudo").borders(Borders::ALL))
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::Cyan));

    let num_columns = state.total_columns();
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
                let selected_for_this_column = if is_active_column {
                    state.selected_in_column
                } else {
                    usize::MAX
                };

                draw_project_column(
                    f,
                    project_todos,
                    project_name,
                    columns[col_idx],
                    is_active_column,
                    selected_for_this_column,
                );
            }
        }

        // Draw Claude Sessions column if enabled
        if state.claude_sessions_enabled {
            let claude_col_idx = state.project_names.len();
            let is_active = state.is_on_claude_column();
            let sessions = state.monitor_state
                .as_ref()
                .and_then(|ms| ms.lock().ok())
                .map_or_else(Vec::new, |lock| lock.sessions.clone());

            draw_claude_sessions_column(
                f,
                &sessions,
                columns[claude_col_idx],
                is_active,
                state.claude_selected_index,
            );
        }
    }

    let instructions =
        Paragraph::new("jk: Navigate | hl: Change Column | x: Complete | s: Switch Pane | r: Reload | q: Quit")
            .block(Block::default().title("Instructions").borders(Borders::ALL))
            .alignment(Alignment::Center);

    f.render_widget(title, chunks[0]);
    f.render_widget(instructions, chunks[2]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_elapsed_seconds() {
        let now = Instant::now();
        // Instant::now() just happened, so elapsed is ~0s
        let result = format_elapsed(now);
        assert_eq!(result, "0s");
    }

    #[test]
    fn test_format_elapsed_minutes() {
        let since = Instant::now() - std::time::Duration::from_secs(120);
        assert_eq!(format_elapsed(since), "2m");
    }

    #[test]
    fn test_format_elapsed_hours() {
        let since = Instant::now() - std::time::Duration::from_secs(7200);
        assert_eq!(format_elapsed(since), "2h");
    }

    #[test]
    fn test_format_elapsed_boundary_59s() {
        let since = Instant::now() - std::time::Duration::from_secs(59);
        assert_eq!(format_elapsed(since), "59s");
    }

    #[test]
    fn test_format_elapsed_boundary_60s() {
        let since = Instant::now() - std::time::Duration::from_secs(60);
        assert_eq!(format_elapsed(since), "1m");
    }

    #[test]
    fn test_format_elapsed_boundary_3599s() {
        let since = Instant::now() - std::time::Duration::from_secs(3599);
        assert_eq!(format_elapsed(since), "59m");
    }

    #[test]
    fn test_format_elapsed_boundary_3600s() {
        let since = Instant::now() - std::time::Duration::from_secs(3600);
        assert_eq!(format_elapsed(since), "1h");
    }

    #[test]
    fn test_state_color() {
        assert_eq!(state_color(&ClaudeState::Working), Color::Blue);
        assert_eq!(state_color(&ClaudeState::WaitingForApproval), Color::LightRed);
        assert_eq!(state_color(&ClaudeState::Idle), Color::White);
    }

    #[test]
    fn test_state_label() {
        assert_eq!(state_label(&ClaudeState::Working), "Running");
        assert_eq!(state_label(&ClaudeState::WaitingForApproval), "Approval");
        assert_eq!(state_label(&ClaudeState::Idle), "Idle");
    }
}
