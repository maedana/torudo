use crate::app_state::AppState;
use crate::todo::Item;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

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

pub fn draw_ui(f: &mut ratatui::Frame, state: &AppState) {
    let size = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(
            [
                Constraint::Min(0),
                Constraint::Length(3),
            ]
            .as_ref(),
        )
        .split(size);

    let num_columns = state.project_names.len();
    if num_columns > 0 {
        let column_constraints: Vec<Constraint> = (0..num_columns)
            .map(|_| Constraint::Percentage(100 / u16::try_from(num_columns).unwrap_or(1)))
            .collect();
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(column_constraints)
            .split(chunks[0]);

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
    }

    let version = env!("CARGO_PKG_VERSION");
    let footer_text = state.status_message.as_ref().map_or_else(
        || {
            let base = format!("torudo v{version} | hjkl: Nav | x: Complete | r: Reload");
            let claude_cmd = if state.crmux_available() || state.claude_available() {
                " | c: Claude"
            } else {
                ""
            };
            format!("{base}{claude_cmd} | q: Quit")
        },
        Clone::clone,
    );
    let footer_style = if state.status_message.is_some() {
        Style::default().fg(Color::Green)
    } else {
        Style::default()
    };
    let footer = Paragraph::new(footer_text)
        .block(Block::default().borders(Borders::ALL))
        .style(footer_style)
        .alignment(Alignment::Center);

    f.render_widget(footer, chunks[1]);

    // Draw plan modal overlay if open
    if let Some(modal) = &state.plan_modal {
        draw_plan_modal(f, modal, size);
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn draw_plan_modal(
    f: &mut ratatui::Frame,
    modal: &crate::app_state::PlanModal,
    area: Rect,
) {
    let modal_area = centered_rect(60, 60, area);
    f.render_widget(Clear, modal_area);

    let block = Block::default()
        .title("Get Plans")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(modal_area);
    f.render_widget(block, modal_area);

    // Split inner area: list + help text
    let inner_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let lines: Vec<Line<'_>> = modal
        .plans
        .iter()
        .enumerate()
        .map(|(i, plan)| {
            let checkbox = if modal.checked[i] { "[x] " } else { "[ ] " };
            let text = format!("{checkbox}{}: {}", plan.project_name, plan.title);
            let style = if i == modal.selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            Line::from(Span::styled(text, style))
        })
        .collect();

    let list = Paragraph::new(lines).wrap(Wrap { trim: true });
    f.render_widget(list, inner_chunks[0]);

    let help = Paragraph::new("j/k: Move | Space: Toggle | Enter: Import | q: Cancel")
        .style(Style::default())
        .alignment(Alignment::Center);
    f.render_widget(help, inner_chunks[1]);
}
