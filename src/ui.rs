use std::collections::HashSet;

use ansi_to_tui::IntoText;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Padding, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, MonitorSort, RenameTarget};
use crate::event::Mode;
use crate::procs::{self, ProcessRow};
use crate::tree::{self, NodeId};

pub fn render(frame: &mut Frame, app: &mut App) {
    if app.mode == Mode::Previewing {
        render_full_preview(frame, app, frame.area());
        return;
    }

    if app.mode == Mode::Monitor
        || app.mode == Mode::ProcessDetail
        || (app.mode == Mode::Confirming && app.confirming_process.is_some())
    {
        render_monitor(frame, app, frame.area());
        if app.mode == Mode::Confirming {
            render_confirmation(frame, app);
        }
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(frame.area());

    render_tree(frame, app, chunks[0]);
    render_preview(frame, app, chunks[1]);

    if app.mode == Mode::Confirming {
        render_confirmation(frame, app);
    }

    if app.mode == Mode::Renaming {
        render_rename_input(frame, app);
    }

    if app.mode == Mode::MoveWindow {
        render_move_window(frame, app);
    }

    if app.mode == Mode::About {
        render_about(frame);
    }
}

fn render_tree(frame: &mut Frame, app: &mut App, area: Rect) {
    let key_width = if app.flat_entries.len() > 10 { 5 } else { 3 };

    let hidden_session_ids: HashSet<String> = app.sessions.iter()
        .filter(|s| app.hidden.contains(&s.name))
        .map(|s| s.id.clone())
        .collect();

    let mut shortcut_index = 0usize;
    let mut items = Vec::with_capacity(app.flat_entries.len());
    for entry in &app.flat_entries {
        let is_expanded = app.opened.contains(&entry.node_id);
        let display_index = if matches!(entry.node_id, NodeId::Group(_)) { usize::MAX } else { shortcut_index };
        let raw_line = tree::format_line(entry, display_index, is_expanded, key_width);
        if entry.node_id != NodeId::Separator && !matches!(entry.node_id, NodeId::Group(_)) {
            shortcut_index += 1;
        }
        let is_marked = match &entry.node_id {
            NodeId::Window(_, window_id) => app.marked_windows.contains(window_id),
            _ => false,
        };
        let mut spans = Vec::new();
        if is_marked {
            spans.push(Span::styled(
                "● ",
                Style::default()
                    .fg(app.primary_color)
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            spans.push(Span::raw("  "));
        }
        let line_style = raw_line.style;
        let line_alignment = raw_line.alignment;
        for span in raw_line.spans {
            spans.push(span);
        }
        let mut line = Line::from(spans);
        line.style = line_style;
        line.alignment = line_alignment;
        let item = if is_marked {
            ListItem::new(line).style(Style::default().fg(app.primary_color))
        } else {
            ListItem::new(line)
        };
        let is_hidden = match &entry.node_id {
            NodeId::Session(id) | NodeId::Window(id, _) | NodeId::Pane(id, _, _) => {
                hidden_session_ids.contains(id)
            }
            _ => false,
        };
        if matches!(entry.node_id, NodeId::DeadSession(_)) || is_hidden {
            items.push(item.style(Style::default().add_modifier(Modifier::DIM)));
        } else {
            items.push(item);
        }
    }

    let list = List::new(items)
        .highlight_style(app.highlight_style);

    if app.mode == Mode::Filtering {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(area);
        frame.render_stateful_widget(list, chunks[0], &mut app.list_state);

        let chars: Vec<char> = app.filter_query.chars().collect();
        let before: String = chars[..app.filter_cursor].iter().collect();
        let cursor_char = if app.filter_cursor < chars.len() {
            chars[app.filter_cursor].to_string()
        } else {
            " ".to_string()
        };
        let after: String = if app.filter_cursor < chars.len() {
            chars[app.filter_cursor + 1..].iter().collect()
        } else {
            String::new()
        };
        let filter_line = ratatui::text::Line::from(vec![
            ratatui::text::Span::raw(format!("/ {}", before)),
            ratatui::text::Span::styled(
                cursor_char,
                ratatui::style::Style::default()
                    .bg(ratatui::style::Color::White)
                    .fg(ratatui::style::Color::Black),
            ),
            ratatui::text::Span::raw(after),
        ]);
        frame.render_widget(Paragraph::new(filter_line), chunks[1]);
    } else {
        frame.render_stateful_widget(list, area, &mut app.list_state);
    }
}

fn render_preview(frame: &mut Frame, app: &App, area: Rect) {
    let title = if app.preview_title.is_empty() {
        " Preview ".to_string()
    } else {
        format!(" {} ", app.preview_title)
    };

    let outer_block = Block::default().borders(Borders::ALL).title(title);
    let inner = outer_block.inner(area);
    frame.render_widget(outer_block, area);

    let preview_area = if app.selecting {
        let marked_count = app.marked_windows.len();
        let hint = format!(
            " {} selected · j/k extend · v done · M move · x delete · Esc clear ",
            marked_count
        );
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
            .split(inner);
        frame.render_widget(
            Paragraph::new(hint).style(Style::default().fg(app.primary_color)),
            chunks[0],
        );
        chunks[1]
    } else if !app.marked_windows.is_empty() {
        let marked_count = app.marked_windows.len();
        let hint = format!(
            " {} selected · M move · x delete · v reselect · Esc clear ",
            marked_count
        );
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
            .split(inner);
        frame.render_widget(
            Paragraph::new(hint).style(Style::default().fg(app.primary_color)),
            chunks[0],
        );
        chunks[1]
    } else {
        inner
    };

    if app.preview_panes.is_empty() {
        if let Some(preview_notice) = app.preview_notice.as_ref() {
            frame.render_widget(
                Paragraph::new(preview_notice.as_str())
                    .style(Style::default().add_modifier(Modifier::DIM)),
                preview_area,
            );
        }
        return;
    }

    let constraints: Vec<Constraint> = app.preview_panes.iter()
        .map(|_| Constraint::Ratio(1, app.preview_panes.len() as u32))
        .collect();

    let pane_areas = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(preview_area);

    for (idx, preview_pane) in app.preview_panes.iter().enumerate() {
        let pane_area = pane_areas[idx];

        let pane_inner = if idx > 0 {
            let pane_block = Block::default().borders(Borders::LEFT);
            let inner = pane_block.inner(pane_area);
            frame.render_widget(pane_block, pane_area);
            inner
        } else {
            pane_area
        };

        let content = preview_pane.content.as_slice().into_text().unwrap_or_default();
        let paragraph = Paragraph::new(content);
        frame.render_widget(paragraph, pane_inner);

        // Render label overlay centered in the pane
        let label_text = format!(" {} ", preview_pane.label);
        let label_width = label_text.len() as u16 + 2; // +2 for border
        let label_height = 3u16; // top border + text + bottom border

        if pane_area.width >= label_width && pane_area.height >= label_height {
            let label_area = Rect::new(
                pane_area.x + (pane_area.width.saturating_sub(label_width)) / 2,
                pane_area.y + (pane_area.height.saturating_sub(label_height)) / 2,
                label_width.min(pane_area.width),
                label_height,
            );

            let label_color = if preview_pane.is_active {
                app.primary_color
            } else {
                Color::DarkGray
            };

            let label_block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::White));

            let label_inner = label_block.inner(label_area);
            frame.render_widget(Clear, label_area);
            frame.render_widget(label_block, label_area);
            frame.render_widget(
                Paragraph::new(Span::styled(label_text.trim(), Style::default().fg(label_color)))
                    .alignment(Alignment::Center),
                label_inner,
            );
        }
    }
}

fn render_confirmation(frame: &mut Frame, app: &App) {
    let text = match app.confirming_message() {
        Some(text) => text,
        None => return,
    };

    let area = centered_rect(40, 6, frame.area());
    frame.render_widget(Clear, area);

    let popup = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title("Confirm").padding(Padding::vertical(1)))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: false });

    frame.render_widget(popup, area);
}

