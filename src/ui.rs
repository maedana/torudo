use crate::app_state::AppState;
use crate::help::HELP_ENTRIES;
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

fn calc_todo_height(todo: &Item, available_width: u16) -> u16 {
    let spans = create_todo_spans(todo);
    let total_text_len: usize = spans.iter().map(|span| span.content.chars().count()).sum();

    if available_width > 10 {
        let effective_width = available_width.saturating_sub(2);
        let lines = u16::try_from(total_text_len)
            .unwrap_or(u16::MAX)
            .div_ceil(effective_width)
            .max(1);
        (lines + 2).min(8) // +2 for borders, cap at 8
    } else {
        4
    }
}

pub fn draw_project_column(
    f: &mut ratatui::Frame,
    project_todos: &[Item],
    project_name: &str,
    column_area: ratatui::layout::Rect,
    is_active_column: bool,
    selected_in_column: usize,
    scroll_offset: &mut usize,
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

    if project_todos.is_empty() {
        return;
    }

    let available_width = inner_area.width.saturating_sub(4);
    let available_height = inner_area.height;

    // Calculate heights for all todos
    let heights: Vec<u16> = project_todos
        .iter()
        .map(|todo| calc_todo_height(todo, available_width))
        .collect();

    // Adjust scroll_offset so selected item is visible (only for active column)
    if is_active_column {
        // Scroll up if selected is above viewport
        if selected_in_column < *scroll_offset {
            *scroll_offset = selected_in_column;
        }

        // Scroll down if selected is below viewport
        loop {
            let used: u16 = heights[*scroll_offset..=selected_in_column]
                .iter()
                .sum();
            if used <= available_height {
                break;
            }
            *scroll_offset += 1;
        }
    }

    // Determine visible range starting from scroll_offset
    let mut used_height: u16 = 0;
    let mut visible_end = *scroll_offset;
    for &h in &heights[*scroll_offset..] {
        if used_height + h > available_height {
            break;
        }
        used_height += h;
        visible_end += 1;
    }

    let visible_todos = &project_todos[*scroll_offset..visible_end];
    let visible_heights = &heights[*scroll_offset..visible_end];

    let constraints: Vec<Constraint> = visible_heights
        .iter()
        .map(|&h| Constraint::Length(h))
        .collect();

    let todo_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner_area);

    for (i, todo) in visible_todos.iter().enumerate() {
        let actual_idx = *scroll_offset + i;
        let spans = create_todo_spans(todo);
        let is_selected = is_active_column && actual_idx == selected_in_column;
        let (todo_style, background_style) = get_todo_styles(is_selected, todo.completed);

        let todo_paragraph = Paragraph::new(Line::from(spans))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(todo_style),
            )
            .style(background_style)
            .wrap(Wrap { trim: true });

        f.render_widget(todo_paragraph, todo_layout[i]);
    }
}

pub fn draw_ui(f: &mut ratatui::Frame, state: &mut AppState) {
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

    let visible_projects = state.visible_project_names();
    let num_columns = visible_projects.len();
    if num_columns > 0 {
        let column_constraints: Vec<Constraint> = (0..num_columns)
            .map(|_| Constraint::Percentage(100 / u16::try_from(num_columns).unwrap_or(1)))
            .collect();
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(column_constraints)
            .split(chunks[0]);

        for (col_idx, project_name) in visible_projects.iter().enumerate() {
            if let Some(project_todos) = state.grouped_todos.get(project_name) {
                let is_active_column = col_idx == state.current_column;
                let selected_for_this_column = if is_active_column {
                    state.selected_in_column
                } else {
                    usize::MAX
                };

                let mut col_scroll = if is_active_column {
                    state.scroll_offset
                } else {
                    0
                };

                draw_project_column(
                    f,
                    project_todos,
                    project_name,
                    columns[col_idx],
                    is_active_column,
                    selected_for_this_column,
                    &mut col_scroll,
                );

                if is_active_column {
                    state.scroll_offset = col_scroll;
                }
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
            let hidden = state.hidden_projects_display().map_or_else(String::new, |h| format!(" | {h}"));
            format!("{base}{claude_cmd} | v: Hide | V: Show all | ?: Help | q: Quit{hidden}")
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

    // Draw help overlay if shown
    if state.show_help {
        draw_help_overlay(f, size);
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

    let visible_height = inner_chunks[0].height as usize;
    let scroll_offset = if modal.selected >= visible_height {
        modal.selected - visible_height + 1
    } else {
        0
    };

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

    let list =
        Paragraph::new(lines).scroll((u16::try_from(scroll_offset).unwrap_or(u16::MAX), 0));
    f.render_widget(list, inner_chunks[0]);

    let help = Paragraph::new("j/k: Move | Space: Toggle | Enter: Import | q: Cancel")
        .style(Style::default())
        .alignment(Alignment::Center);
    f.render_widget(help, inner_chunks[1]);
}

fn draw_help_overlay(f: &mut ratatui::Frame, area: Rect) {
    let modal_area = centered_rect(50, 60, area);
    f.render_widget(Clear, modal_area);

    let block = Block::default()
        .title("Keyboard Controls")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(modal_area);
    f.render_widget(block, modal_area);

    let inner_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let max_key_width = HELP_ENTRIES
        .iter()
        .map(|e| e.key.len())
        .max()
        .unwrap_or(0);

    let lines: Vec<Line<'_>> = HELP_ENTRIES
        .iter()
        .map(|e| {
            let mut spans = Vec::new();
            if e.indent {
                // Use a non-whitespace-only span to prevent trim from eating indent
                spans.push(Span::styled(
                    "  ",
                    Style::default().fg(Color::DarkGray),
                ));
                spans.push(Span::styled(
                    format!("{:<width$}  ", e.key, width = max_key_width),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ));
            } else {
                spans.push(Span::styled(
                    format!("{:<width$}    ", e.key, width = max_key_width),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ));
            }
            spans.push(Span::styled(e.desc, Style::default().fg(Color::White)));
            Line::from(spans)
        })
        .collect();

    let list = Paragraph::new(lines);
    f.render_widget(list, inner_chunks[0]);

    let footer = Paragraph::new("Press ? or q or Esc to close")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    f.render_widget(footer, inner_chunks[1]);
}
