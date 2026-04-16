use crate::app_state::{AppState, ViewMode};
use crate::help;
use crate::md_preview::format_elapsed;
use crate::todo::Item;
use crate::url::strip_urls;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Flex, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Tabs},
};
use std::time::SystemTime;
use unicode_width::UnicodeWidthChar;

const SELECTED_ICON: &str = "> ";
const PENDING_FG: Color = Color::Rgb(120, 120, 120);
const MD_META_FG: Color = Color::Rgb(170, 170, 170);

fn selected_icon_span() -> Span<'static> {
    Span::styled(
        SELECTED_ICON,
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )
}

pub fn create_todo_spans(todo: &Item) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    if todo.completed {
        spans.push(Span::styled("✓ ", Style::default().fg(Color::Green)));
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
    let (display_text, has_urls) = strip_urls(&todo.description);
    if has_urls {
        spans.push(Span::styled("🔗 ", Style::default().fg(Color::Blue)));
    }
    spans.push(Span::raw(display_text));
    for context in &todo.contexts {
        spans.push(Span::styled(
            format!(" @{context}"),
            Style::default().fg(Color::Cyan),
        ));
    }
    spans
}

pub fn get_todo_border_style(is_selected: bool, is_overdue: bool, is_dimmed: bool) -> Style {
    if is_selected {
        Style::default().fg(Color::Yellow)
    } else if is_overdue {
        Style::default().fg(Color::Red)
    } else if is_dimmed {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::White)
    }
}

fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    if text.is_empty() || max_width == 0 {
        return vec![String::new()];
    }

    let mut lines = Vec::new();
    let mut current_line = String::new();
    let mut line_width: usize = 0;

    for ch in text.chars() {
        let ch_width = ch.width().unwrap_or(0);
        if line_width + ch_width > max_width && line_width > 0 {
            lines.push(current_line);
            current_line = String::new();
            line_width = 0;
        }
        current_line.push(ch);
        line_width += ch_width;
    }
    lines.push(current_line);

    lines
}

fn meta_label(todo: &Item, now: SystemTime) -> Option<String> {
    let meta = todo.md_meta.as_ref()?;
    let elapsed = format_elapsed(meta.mtime, now);
    Some(match meta.stats {
        Some((done, total)) => format!("{done}/{total} {elapsed}"),
        None => elapsed,
    })
}

