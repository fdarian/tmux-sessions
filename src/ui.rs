use ansi_to_tui::IntoText;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::App;
use crate::event::Mode;
use crate::tree;

pub fn render(frame: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(frame.area());

    render_tree(frame, app, chunks[0]);
    render_preview(frame, app, chunks[1]);

    if app.mode == Mode::Confirming {
        render_confirmation(frame, app);
    }
}

fn render_tree(frame: &mut Frame, app: &mut App, area: Rect) {
    let key_width = if app.flat_entries.is_empty() {
        3
    } else {
        format!("({})", app.flat_entries.len() - 1).len()
    };

    let items: Vec<ListItem> = app
        .flat_entries
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let is_expanded = app.opened.contains(&entry.node_id);
            let line = tree::format_line(entry, i, is_expanded, key_width);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .highlight_style(app.highlight_style);

    if app.mode == Mode::Filtering {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(area);
        frame.render_stateful_widget(list, chunks[0], &mut app.list_state);
        frame.render_widget(
            Paragraph::new(format!("/ {}_", app.filter_query)),
            chunks[1],
        );
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

    if app.preview_panes.is_empty() {
        return;
    }

    let constraints: Vec<Constraint> = app.preview_panes.iter()
        .map(|_| Constraint::Ratio(1, app.preview_panes.len() as u32))
        .collect();

    let pane_areas = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(inner);

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
    let label = app
        .confirming_label()
        .unwrap_or_else(|| "item".to_string());
    let text = format!("Kill {}? (y/n)", label);

    let area = centered_rect(50, 5, frame.area());
    frame.render_widget(Clear, area);

    let popup = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title("Confirm"))
        .wrap(Wrap { trim: false });

    frame.render_widget(popup, area);
}

fn centered_rect(width_percent: u16, height: u16, area: Rect) -> Rect {
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
            Constraint::Percentage((100 - width_percent) / 2),
            Constraint::Percentage(width_percent),
            Constraint::Percentage((100 - width_percent) / 2),
        ])
        .split(vertical[1]);

    horizontal[1]
}
