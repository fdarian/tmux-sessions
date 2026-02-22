use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
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
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    frame.render_stateful_widget(list, area, &mut app.list_state);
}

fn render_preview(frame: &mut Frame, app: &App, area: Rect) {
    let title = match app.list_state.selected() {
        Some(i) => format!(" {} (sort: index) ", i),
        None => " Preview ".to_string(),
    };

    let preview = Paragraph::new(app.preview_content.as_str())
        .block(Block::default().borders(Borders::ALL).title(title))
        .wrap(Wrap { trim: false });

    frame.render_widget(preview, area);
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
