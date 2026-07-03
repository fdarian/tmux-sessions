use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};

use crate::app::App;
use crate::create::{CreateCandidate, CreateTab};

pub fn render_create_session(frame: &mut Frame, app: &App) {
    let max_width = frame.area().width.saturating_sub(2).max(1);
    let max_height = frame.area().height.saturating_sub(2).max(1);
    let popup_width = ((frame.area().width.saturating_mul(4)) / 5)
        .max(68)
        .min(max_width);
    let popup_height = 24.min(max_height);
    let area = super::centered_rect(popup_width, popup_height, frame.area());
    frame.render_widget(Clear, area);

    let mut tab_spans = Vec::new();
    for index in 0..app.create_available_tabs.len() {
        if index > 0 {
            tab_spans.push(Span::raw("  "));
        }
        let tab = app.create_available_tabs[index];
        let style = if tab == app.create_tab {
            Style::default()
                .fg(app.primary_color)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().add_modifier(Modifier::DIM)
        };
        tab_spans.push(Span::styled(tab.label().to_string(), style));
    }

    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.primary_color))
        .title(Line::from(" Open Session ").alignment(Alignment::Left))
        .title(Line::from(tab_spans).alignment(Alignment::Right));
    if app.create_available_tabs.len() > 1 {
        block = block.title_bottom(
            Line::from(vec![Span::styled(
                " Tab / Shift+Tab switch mode ",
                Style::default().add_modifier(Modifier::DIM),
            )])
            .alignment(Alignment::Right),
        );
    }
    if let Some(err) = &app.create_load_error {
        block = block.title_bottom(
            Line::from(vec![Span::styled(
                format!(" {} ", err),
                Style::default().fg(Color::Red).add_modifier(Modifier::DIM),
            )])
            .alignment(Alignment::Left),
        );
    }
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(inner);

    if app.create_candidates.is_empty() {
        let message = if app.create_tab == CreateTab::History {
            "no matches - type a name to create"
        } else if app.create_tab == CreateTab::Worktree
            && app
                .config
                .as_ref()
                .and_then(|c| c.worktree_create_command.as_ref())
                .is_some()
        {
            "type a branch name to create a new worktree"
        } else {
            "no matches"
        };
        let hint = Paragraph::new(Span::styled(message, Style::default().fg(Color::DarkGray)))
            .alignment(Alignment::Center);
        frame.render_widget(hint, chunks[0]);
    } else {
        let list_width = chunks[0].width as usize;
        let score_width = app
            .create_candidates
            .iter()
            .filter_map(|candidate| candidate.frecency)
            .map(|frecency| format!("{frecency:.1}").len())
            .max()
            .unwrap_or(0);
        let items: Vec<ListItem> = app
            .create_candidates
            .iter()
            .map(|candidate| {
                let line = create_candidate_line(candidate, list_width, score_width);
                ListItem::new(line)
            })
            .collect();
        let list = List::new(items).highlight_style(app.highlight_style);
        let mut state = ListState::default();
        state.select(Some(app.create_selected));
        frame.render_stateful_widget(list, chunks[0], &mut state);
    }

    let chars: Vec<char> = app.create_query.chars().collect();
    let before: String = chars[..app.create_cursor].iter().collect();
    let cursor_char = if app.create_cursor < chars.len() {
        chars[app.create_cursor].to_string()
    } else {
        " ".to_string()
    };
    let after: String = if app.create_cursor < chars.len() {
        chars[app.create_cursor + 1..].iter().collect()
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
    let counter = format!(
        "{}/{}",
        app.create_candidates.len(),
        app.create_total_count()
    );
    let footer_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(counter.len() as u16 + 1),
        ])
        .split(chunks[1]);
    frame.render_widget(Paragraph::new(search_line), footer_chunks[0]);
    frame.render_widget(
        Paragraph::new(Span::styled(
            counter,
            Style::default().add_modifier(Modifier::DIM),
        ))
        .alignment(Alignment::Right),
        footer_chunks[1],
    );
}

fn create_candidate_line(
    candidate: &CreateCandidate,
    width: usize,
    score_width: usize,
) -> Line<'static> {
    let mut spans = Vec::new();
    let mut content_width = width;

    if let Some(frecency) = candidate.frecency {
        let score_text = format!("{frecency:.1}");
        let padded_score = format!("{score_text:>width$}", width = score_width);
        spans.push(Span::styled(
            padded_score,
            Style::default().add_modifier(Modifier::DIM),
        ));
        spans.push(Span::raw(" "));
        content_width = content_width.saturating_sub(score_width + 1);
    }

    append_primary_spans(&mut spans, &candidate.primary, &candidate.match_indices);

    if let Some(secondary) = candidate.secondary.as_ref() {
        let primary_width = candidate.primary.chars().count();
        if content_width > primary_width + 2 {
            let secondary_width = content_width.saturating_sub(primary_width + 2);
            let secondary_text = truncate_chars(secondary, secondary_width);
            if !secondary_text.is_empty() {
                spans.push(Span::raw("  "));
                spans.push(Span::styled(
                    secondary_text,
                    Style::default().add_modifier(Modifier::DIM),
                ));
            }
        }
    }

    Line::from(spans)
}

fn append_primary_spans(spans: &mut Vec<Span<'static>>, text: &str, match_indices: &[usize]) {
    let mut match_cursor = 0usize;
    for (index, ch) in text.chars().enumerate() {
        let is_match = match_cursor < match_indices.len() && match_indices[match_cursor] == index;
        let style = if is_match {
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        spans.push(Span::styled(ch.to_string(), style));
        if is_match {
            match_cursor += 1;
        }
    }
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}
