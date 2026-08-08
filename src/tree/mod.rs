mod flatten;
mod format;
mod matching;

use std::collections::{HashMap, HashSet};

use crate::tmux;

const RECENTS_HEADER_TEXT: &str = "recents";

fn session_text(session: &tmux::Session) -> String {
    let mut text = format!("{}: {} windows", session.display_name, session.window_count);
    if session.attached {
        text.push_str(" (attached)");
    }
    text
}

fn session_text_with_suffix(session: &tmux::Session, separator: &str) -> String {
    let suffix = session
        .display_name
        .split_once(separator)
        .map_or("@", |(_, suffix)| suffix);
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
    Recent(Box<NodeId>),
    Separator(usize),
    Header(String),
    DeadSession(String),
}

impl NodeId {
    pub fn target(&self) -> &NodeId {
        match self {
            NodeId::Recent(node_id) => node_id.target(),
            _ => self,
        }
    }
}

pub fn is_shortcut_labeled(node_id: &NodeId) -> bool {
    !matches!(node_id, NodeId::Separator(_) | NodeId::Header(_))
        && !matches!(node_id.target(), NodeId::Group(_))
}

pub struct FlatEntry {
    pub node_id: NodeId,
    pub depth: u8,
    pub has_children: bool,
    pub is_last_sibling: bool,
    pub ancestor_is_last: Vec<bool>,
    pub text: String,
}

fn maybe_wrap_recent(node_id: NodeId, wrap_recent: bool) -> NodeId {
    if wrap_recent {
        return NodeId::Recent(Box::new(node_id));
    }
    node_id
}

fn flatten_group_sessions(
    sessions: &[&tmux::Session],
    windows: &[tmux::Window],
    panes: &[tmux::Pane],
    opened: &HashSet<NodeId>,
    entries: &mut Vec<FlatEntry>,
    separator: &str,
    wrap_recent: bool,
) {
    for (si, session) in sessions.iter().enumerate() {
        let session_is_last = si == sessions.len() - 1;
        let has_children = windows.iter().any(|w| w.session_id == session.id);
        let session_node_id = NodeId::Session(session.id.clone());
        let opened_session_node_id = maybe_wrap_recent(session_node_id.clone(), wrap_recent);

        entries.push(FlatEntry {
            node_id: opened_session_node_id.clone(),
            depth: 1,
            has_children,
            is_last_sibling: session_is_last,
            ancestor_is_last: vec![],
            text: session_text_with_suffix(session, separator),
        });

        if !opened.contains(&opened_session_node_id) {
            continue;
        }

        let session_windows: Vec<&tmux::Window> = windows
            .iter()
            .filter(|w| w.session_id == session.id)
            .collect();

        for (wi, window) in session_windows.iter().enumerate() {
            let window_is_last = wi == session_windows.len() - 1;
            let has_children = panes
                .iter()
                .any(|p| p.session_id == session.id && p.window_id == window.id);
            let window_node_id = NodeId::Window(session.id.clone(), window.id.clone());
            let opened_window_node_id = maybe_wrap_recent(window_node_id.clone(), wrap_recent);

            entries.push(FlatEntry {
                node_id: opened_window_node_id.clone(),
                depth: 2,
                has_children,
                is_last_sibling: window_is_last,
                ancestor_is_last: vec![session_is_last],
                text: window_text(window),
            });

            if !opened.contains(&opened_window_node_id) {
                continue;
            }

            let window_panes: Vec<&tmux::Pane> = panes
                .iter()
                .filter(|p| p.session_id == session.id && p.window_id == window.id)
                .collect();

            for (pi, pane) in window_panes.iter().enumerate() {
                let pane_is_last = pi == window_panes.len() - 1;
                let pane_node_id =
                    NodeId::Pane(session.id.clone(), window.id.clone(), pane.id.clone());
                entries.push(FlatEntry {
                    node_id: maybe_wrap_recent(pane_node_id, wrap_recent),
                    depth: 3,
                    has_children: false,
                    is_last_sibling: pane_is_last,
                    ancestor_is_last: vec![session_is_last, window_is_last],
                    text: pane_text(pane),
                });
            }
        }
    }
}