fn calc_todo_height(todo: &Item, available_width: u16) -> u16 {
    let spans = create_todo_spans(todo);
    let text: String = spans.iter().map(|s| s.content.as_ref()).collect();

    let preview_lines = todo
        .md_meta
        .as_ref()
        .map_or(0, |m| u16::try_from(m.preview.len()).unwrap_or(0));
    if available_width > 10 {
        let effective_width = usize::from(available_width.saturating_sub(2));
        let lines = wrap_text(&text, effective_width).len();
        let lines_u16 = u16::try_from(lines).unwrap_or(u16::MAX);
        (lines_u16 + 2).min(8) + preview_lines // +2 for borders, body cap at 8
    } else {
        4 + preview_lines
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw_project_column(
    f: &mut ratatui::Frame,
    project_todos: &[Item],
    project_name: &str,
    column_area: ratatui::layout::Rect,
    is_active_column: bool,
    selected_in_column: usize,
    scroll_offset: usize,
    today: chrono::NaiveDate,
    now: SystemTime,
) -> usize {
    let border_style = if is_active_column {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::White)
    };

    let title_text = format!("{project_name} ({})", project_todos.len());
    let title_line = if is_active_column {
        Line::from(vec![selected_icon_span(), Span::raw(title_text)])
    } else {
        Line::from(title_text)
    };
    let project_block = Block::default()
        .title(title_line)
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner_area = project_block.inner(column_area);
    f.render_widget(project_block, column_area);

    if project_todos.is_empty() {
        return scroll_offset;
    }

    let available_width = inner_area.width;
    let available_height = inner_area.height;

    let heights: Vec<u16> = project_todos
        .iter()
        .map(|todo| calc_todo_height(todo, available_width))
        .collect();

    // Adjust scroll_offset so selected item is visible (only for active column)
    let mut offset = scroll_offset;
    if is_active_column {
        if selected_in_column < offset {
            offset = selected_in_column;
        }

        while offset < selected_in_column {
            let used: u16 = heights[offset..=selected_in_column].iter().sum();
            if used <= available_height {
                break;
            }
            offset += 1;
        }
    }

    // Determine visible range
    let mut used_height: u16 = 0;
    let mut visible_end = offset;
    for &h in &heights[offset..] {
        if used_height + h > available_height {
            break;
        }
        used_height += h;
        visible_end += 1;
    }

    let visible_todos = &project_todos[offset..visible_end];
    let visible_heights = &heights[offset..visible_end];

    let constraints: Vec<Constraint> = visible_heights
        .iter()
        .map(|&h| Constraint::Length(h))
        .collect();

    let todo_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .flex(Flex::Start)
        .split(inner_area);

    for (i, todo) in visible_todos.iter().enumerate() {
        let actual_idx = offset + i;
        let is_pending = todo.is_threshold_pending(today);
        let spans = create_todo_spans(todo);
        let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
        let is_selected = is_active_column && actual_idx == selected_in_column;
        let is_overdue = todo.is_overdue(today);
        let border_style =
            get_todo_border_style(is_selected, is_overdue, todo.completed || is_pending);

        let effective_width = usize::from(todo_layout[i].width.saturating_sub(2));
        let mut wrapped_lines: Vec<Line<'_>> = wrap_text(&text, effective_width)
            .into_iter()
            .map(Line::from)
            .collect();
        if let Some(meta) = &todo.md_meta {
            for preview_text in &meta.preview {
                wrapped_lines.push(Line::from(Span::styled(
                    format!("☐ {preview_text}"),
                    Style::default().fg(MD_META_FG),
                )));
            }
        }

        let mut block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style);
        if is_selected {
            block = block.title(selected_icon_span());
        }
        if let Some(label) = meta_label(todo, now) {
            block = block.title_bottom(
                Line::from(Span::styled(label, Style::default().fg(MD_META_FG))).right_aligned(),
            );
        }
        let mut todo_paragraph = Paragraph::new(wrapped_lines).block(block);
        if is_pending {
            todo_paragraph = todo_paragraph.style(Style::default().fg(PENDING_FG));
        }

        f.render_widget(todo_paragraph, todo_layout[i]);
    }

    offset
}

fn draw_tab_bar(f: &mut ratatui::Frame, state: &AppState, area: Rect) {
    let tab_titles: Vec<String> = ViewMode::ALL
        .iter()
        .enumerate()
        .map(|(i, m)| format!("{} ({})", m.label(), state.mode_counts[i]))
        .collect();
    let tabs = Tabs::new(tab_titles)
        .select(state.current_mode_index())
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    f.render_widget(tabs, area);
}

