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

/// Builds the indent + branch-connector prefix for a tree row at `depth`,
/// using `ancestor_is_last` to decide between a blank gap and a vertical
/// bar at each ancestor level, and `is_last_sibling` to pick the row's own
/// connector. This is the tree-rendering convention shared by the session
/// tree and the process monitor's tree — the session tree owns it, other
/// views borrow it via `crate::tree::connector_prefix`.
pub fn connector_prefix(depth: u8, ancestor_is_last: &[bool], is_last_sibling: bool) -> String {
    if depth == 0 {
        return String::new();
    }

    let mut prefix = String::new();
    for d in 0..(depth - 1) {
        if ancestor_is_last[d as usize] {
            prefix.push_str("    ");
        } else {
            prefix.push_str("\u{2502}   ");
        }
    }

    if is_last_sibling {
        prefix.push_str("\u{2514}\u{2500}> ");
    } else {
        prefix.push_str("\u{251C}\u{2500}> ");
    }
    prefix
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

    result.push_str(&connector_prefix(
        entry.depth,
        &entry.ancestor_is_last,
        entry.is_last_sibling,
    ));

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
