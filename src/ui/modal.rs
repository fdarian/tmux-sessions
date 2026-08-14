use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, Borders, Clear, List, ListItem, ListState, Padding, Paragraph, Wrap,
};

use crate::app::{App, RenameTarget};

pub fn render_confirmation(frame: &mut Frame, app: &App) {
    let text = match app.confirming_message() {
        Some(text) => text,
        None => return,
    };

    let area = super::centered_rect(40, 6, frame.area());
    frame.render_widget(Clear, area);

    let popup = Paragraph::new(text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Confirm")
                .padding(Padding::vertical(1)),
        )
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: false });

    frame.render_widget(popup, area);
}

pub fn render_rename_input(frame: &mut Frame, app: &App) {
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
            Style::default().bg(Color::White).fg(Color::Black),
        ),
        Span::raw(after),
    ]);
    let hint_line = Line::from(Span::styled(
        "Enter confirm · Esc cancel",
        Style::default().fg(Color::DarkGray),
    ));
    let text = Text::from(vec![input_line, hint_line]);

    let title = match app.renaming_target {
        Some(RenameTarget::Window(_)) => "Rename window",
        _ => "Rename session",
    };

    let area = super::centered_rect(50, 6, frame.area());
    frame.render_widget(Clear, area);

    let popup = Paragraph::new(text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .padding(Padding::vertical(1)),
        )
        .alignment(Alignment::Left);

    frame.render_widget(popup, area);
}

pub fn render_quick_create_input(frame: &mut Frame, app: &App) {
    let chars: Vec<char> = app.quick_create_buffer.chars().collect();
    let before: String = chars[..app.quick_create_cursor].iter().collect();
    let cursor_char = if app.quick_create_cursor < chars.len() {
        chars[app.quick_create_cursor].to_string()
    } else {
        " ".to_string()
    };
    let after: String = if app.quick_create_cursor < chars.len() {
        chars[app.quick_create_cursor + 1..].iter().collect()
    } else {
        String::new()
    };

    let input_line = Line::from(vec![
        Span::raw(before),
        Span::styled(
            cursor_char,
            Style::default().bg(Color::White).fg(Color::Black),
        ),
        Span::raw(after),
    ]);
    let hint_line = Line::from(Span::styled(
        "Enter confirm · Esc cancel · empty = random",
        Style::default().fg(Color::DarkGray),
    ));
    let text = Text::from(vec![input_line, hint_line]);

    let area = super::centered_rect(50, 6, frame.area());
    frame.render_widget(Clear, area);

    let mut block = Block::default()
        .borders(Borders::ALL)
        .title("New session")
        .padding(Padding::vertical(1));
    if let Some(err) = &app.quick_create_error {
        block = block.title_bottom(
            Line::from(vec![Span::styled(
                format!(" {} ", err),
                Style::default().fg(Color::Red).add_modifier(Modifier::DIM),
            )])
            .alignment(Alignment::Left),
        );
    }

    let popup = Paragraph::new(text).block(block).alignment(Alignment::Left);

    frame.render_widget(popup, area);
}

pub fn render_move_window(frame: &mut Frame, app: &App) {
    let area = super::centered_rect(60, 16, frame.area());
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
        let hint = Paragraph::new(Span::styled(
            "no matches - type a name to create",
            Style::default().fg(Color::DarkGray),
        ))
        .alignment(Alignment::Center);
        frame.render_widget(hint, chunks[0]);
    } else {
        let items: Vec<ListItem> = app
            .move_candidates
            .iter()
            .map(|candidate| {
                let item = ListItem::new(candidate.label.clone());
                if candidate.dim {
                    item.style(Style::default().add_modifier(Modifier::DIM))
                } else {
                    item
                }
            })
            .collect();
        let list = List::new(items).highlight_style(app.highlight_style);
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
            Style::default().bg(Color::White).fg(Color::Black),
        ),
        Span::raw(after),
    ]);
    frame.render_widget(Paragraph::new(search_line), chunks[1]);
}

pub fn render_about(frame: &mut Frame) {
    let name = env!("CARGO_PKG_NAME");
    let version = env!("CARGO_PKG_VERSION");
    let commit = env!("GIT_COMMIT");

    let text = Text::from(vec![
        Line::from(name).alignment(Alignment::Center),
        Line::from(format!("v{} ({})", version, commit)).alignment(Alignment::Center),
        Line::from(""),
        Line::from(Span::styled(
            "[esc] close",
            Style::default().add_modifier(Modifier::DIM),
        ))
        .alignment(Alignment::Center),
    ]);

    let area = super::centered_rect(34, 7, frame.area());
    frame.render_widget(Clear, area);

    let popup = Paragraph::new(text).block(
        Block::default()
            .borders(Borders::ALL)
            .title("About")
            .padding(Padding::vertical(1)),
    );

    frame.render_widget(popup, area);
}