fn flatten_recents_header(entries: &mut Vec<FlatEntry>) {
    entries.push(FlatEntry {
        node_id: NodeId::Header(RECENTS_HEADER_TEXT.to_string()),
        depth: 0,
        has_children: false,
        is_last_sibling: false,
        ancestor_is_last: vec![],
        text: RECENTS_HEADER_TEXT.to_string(),
    });
}

fn flatten_recents_grouped(
    sessions: &[&tmux::Session],
    windows: &[tmux::Window],
    panes: &[tmux::Pane],
    opened: &HashSet<NodeId>,
    entries: &mut Vec<FlatEntry>,
    separator: &str,
) {
    let mut group_map: HashMap<String, Vec<&tmux::Session>> = HashMap::new();
    let mut ungrouped: Vec<&tmux::Session> = Vec::new();

    for session in sessions.iter() {
        if let Some((prefix, suffix)) = session.display_name.split_once(separator) {
            if !prefix.is_empty() && !suffix.is_empty() {
                group_map
                    .entry(prefix.to_string())
                    .or_default()
                    .push(*session);
                continue;
            }
        }
        ungrouped.push(*session);
    }

    let mut peer_session: HashMap<String, &tmux::Session> = HashMap::new();
    let mut top_level_ungrouped: Vec<&tmux::Session> = ungrouped
        .into_iter()
        .filter(|session| {
            if group_map.contains_key(&session.display_name) {
                peer_session.insert(session.display_name.clone(), *session);
                false
            } else {
                true
            }
        })
        .collect();

    for group_sessions in group_map.values_mut() {
        group_sessions.sort_by(|a, b| b.activity.cmp(&a.activity));
    }
    top_level_ungrouped.sort_by(|a, b| b.activity.cmp(&a.activity));

    let mut top_level_groups: Vec<(String, u64)> = group_map
        .iter()
        .map(|entry| {
            let max_activity = entry
                .1
                .iter()
                .map(|session| session.activity)
                .chain(peer_session.get(entry.0).map(|peer| peer.activity))
                .max()
                .unwrap_or(0);
            (entry.0.clone(), max_activity)
        })
        .collect();
    top_level_groups.sort_by(|a, b| b.1.cmp(&a.1));

    let mut group_index = 0usize;
    let mut ungrouped_index = 0usize;
    while group_index < top_level_groups.len() || ungrouped_index < top_level_ungrouped.len() {
        let next_group = top_level_groups.get(group_index);
        let next_session = top_level_ungrouped.get(ungrouped_index);
        let use_group = match (next_group, next_session) {
            (Some(group), Some(session)) => group.1 >= session.activity,
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (None, None) => break,
        };

        if use_group {
            let prefix = &top_level_groups[group_index].0;
            let group_sessions = group_map.get(prefix).expect("recent group must exist");
            let peer = peer_session.get(prefix).copied();
            let count = group_sessions.len() + peer.is_some() as usize;
            let group_node_id = NodeId::Recent(Box::new(NodeId::Group(prefix.clone())));
            entries.push(FlatEntry {
                node_id: group_node_id.clone(),
                depth: 0,
                has_children: true,
                is_last_sibling: false,
                ancestor_is_last: vec![],
                text: format!("{} ({})", prefix, count),
            });
            if opened.contains(&group_node_id) {
                let members: Vec<&tmux::Session> = peer
                    .into_iter()
                    .chain(group_sessions.iter().copied())
                    .collect();
                flatten_group_sessions(
                    &members,
                    windows,
                    panes,
                    opened,
                    entries,
                    separator,
                    true,
                );
            }
            group_index += 1;
            continue;
        }

        flatten::flatten_session_list(
            &[top_level_ungrouped[ungrouped_index]],
            windows,
            panes,
            opened,
            entries,
            true,
            0,
            &[],
        );
        ungrouped_index += 1;
    }
}

pub struct DeadSessionRef<'a> {
    pub name: &'a str,
    pub display_name: &'a str,
    pub last_seen: u64,
}

pub use flatten::flatten;
pub use format::{connector_prefix, format_line};
pub use matching::{fuzzy_match_multi, match_dead_sessions, match_live_sessions};