fn render_rename_input(frame: &mut Frame, app: &App) {
    let chars: Vec<char> = app.rename_buffer.chars().collect();
    let before: String = chars[..app.rename_cursor].iter().collect();
    let cursor_char = if app.rename_cursor < chars.len() {
        chars[app.rename_cursor].to_string()
    } else {
        " ".to_string()
    };
    let after: String = if app.rename_cursor < chars.len() {
        chars[app.rename_cursor + 1..].iter().collect()
    } else {
        String::new()
    };

    let input_line = Line::from(vec![
        Span::raw(before),
        Span::styled(
            cursor_char,
            Style::default()
                .bg(Color::White)
                .fg(Color::Black),
        ),
        Span::raw(after),
    ]);
    let hint_line = Line::from(
        Span::styled(
            "Enter confirm · Esc cancel",
            Style::default().fg(Color::DarkGray),
        )
    );
    let text = Text::from(vec![input_line, hint_line]);

    let title = match app.renaming_target {
        Some(RenameTarget::Window(_)) => "Rename window",
        _ => "Rename session",
    };

    let area = centered_rect(50, 6, frame.area());
    frame.render_widget(Clear, area);

    let popup = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title(title).padding(Padding::vertical(1)))
        .alignment(Alignment::Left);

    frame.render_widget(popup, area);
}

