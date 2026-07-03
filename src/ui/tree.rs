use std::collections::HashSet;

use ansi_to_tui::IntoText;
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};

use crate::app::App;
use crate::event::Mode;
use crate::tree::{self, NodeId};

pub fn render_tree(frame: &mut Frame, app: &mut App, area: Rect) {
    let key_width = if app.flat_entries.len() > 10 { 5 } else { 3 };

    let hidden_session_ids: HashSet<String> = app
        .sessions
        .iter()
        .filter(|s| app.hidden.contains(&s.name))
        .map(|s| s.id.clone())
        .collect();

    let mut shortcut_index = 0usize;
    let mut items = Vec::with_capacity(app.flat_entries.len());
    for entry in &app.flat_entries {
        let is_expanded = app.opened.contains(&entry.node_id);
        let display_index = if tree::is_shortcut_labeled(&entry.node_id) {
            shortcut_index
        } else {
            usize::MAX
        };
        let raw_line = tree::format_line(entry, display_index, is_expanded, key_width);
        if tree::is_shortcut_labeled(&entry.node_id) {
            shortcut_index += 1;
        }
        let is_marked = match entry.node_id.target() {
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
        let is_hidden = match entry.node_id.target() {
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

    let list = List::new(items).highlight_style(app.highlight_style);

    if app.mode == Mode::Filtering || !app.filter_query.is_empty() {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(area);
        frame.render_stateful_widget(list, chunks[0], &mut app.list_state);

        let filter_line = if app.mode == Mode::Filtering {
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
            ratatui::text::Line::from(vec![
                ratatui::text::Span::raw(format!("/ {}", before)),
                ratatui::text::Span::styled(
                    cursor_char,
                    ratatui::style::Style::default()
                        .bg(ratatui::style::Color::White)
                        .fg(ratatui::style::Color::Black),
                ),
                ratatui::text::Span::raw(after),
            ])
        } else {
            ratatui::text::Line::from(ratatui::text::Span::styled(
                format!("/ {}", app.filter_query),
                ratatui::style::Style::default().add_modifier(Modifier::DIM),
            ))
        };
        frame.render_widget(Paragraph::new(filter_line), chunks[1]);
    } else {
        frame.render_stateful_widget(list, area, &mut app.list_state);
    }
}

pub fn render_preview(frame: &mut Frame, app: &App, area: Rect) {
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

    let constraints: Vec<Constraint> = app
        .preview_panes
        .iter()
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

        let content = preview_pane
            .content
            .as_slice()
            .into_text()
            .unwrap_or_default();
        let paragraph = Paragraph::new(content);
        frame.render_widget(paragraph, pane_inner);

        let label_text = format!(" {} ", preview_pane.label);
        let label_width = label_text.len() as u16 + 2;
        let label_height = 3u16;

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
                Paragraph::new(Span::styled(
                    label_text.trim(),
                    Style::default().fg(label_color),
                ))
                .alignment(Alignment::Center),
                label_inner,
            );
        }
    }
}
