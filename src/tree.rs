use std::collections::HashSet;

use ratatui::text::Line;

use crate::tmux;

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum NodeId {
    Session(String),
    Window(String, String),
    Pane(String, String, String),
}

pub struct FlatEntry {
    pub node_id: NodeId,
    pub depth: u8,
    pub has_children: bool,
    pub is_last_sibling: bool,
    pub ancestor_is_last: Vec<bool>,
    pub text: String,
}

pub fn flatten(
    sessions: &[tmux::Session],
    windows: &[tmux::Window],
    panes: &[tmux::Pane],
    opened: &HashSet<NodeId>,
) -> Vec<FlatEntry> {
    let mut entries = Vec::new();

    for (si, session) in sessions.iter().enumerate() {
        let session_is_last_sibling = si == sessions.len() - 1;
        let has_children = windows.iter().any(|w| w.session_id == session.id);

        let mut text = format!("{}: {} windows", session.name, session.window_count);
        if session.attached {
            text.push_str(" (attached)");
        }

        entries.push(FlatEntry {
            node_id: NodeId::Session(session.id.clone()),
            depth: 0,
            has_children,
            is_last_sibling: session_is_last_sibling,
            ancestor_is_last: vec![],
            text,
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

            let text = format!(
                "{}: {}{}: \"{}\"",
                window.index, window.name, window.flags, window.pane_title
            );

            entries.push(FlatEntry {
                node_id: NodeId::Window(session.id.clone(), window.id.clone()),
                depth: 1,
                has_children,
                is_last_sibling: window_is_last_sibling,
                ancestor_is_last: vec![session_is_last_sibling],
                text,
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

                let text = if pane.active {
                    format!(
                        "{}: {}*: \"{}\"",
                        pane.index, pane.current_command, pane.title
                    )
                } else {
                    format!(
                        "{}: {}: \"{}\"",
                        pane.index, pane.current_command, pane.title
                    )
                };

                entries.push(FlatEntry {
                    node_id: NodeId::Pane(
                        session.id.clone(),
                        window.id.clone(),
                        pane.id.clone(),
                    ),
                    depth: 2,
                    has_children: false,
                    is_last_sibling: pane_is_last_sibling,
                    ancestor_is_last: vec![session_is_last_sibling, window_is_last_sibling],
                    text,
                });
            }
        }
    }

    entries
}

pub fn format_line(
    entry: &FlatEntry,
    line_index: usize,
    is_expanded: bool,
    _key_width: usize,
) -> Line<'static> {
    let mut result = format!("({}) ", line_index);

    if entry.depth > 0 {
        // Ancestor columns: one 4-char column per ancestor level
        for d in 0..(entry.depth - 1) {
            if entry.ancestor_is_last[d as usize] {
                result.push_str("    ");
            } else {
                result.push_str(" \u{2502}  ");
            }
        }

        // Immediate connector for this node
        if entry.is_last_sibling {
            result.push_str(" \u{2514}> ");
        } else {
            result.push_str(" \u{251C}> ");
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

pub fn resolve_preview_pane_id(
    selected: &NodeId,
    windows: &[tmux::Window],
    panes: &[tmux::Pane],
) -> Option<String> {
    match selected {
        NodeId::Pane(_, _, pane_id) => Some(pane_id.clone()),
        NodeId::Window(session_id, window_id) => panes
            .iter()
            .find(|p| p.session_id == *session_id && p.window_id == *window_id)
            .map(|p| p.id.clone()),
        NodeId::Session(session_id) => {
            let first_window = windows.iter().find(|w| w.session_id == *session_id)?;
            panes
                .iter()
                .find(|p| p.session_id == *session_id && p.window_id == first_window.id)
                .map(|p| p.id.clone())
        }
    }
}
