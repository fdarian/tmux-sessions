use std::collections::{HashMap, HashSet};

use fuzzy_matcher::FuzzyMatcher;
use ratatui::style::{Color, Style};
use ratatui::text::Line;

use crate::tmux;

fn session_text(session: &tmux::Session) -> String {
    let mut text = format!("{}: {} windows", session.display_name, session.window_count);
    if session.attached {
        text.push_str(" (attached)");
    }
    text
}

fn session_text_with_suffix(session: &tmux::Session, separator: &str) -> String {
    let suffix = session.display_name
        .split_once(separator)
        .expect("caller must guarantee separator is present in display_name")
        .1;
    let mut text = format!("{}: {} windows", suffix, session.window_count);
    if session.attached {
        text.push_str(" (attached)");
    }
    text
}

fn window_text(window: &tmux::Window) -> String {
    format!(
        "{}: {}{}: \"{}\"",
        window.index, window.name, window.flags, window.pane_title
    )
}

fn pane_text(pane: &tmux::Pane) -> String {
    if pane.active {
        format!(
            "{}: {}*: \"{}\"",
            pane.index, pane.current_command, pane.title
        )
    } else {
        format!(
            "{}: {}: \"{}\"",
            pane.index, pane.current_command, pane.title
        )
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum NodeId {
    Group(String),
    Session(String),
    Window(String, String),
    Pane(String, String, String),
    Separator,
}

pub struct FlatEntry {
    pub node_id: NodeId,
    pub depth: u8,
    pub has_children: bool,
    pub is_last_sibling: bool,
    pub ancestor_is_last: Vec<bool>,
    pub text: String,
    pub bound_session_id: Option<String>,
}

fn flatten_session_list(
    sessions: &[&tmux::Session],
    windows: &[tmux::Window],
    panes: &[tmux::Pane],
    opened: &HashSet<NodeId>,
    entries: &mut Vec<FlatEntry>,
) {
    for (si, session) in sessions.iter().enumerate() {
        let session_is_last_sibling = si == sessions.len() - 1;
        let has_children = windows.iter().any(|w| w.session_id == session.id);

        entries.push(FlatEntry {
            node_id: NodeId::Session(session.id.clone()),
            depth: 0,
            has_children,
            is_last_sibling: session_is_last_sibling,
            ancestor_is_last: vec![],
            text: session_text(session),
            bound_session_id: None,
        });

        if !opened.contains(&NodeId::Session(session.id.clone())) {
            continue;
        }

        let session_windows: Vec<&tmux::Window> =
            windows.iter().filter(|w| w.session_id == session.id).collect();

        for (wi, window) in session_windows.iter().enumerate() {
            let window_is_last_sibling = wi == session_windows.len() - 1;
            let has_children = panes
                .iter()
                .any(|p| p.session_id == session.id && p.window_id == window.id);

            entries.push(FlatEntry {
                node_id: NodeId::Window(session.id.clone(), window.id.clone()),
                depth: 1,
                has_children,
                is_last_sibling: window_is_last_sibling,
                ancestor_is_last: vec![],
                text: window_text(window),
                bound_session_id: None,
            });

            if !opened.contains(&NodeId::Window(session.id.clone(), window.id.clone())) {
                continue;
            }

            let window_panes: Vec<&tmux::Pane> = panes
                .iter()
                .filter(|p| p.session_id == session.id && p.window_id == window.id)
                .collect();

            for (pi, pane) in window_panes.iter().enumerate() {
                let pane_is_last_sibling = pi == window_panes.len() - 1;

                entries.push(FlatEntry {
                    node_id: NodeId::Pane(
                        session.id.clone(),
                        window.id.clone(),
                        pane.id.clone(),
                    ),
                    depth: 2,
                    has_children: false,
                    is_last_sibling: pane_is_last_sibling,
                    ancestor_is_last: vec![window_is_last_sibling],
                    text: pane_text(pane),
                    bound_session_id: None,
                });
            }
        }
    }
}

fn flatten_group_sessions(
    sessions: &[&tmux::Session],
    windows: &[tmux::Window],
    panes: &[tmux::Pane],
    opened: &HashSet<NodeId>,
    entries: &mut Vec<FlatEntry>,
    separator: &str,
) {
    for (si, session) in sessions.iter().enumerate() {
        let session_is_last = si == sessions.len() - 1;
        let has_children = windows.iter().any(|w| w.session_id == session.id);

        entries.push(FlatEntry {
            node_id: NodeId::Session(session.id.clone()),
            depth: 1,
            has_children,
            is_last_sibling: session_is_last,
            ancestor_is_last: vec![],
            text: session_text_with_suffix(session, separator),
            bound_session_id: None,
        });

        if !opened.contains(&NodeId::Session(session.id.clone())) {
            continue;
        }

        let session_windows: Vec<&tmux::Window> =
            windows.iter().filter(|w| w.session_id == session.id).collect();

        for (wi, window) in session_windows.iter().enumerate() {
            let window_is_last = wi == session_windows.len() - 1;
            let has_children = panes
                .iter()
                .any(|p| p.session_id == session.id && p.window_id == window.id);

            entries.push(FlatEntry {
                node_id: NodeId::Window(session.id.clone(), window.id.clone()),
                depth: 2,
                has_children,
                is_last_sibling: window_is_last,
                ancestor_is_last: vec![session_is_last],
                text: window_text(window),
                bound_session_id: None,
            });

            if !opened.contains(&NodeId::Window(session.id.clone(), window.id.clone())) {
                continue;
            }

            let window_panes: Vec<&tmux::Pane> = panes
                .iter()
                .filter(|p| p.session_id == session.id && p.window_id == window.id)
                .collect();

            for (pi, pane) in window_panes.iter().enumerate() {
                let pane_is_last = pi == window_panes.len() - 1;
                entries.push(FlatEntry {
                    node_id: NodeId::Pane(
                        session.id.clone(),
                        window.id.clone(),
                        pane.id.clone(),
                    ),
                    depth: 3,
                    has_children: false,
                    is_last_sibling: pane_is_last,
                    ancestor_is_last: vec![session_is_last, window_is_last],
                    text: pane_text(pane),
                    bound_session_id: None,
                });
            }
        }
    }
}

fn flatten_grouped(
    sessions: &[&tmux::Session],
    windows: &[tmux::Window],
    panes: &[tmux::Pane],
    opened: &HashSet<NodeId>,
    entries: &mut Vec<FlatEntry>,
    separator: &str,
) {
    let mut group_order: Vec<String> = Vec::new();
    let mut group_map: HashMap<String, Vec<&tmux::Session>> = HashMap::new();
    let mut ungrouped: Vec<&tmux::Session> = Vec::new();

    for session in sessions.iter() {
        let mut parts = session.display_name.splitn(2, separator);
        let prefix = parts.next().unwrap_or("");
        let suffix = parts.next().unwrap_or("");
        if !prefix.is_empty() && !suffix.is_empty() {
            if !group_map.contains_key(prefix) {
                group_order.push(prefix.to_string());
                group_map.insert(prefix.to_string(), Vec::new());
            }
            group_map.get_mut(prefix).unwrap().push(*session);
        } else {
            ungrouped.push(*session);
        }
    }

    // Sessions whose display_name exactly matches a group prefix are absorbed into the group row.
    let mut group_bound_session: HashMap<String, &tmux::Session> = HashMap::new();
    let truly_ungrouped: Vec<&tmux::Session> = ungrouped.into_iter().filter(|session| {
        if group_order.contains(&session.display_name) {
            group_bound_session.insert(session.display_name.clone(), *session);
            false
        } else {
            true
        }
    }).collect();

    for prefix in &group_order {
        let group_sessions = group_map.get(prefix).unwrap();
        let count = group_sessions.len();
        let is_expanded = opened.contains(&NodeId::Group(prefix.clone()));
        let bound_session = group_bound_session.get(prefix).copied();

        let text = if let Some(s) = bound_session {
            let mut t = format!("{} ({})", prefix, count);
            t.push_str(&format!(": {} windows", s.window_count));
            if s.attached {
                t.push_str(" (attached)");
            }
            t
        } else {
            format!("{} ({})", prefix, count)
        };

        entries.push(FlatEntry {
            node_id: NodeId::Group(prefix.clone()),
            depth: 0,
            has_children: true,
            is_last_sibling: false,
            ancestor_is_last: vec![],
            text,
            bound_session_id: bound_session.map(|s| s.id.clone()),
        });

        if is_expanded {
            flatten_group_sessions(group_sessions, windows, panes, opened, entries, separator);
        }
    }

    flatten_session_list(&truly_ungrouped, windows, panes, opened, entries);
}

pub fn flatten(
    sessions: &[tmux::Session],
    windows: &[tmux::Window],
    panes: &[tmux::Pane],
    opened: &HashSet<NodeId>,
    pinned: &HashSet<String>,
    group_separator: Option<&str>,
) -> Vec<FlatEntry> {
    let mut entries = Vec::new();

    let pinned_sessions: Vec<&tmux::Session> =
        sessions.iter().filter(|s| pinned.contains(&s.name)).collect();
    let unpinned_sessions: Vec<&tmux::Session> =
        sessions.iter().filter(|s| !pinned.contains(&s.name)).collect();

    // Pinned always render flat at the top, regardless of grouping.
    flatten_session_list(&pinned_sessions, windows, panes, opened, &mut entries);

    if !pinned_sessions.is_empty() && !unpinned_sessions.is_empty() {
        entries.push(FlatEntry {
            node_id: NodeId::Separator,
            depth: 0,
            has_children: false,
            is_last_sibling: false,
            ancestor_is_last: vec![],
            text: String::new(),
            bound_session_id: None,
        });
    }

    match group_separator {
        Some(sep) => flatten_grouped(
            &unpinned_sessions, windows, panes, opened, &mut entries, sep,
        ),
        None => flatten_session_list(
            &unpinned_sessions, windows, panes, opened, &mut entries,
        ),
    }

    entries
}

pub fn flatten_filtered(
    sessions: &[tmux::Session],
    windows: &[tmux::Window],
    query: &str,
) -> Vec<FlatEntry> {
    let matcher = fuzzy_matcher::skim::SkimMatcherV2::default();
    let mut scored: Vec<(i64, FlatEntry)> = Vec::new();

    for session in sessions.iter() {
        let text = session_text(session);
        if let Some(score) = matcher.fuzzy_match(&text, query) {
            let has_children = windows.iter().any(|w| w.session_id == session.id);
            scored.push((score, FlatEntry {
                node_id: NodeId::Session(session.id.clone()),
                depth: 0,
                has_children,
                is_last_sibling: false,
                ancestor_is_last: vec![],
                text,
                bound_session_id: None,
            }));
        }
    }

    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.into_iter().map(|(_, entry)| entry).collect()
}

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
    if entry.node_id == NodeId::Separator {
        let prefix = " ".repeat(key_width + 1);
        return Line::styled(
            format!("{}─────────────────────────────────────", prefix),
            Style::default().fg(Color::DarkGray),
        );
    }

    let key_str = match shortcut_label(line_index) {
        Some(label) => format!("({})", label),
        None => " ".repeat(key_width),
    };
    let mut result = format!("{:>width$} ", key_str, width = key_width);

    if entry.depth > 0 {
        // Ancestor columns: one 4-char column per ancestor level
        for d in 0..(entry.depth - 1) {
            if entry.ancestor_is_last[d as usize] {
                result.push_str("    ");
            } else {
                result.push_str("\u{2502}   ");
            }
        }

        // Immediate connector for this node
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