fn render_move_window(frame: &mut Frame, app: &App) {
    let area = centered_rect(60, 16, frame.area());
    frame.render_widget(Clear, area);

    let marked_count = app.marked_windows.len();
    let title = if marked_count == 1 {
        " Move 1 window -> session ".to_string()
    } else {
        format!(" Move {} windows -> session ", marked_count)
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.primary_color))
        .title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(inner);

    if app.move_candidates.is_empty() {
        let hint = Paragraph::new(
            Span::styled(
                "no matches - type a name to create",
                Style::default().fg(Color::DarkGray),
            )
        )
            .alignment(Alignment::Center);
        frame.render_widget(hint, chunks[0]);
    } else {
        let items: Vec<ListItem> = app.move_candidates.iter().map(|candidate| {
            let item = ListItem::new(candidate.label.clone());
            if candidate.dim {
                item.style(Style::default().add_modifier(Modifier::DIM))
            } else {
                item
            }
        }).collect();
        let list = List::new(items)
            .highlight_style(app.highlight_style);
        let mut state = ListState::default();
        state.select(Some(app.move_selected));
        frame.render_stateful_widget(list, chunks[0], &mut state);
    }

    let chars: Vec<char> = app.move_query.chars().collect();
    let before: String = chars[..app.move_cursor].iter().collect();
    let cursor_char = if app.move_cursor < chars.len() {
        chars[app.move_cursor].to_string()
    } else {
        " ".to_string()
    };
    let after: String = if app.move_cursor < chars.len() {
        chars[app.move_cursor + 1..].iter().collect()
    } else {
        String::new()
    };
    let search_line = Line::from(vec![
        Span::raw(format!("Search: {}", before)),
        Span::styled(
            cursor_char,
            Style::default()
                .bg(Color::White)
                .fg(Color::Black),
        ),
        Span::raw(after),
    ]);
    frame.render_widget(Paragraph::new(search_line), chunks[1]);
}

fn render_about(frame: &mut Frame) {
    let name = env!("CARGO_PKG_NAME");
    let version = env!("CARGO_PKG_VERSION");
    let commit = env!("GIT_COMMIT");

    let text = Text::from(vec![
        Line::from(name).alignment(Alignment::Center),
        Line::from(format!("v{} ({})", version, commit)).alignment(Alignment::Center),
        Line::from(""),
        Line::from(
            Span::styled("[esc] close", Style::default().add_modifier(Modifier::DIM))
        ).alignment(Alignment::Center),
    ]);

    let area = centered_rect(34, 7, frame.area());
    frame.render_widget(Clear, area);

    let popup = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title("About").padding(Padding::vertical(1)));

    frame.render_widget(popup, area);
}

const MONITOR_MEM_WIDTH: usize = 8;
const MONITOR_CPU_WIDTH: usize = 7;
const MONITOR_COL_GAP: usize = 2;
const MONITOR_COMMAND_WIDTH: usize = 28;

fn monitor_pane_width(inner_width: usize) -> usize {
    let fixed = MONITOR_MEM_WIDTH
        + MONITOR_COL_GAP
        + MONITOR_CPU_WIDTH
        + MONITOR_COL_GAP
        + MONITOR_COMMAND_WIDTH
        + MONITOR_COL_GAP;
    inner_width.saturating_sub(fixed).max(1)
}

fn monitor_detail_items(row: &ProcessRow) -> Vec<ListItem<'_>> {
    let dim = Style::default().add_modifier(Modifier::DIM);
    let mut lines: Vec<String> = Vec::new();
    lines.push(format!("  ├ {}", row.command));
    lines.push(format!("  ├ cwd: {}", row.pane.cwd));
    lines.push(format!("  ├ pid: {}", row.pid));
    if row.ancestors.is_empty() {
        lines.push("  └ (none)".to_string());
    } else {
        let ancestor_count = row.ancestors.len();
        for i in 0..ancestor_count {
            let ancestor = &row.ancestors[i];
            let connector = if i + 1 == ancestor_count { "  └ " } else { "  ├ " };
            lines.push(format!(
                "{}{} ({})",
                connector, ancestor.command, ancestor.pid
            ));
        }
    }
    lines
        .into_iter()
        .map(|text| ListItem::new(Line::from(Span::styled(text, dim))))
        .collect()
}