pub fn draw_ui(f: &mut ratatui::Frame, state: &mut AppState) {
    let now = SystemTime::now();
    let size = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(
            [
                Constraint::Length(1),
                Constraint::Min(0),
                Constraint::Length(3),
            ]
            .as_ref(),
        )
        .split(size);

    draw_tab_bar(f, state, chunks[0]);

    let visible_projects = &state.project_names;
    let num_columns = visible_projects.len();
    if num_columns > 0 {
        let today = chrono::Local::now().date_naive();
        let column_constraints: Vec<Constraint> = (0..num_columns)
            .map(|_| Constraint::Percentage(100 / u16::try_from(num_columns).unwrap_or(1)))
            .collect();
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(column_constraints)
            .split(chunks[1]);

        for (col_idx, project_name) in visible_projects.iter().enumerate() {
            if let Some(project_todos) = state.grouped_todos.get(project_name) {
                let is_active_column = col_idx == state.current_column;
                let selected_for_this_column = if is_active_column {
                    state.selected_in_column
                } else {
                    usize::MAX
                };

                let col_scroll = if is_active_column {
                    state.scroll_offset
                } else {
                    0
                };

                let new_scroll = draw_project_column(
                    f,
                    project_todos,
                    project_name,
                    columns[col_idx],
                    is_active_column,
                    selected_for_this_column,
                    col_scroll,
                    today,
                    now,
                );

                if is_active_column {
                    state.scroll_offset = new_scroll;
                }
            }
        }
    } else {
        let paragraph = Paragraph::new("No items")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(paragraph, chunks[1]);
    }

    let version = env!("CARGO_PKG_VERSION");
    let footer_spans: Vec<Span<'_>> = if let Some(ref msg) = state.status_message {
        vec![Span::styled(msg.clone(), Style::default().fg(Color::Green))]
    } else {
        let mut spans = vec![Span::raw(format!("torudo v{version}"))];
        if let Some(ref v) = state.update_available {
            spans.push(Span::styled(
                format!(" ({v} available! Run: torudo update)"),
                Style::default().fg(Color::Yellow),
            ));
        }
        let is_todo = state.view_mode == ViewMode::Todo;
        let is_waiting = state.view_mode == ViewMode::Waiting;
        let has_claude = state.crmux_available() || state.claude_available();
        let footer_str = help::footer_entries(is_todo, is_waiting, has_claude)
            .iter()
            .map(|(key, desc)| format!("{key}:{desc}"))
            .collect::<Vec<_>>()
            .join(" │ ");
        spans.push(Span::raw(format!(" │ {footer_str}")));
        spans
    };
    let footer = Paragraph::new(Line::from(footer_spans))
        .block(Block::default().borders(Borders::ALL))
        .alignment(Alignment::Center);

    f.render_widget(footer, chunks[2]);

    // Draw plan modal overlay if open
    if let Some(modal) = &state.plan_modal {
        draw_plan_modal(f, modal, size);
    }

    // Draw help overlay if shown
    if state.show_help {
        let has_claude = state.crmux_available() || state.claude_available();
        draw_help_overlay(f, size, state.view_mode, has_claude);
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

fn draw_plan_modal(f: &mut ratatui::Frame, modal: &crate::app_state::PlanModal, area: Rect) {
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

    let list = Paragraph::new(lines).scroll((u16::try_from(scroll_offset).unwrap_or(u16::MAX), 0));
    f.render_widget(list, inner_chunks[0]);

    let help = Paragraph::new("j/k: Move | Space: Toggle | Enter: Import | q: Cancel")
        .style(Style::default())
        .alignment(Alignment::Center);
    f.render_widget(help, inner_chunks[1]);
}

fn draw_help_overlay(f: &mut ratatui::Frame, area: Rect, view_mode: ViewMode, has_claude: bool) {
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

    let is_todo = view_mode == ViewMode::Todo;
    let is_waiting = view_mode == ViewMode::Waiting;
    let entries = help::visible_entries(is_todo, is_waiting, has_claude);

    let max_key_width = entries.iter().map(|e| e.key.len()).max().unwrap_or(0);

    let lines: Vec<Line<'_>> = entries
        .iter()
        .map(|e| {
            let mut spans = Vec::new();
            if e.indent {
                // Use a non-whitespace-only span to prevent trim from eating indent
                spans.push(Span::styled("  ", Style::default().fg(Color::DarkGray)));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::md_preview::MdMeta;
    use crate::todo::Item;
    use std::collections::HashMap;

    fn make_item(description: &str) -> Item {
        Item {
            completed: false,
            priority: None,
            creation_date: None,
            completion_date: None,
            description: description.to_string(),
            projects: vec![],
            contexts: vec![],
            id: None,
            key_values: HashMap::new(),
            line_number: 0,
            md_meta: None,
        }
    }

    #[test]
    fn calc_todo_height_cjk_short() {
        // "テスト" = 6 display cells
        // width=30 → effective_width=28 → ceil(6/28)=1 → height=3
        let item = make_item("テスト");
        assert_eq!(calc_todo_height(&item, 30), 3);
    }

    #[test]
    fn calc_todo_height_cjk_wraps() {
        // 15 CJK chars = 30 display cells
        // width=20 → effective_width=18 → ceil(30/18)=2 → height=4
        let item = make_item("あいうえおかきくけこさしすせそ");
        assert_eq!(calc_todo_height(&item, 20), 4);
    }

    fn meta_with(now: SystemTime, preview: Vec<String>, stats: Option<(usize, usize)>) -> MdMeta {
        MdMeta {
            mtime: now,
            preview,
            stats,
        }
    }

    #[test]
    fn calc_todo_height_adds_preview_lines() {
        let mut item = make_item("テスト");
        let baseline = calc_todo_height(&item, 30);
        item.md_meta = Some(meta_with(
            SystemTime::now(),
            vec!["a".into(), "b".into(), "c".into()],
            None,
        ));
        assert_eq!(calc_todo_height(&item, 30), baseline + 3);
    }

    #[test]
    fn calc_todo_height_no_preview_unchanged() {
        let item = make_item("テスト");
        assert_eq!(calc_todo_height(&item, 30), 3);
    }

    #[test]
    fn meta_label_none_when_no_meta() {
        let item = make_item("x");
        assert!(meta_label(&item, SystemTime::now()).is_none());
    }

    #[test]
    fn meta_label_elapsed_only_when_no_stats() {
        let mut item = make_item("x");
        let now = SystemTime::now();
        item.md_meta = Some(meta_with(now, vec![], None));
        assert_eq!(meta_label(&item, now).as_deref(), Some(" 0s"));
    }

    #[test]
    fn meta_label_includes_done_total() {
        let mut item = make_item("x");
        let now = SystemTime::now();
        item.md_meta = Some(meta_with(now, vec![], Some((2, 7))));
        assert_eq!(meta_label(&item, now).as_deref(), Some("2/7  0s"));
    }

    fn render_paragraph_to_lines(description: &str, width: u16, height: u16) -> Vec<String> {
        use ratatui::backend::TestBackend;

        let item = make_item(description);
        let spans = create_todo_spans(&item);

        let area = Rect::new(0, 0, width, height);
        let backend = TestBackend::new(width, height);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();

        let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
        let effective_width = usize::from(width.saturating_sub(2));
        let wrapped_lines: Vec<Line<'_>> = wrap_text(&text, effective_width)
            .into_iter()
            .map(Line::from)
            .collect();

        terminal
            .draw(|f| {
                let p = Paragraph::new(wrapped_lines).block(Block::default().borders(Borders::ALL));
                f.render_widget(p, area);
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        (1..height - 1)
            .map(|y| {
                let row: String = (1..width - 1)
                    .map(|x| buf.cell((x, y)).unwrap().symbol().to_string())
                    .collect();
                row.trim_end().to_string()
            })
            .collect()
    }

    #[test]
    fn render_cjk_paragraph_actual_lines() {
        // Render into a width that should produce 2 wrapped lines
        let lines = render_paragraph_to_lines("あいうえおかきくけこさしすせそ", 20, 6);
        eprintln!("rendered lines: {lines:?}");
        assert!(
            !lines[0].is_empty(),
            "first content row should not be blank"
        );

        let content_line_count = lines.iter().filter(|l| !l.is_empty()).count();
        eprintln!("content line count: {content_line_count}");
    }

    fn assert_height_matches_render(desc: &str, width: u16) {
        let calc_h = calc_todo_height(&make_item(desc), width);
        let lines = render_paragraph_to_lines(desc, width, 14);
        let actual_content_lines =
            u16::try_from(lines.iter().filter(|l| !l.is_empty()).count()).unwrap();
        let actual_h = actual_content_lines + 2;

        eprintln!("desc={desc}");
        eprintln!("lines={lines:?}");
        eprintln!("calc_h={calc_h}, actual_h={actual_h}, content_lines={actual_content_lines}");
        assert_eq!(
            calc_h, actual_h,
            "height mismatch for \"{desc}\" at width={width}"
        );
    }

    #[test]
    fn calc_todo_height_matches_actual_render_cjk() {
        assert_height_matches_render("あいうえおかきくけこさしすせそ", 20);
    }

    #[test]
    fn calc_todo_height_matches_actual_render_with_spaces() {
        // Word wrapping at spaces can produce more lines than simple ceil division
        assert_height_matches_render(
            "[要望] テストの確認のために結果投稿のレスポンスにURLが欲しい [#12345] https://example.com/projects/52/tasks/12345",
            30,
        );
    }

    #[test]
    fn get_todo_border_style_dimmed_is_darkgray() {
        let style = get_todo_border_style(false, false, true);
        assert_eq!(style, Style::default().fg(Color::DarkGray));
    }

    #[test]
    fn get_todo_border_style_overdue_is_red() {
        let style = get_todo_border_style(false, true, false);
        assert_eq!(style, Style::default().fg(Color::Red));
    }

    #[test]
    fn get_todo_border_style_selected_trumps_overdue() {
        let style = get_todo_border_style(true, true, false);
        assert_eq!(style, Style::default().fg(Color::Yellow));
    }

    #[test]
    fn get_todo_border_style_overdue_trumps_dimmed() {
        let style = get_todo_border_style(false, true, true);
        assert_eq!(style, Style::default().fg(Color::Red));
    }
}
