use ansi_to_tui::IntoText;
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::App;

pub fn render_full_preview(frame: &mut Frame, app: &App, area: Rect) {
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
    let footer = Paragraph::new(footer_text).style(Style::default().fg(Color::DarkGray));
    frame.render_widget(footer, chunks[1]);
}
