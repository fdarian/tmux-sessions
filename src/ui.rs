use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;
use tui_tree_widget::Tree;

use crate::app::App;
use crate::event::Mode;

pub fn render(frame: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(frame.area());

    render_tree(frame, app, chunks[0]);
    render_preview(frame, app, chunks[1]);

    if app.mode == Mode::Confirming {
        render_confirmation(frame, app);
    }
}

fn render_tree(frame: &mut Frame, app: &mut App, area: Rect) {
    let tree = Tree::new(&app.tree_items)
        .expect("duplicate root node id")
        .block(Block::default().borders(Borders::ALL).title("Sessions"))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("▶ ");

    frame.render_stateful_widget(tree, area, &mut app.tree_state);
}

fn render_preview(frame: &mut Frame, app: &App, area: Rect) {
    let preview = Paragraph::new(app.preview_content.as_str())
        .block(Block::default().borders(Borders::ALL).title("Preview"))
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
