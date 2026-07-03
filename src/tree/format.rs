use ratatui::style::{Color, Style};
use ratatui::text::Line;

use crate::tree::FlatEntry;
use crate::tree::NodeId;

pub fn shortcut_label(index: usize) -> Option<String> {
    match index {
        0..=9 => Some(index.to_string()),
        10..=35 => Some(format!("M-{}", (b'a' + (index - 10) as u8) as char)),
        _ => None,
    }
}

pub fn format_line(
    entry: &FlatEntry,
    line_index: usize,
    is_expanded: bool,
    key_width: usize,
) -> Line<'static> {
    if matches!(entry.node_id, NodeId::Separator(_)) {
        let prefix = " ".repeat(key_width + 1);
        return Line::styled(
            format!("{}─────────────────────────────────────", prefix),
            Style::default().fg(Color::DarkGray),
        );
    }

    if matches!(entry.node_id, NodeId::Header(_)) {
        let prefix = " ".repeat(key_width + 1);
        return Line::styled(
            format!("{}{}", prefix, entry.text),
            Style::default().fg(Color::DarkGray),
        );
    }

    let key_str = match shortcut_label(line_index) {
        Some(label) => format!("({})", label),
        None => " ".repeat(key_width),
    };
    let mut result = format!("{:<width$} ", key_str, width = key_width);

    if entry.depth > 0 {
        for d in 0..(entry.depth - 1) {
            if entry.ancestor_is_last[d as usize] {
                result.push_str("    ");
            } else {
                result.push_str("\u{2502}   ");
            }
        }

        if entry.is_last_sibling {
            result.push_str("\u{2514}\u{2500}> ");
        } else {
            result.push_str("\u{251C}\u{2500}> ");
        }
    }

    if entry.has_children {
        if is_expanded {
            result.push_str("- ");
        } else {
            result.push_str("+ ");
        }
    }

    result.push_str(&entry.text);

    Line::raw(result)
}