fn format_monitor_cell(value: &str, width: usize, align_right: bool) -> String {
    let truncated = procs::truncate_chars(value, width);
    if align_right {
        format!("{:>width$}", truncated, width = width)
    } else {
        format!("{:<width$}", truncated, width = width)
    }
}

fn render_monitor(frame: &mut Frame, app: &mut App, area: Rect) {
    let sort_label = match app.monitor_sort {
        MonitorSort::Mem => "MEM",
        MonitorSort::Cpu => "CPU",
    };
    let title = format!(" Process Monitor (sort: {}) ", sort_label);

    let outer_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    let block = Block::default().borders(Borders::ALL).title(title);
    let inner = block.inner(outer_chunks[0]);
    frame.render_widget(block, outer_chunks[0]);

    let inner_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(inner);

    let pane_width = monitor_pane_width(inner.width as usize);
    let gap = " ".repeat(MONITOR_COL_GAP);

    let header = Line::from(vec![
        Span::styled(
            format_monitor_cell("MEM", MONITOR_MEM_WIDTH, true),
            Style::default().fg(Color::DarkGray),
        ),
        Span::raw(gap.clone()),
        Span::styled(
            format_monitor_cell("CPU", MONITOR_CPU_WIDTH, true),
            Style::default().fg(Color::DarkGray),
        ),
        Span::raw(gap.clone()),
        Span::styled(
            format_monitor_cell("COMMAND", MONITOR_COMMAND_WIDTH, false),
            Style::default().fg(Color::DarkGray),
        ),
        Span::raw(gap),
        Span::styled(
            format_monitor_cell("PANE", pane_width, false),
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    frame.render_widget(Paragraph::new(header), inner_chunks[0]);

    let show_detail = app.mode == Mode::ProcessDetail;
    let mut items: Vec<ListItem> = Vec::new();
    for i in 0..app.monitor_rows.len() {
        let row = &app.monitor_rows[i];
        let mem = format_monitor_cell(&procs::format_rss_kb(row.rss_kb), MONITOR_MEM_WIDTH, true);
        let cpu = format_monitor_cell(&procs::format_pcpu(row.pcpu), MONITOR_CPU_WIDTH, true);
        let command = format_monitor_cell(
            &procs::command_basename(&row.command),
            MONITOR_COMMAND_WIDTH,
            false,
        );
        let pane = format_monitor_cell(
            &procs::format_pane_label(&row.pane),
            pane_width,
            false,
        );
        let line = Line::from(vec![
            Span::raw(mem),
            Span::raw("  "),
            Span::raw(cpu),
            Span::raw("  "),
            Span::raw(command),
            Span::raw("  "),
            Span::raw(pane),
        ]);
        items.push(ListItem::new(line));
        if show_detail && i == app.monitor_selected {
            items.extend(monitor_detail_items(row));
        }
    }

    let list = List::new(items)
        .highlight_style(app.highlight_style);

    frame.render_stateful_widget(list, inner_chunks[1], &mut app.monitor_list_state);

    let footer = Paragraph::new(
        "[j/k] move  [s] sort  [space] details  [enter] switch  [x] kill  [esc/q] back"
    )
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(footer, outer_chunks[1]);
}

fn render_full_preview(frame: &mut Frame, app: &App, area: Rect) {
    let preview = match app.preview_full_panes.get(app.preview_full_index) {
        Some(p) => p,
        None => return,
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    let title = format!(
        " {} — {} — {}  ({}/{}) ",
        preview.session_name,
        preview.window_label,
        preview.pane_label,
        app.preview_full_index + 1,
        app.preview_full_panes.len()
    );

    let outer_block = Block::default().borders(Borders::ALL).title(title);
    let inner = outer_block.inner(chunks[0]);
    frame.render_widget(outer_block, chunks[0]);

    let content = preview.content.as_slice().into_text().unwrap_or_default();
    let paragraph = Paragraph::new(content);
    frame.render_widget(paragraph, inner);

    let footer_text = if app.preview_full_panes.len() > 1 {
        "[h] prev  [l] next  [esc] back  [enter] switch"
    } else {
        "[esc] back  [enter] switch"
    };
    let footer = Paragraph::new(footer_text)
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(footer, chunks[1]);
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length((area.height.saturating_sub(height)) / 2),
            Constraint::Length(height),
            Constraint::Min(0),
        ])
        .split(area);

    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(width),
            Constraint::Min(0),
        ])
        .split(vertical[1]);

    horizontal[1]
}
