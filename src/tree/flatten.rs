use std::collections::{HashMap, HashSet};

use crate::tmux;
use crate::tree::flatten_group_sessions;
use crate::tree::flatten_recents_grouped;
use crate::tree::flatten_recents_header;
use crate::tree::maybe_wrap_recent;
use crate::tree::pane_text;
use crate::tree::session_text;
use crate::tree::window_text;
use crate::tree::FlatEntry;
use crate::tree::NodeId;

pub fn flatten_session_list(
    sessions: &[&tmux::Session],
    windows: &[tmux::Window],
    panes: &[tmux::Pane],
    opened: &HashSet<NodeId>,
    entries: &mut Vec<FlatEntry>,
    wrap_recent: bool,
    depth_offset: u8,
    ancestor_prefix: &[bool],
) {
    for (si, session) in sessions.iter().enumerate() {
        let session_is_last_sibling = si == sessions.len() - 1;
        let has_children = windows.iter().any(|w| w.session_id == session.id);
        let session_node_id = NodeId::Session(session.id.clone());
        let opened_session_node_id = maybe_wrap_recent(session_node_id.clone(), wrap_recent);

        entries.push(FlatEntry {
            node_id: opened_session_node_id.clone(),
            depth: depth_offset,
            has_children,
            is_last_sibling: session_is_last_sibling,
            ancestor_is_last: ancestor_prefix.to_vec(),
            text: session_text(session),
        });

        if !opened.contains(&opened_session_node_id) {
            continue;
        }

        let session_windows: Vec<&tmux::Window> = windows
            .iter()
            .filter(|w| w.session_id == session.id)
            .collect();

        for (wi, window) in session_windows.iter().enumerate() {
            let window_is_last_sibling = wi == session_windows.len() - 1;
            let has_children = panes
                .iter()
                .any(|p| p.session_id == session.id && p.window_id == window.id);
            let window_node_id = NodeId::Window(session.id.clone(), window.id.clone());
            let opened_window_node_id = maybe_wrap_recent(window_node_id.clone(), wrap_recent);
            let mut ancestor_is_last = ancestor_prefix.to_vec();
            if depth_offset > 0 {
                ancestor_is_last.push(session_is_last_sibling);
            }

            entries.push(FlatEntry {
                node_id: opened_window_node_id.clone(),
                depth: depth_offset + 1,
                has_children,
                is_last_sibling: window_is_last_sibling,
                ancestor_is_last,
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
                let pane_is_last_sibling = pi == window_panes.len() - 1;
                let pane_node_id =
                    NodeId::Pane(session.id.clone(), window.id.clone(), pane.id.clone());
                let mut ancestor_is_last = ancestor_prefix.to_vec();
                if depth_offset > 0 {
                    ancestor_is_last.push(session_is_last_sibling);
                }
                ancestor_is_last.push(window_is_last_sibling);

                entries.push(FlatEntry {
                    node_id: maybe_wrap_recent(pane_node_id, wrap_recent),
                    depth: depth_offset + 2,
                    has_children: false,
                    is_last_sibling: pane_is_last_sibling,
                    ancestor_is_last,
                    text: pane_text(pane),
                });
            }
        }
    }
}

pub fn flatten_grouped(
    sessions: &[&tmux::Session],
    windows: &[tmux::Window],
    panes: &[tmux::Pane],
    opened: &HashSet<NodeId>,
    entries: &mut Vec<FlatEntry>,
    separator: &str,
    wrap_recent: bool,
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
            group_map
                .get_mut(prefix)
                .expect("group must exist")
                .push(*session);
        } else {
            ungrouped.push(*session);
        }
    }

    let mut peer_session: HashMap<String, &tmux::Session> = HashMap::new();
    let truly_ungrouped: Vec<&tmux::Session> = ungrouped
        .into_iter()
        .filter(|session| {
            if group_order.contains(&session.display_name) {
                peer_session.insert(session.display_name.clone(), *session);
                false
            } else {
                true
            }
        })
        .collect();

    for prefix in &group_order {
        let group_sessions = group_map.get(prefix).expect("group must exist");
        let count = group_sessions.len();
        let group_node_id = maybe_wrap_recent(NodeId::Group(prefix.clone()), wrap_recent);
        let is_expanded = opened.contains(&group_node_id);

        if let Some(peer) = peer_session.get(prefix).copied() {
            let has_children = windows.iter().any(|w| w.session_id == peer.id);
            let session_node_id = NodeId::Session(peer.id.clone());
            let opened_session_node_id = maybe_wrap_recent(session_node_id.clone(), wrap_recent);
            entries.push(FlatEntry {
                node_id: opened_session_node_id.clone(),
                depth: 0,
                has_children,
                is_last_sibling: false,
                ancestor_is_last: vec![],
                text: session_text(peer),
            });

            if opened.contains(&opened_session_node_id) {
                let peer_windows: Vec<&tmux::Window> =
                    windows.iter().filter(|w| w.session_id == peer.id).collect();
                for (wi, window) in peer_windows.iter().enumerate() {
                    let window_is_last = wi == peer_windows.len() - 1;
                    let has_win_children = panes
                        .iter()
                        .any(|p| p.session_id == peer.id && p.window_id == window.id);
                    let window_node_id = NodeId::Window(peer.id.clone(), window.id.clone());
                    let opened_window_node_id =
                        maybe_wrap_recent(window_node_id.clone(), wrap_recent);
                    entries.push(FlatEntry {
                        node_id: opened_window_node_id.clone(),
                        depth: 1,
                        has_children: has_win_children,
                        is_last_sibling: window_is_last,
                        ancestor_is_last: vec![],
                        text: window_text(window),
                    });

                    if !opened.contains(&opened_window_node_id) {
                        continue;
                    }

                    let window_panes: Vec<&tmux::Pane> = panes
                        .iter()
                        .filter(|p| p.session_id == peer.id && p.window_id == window.id)
                        .collect();
                    for (pi, pane) in window_panes.iter().enumerate() {
                        let pane_is_last = pi == window_panes.len() - 1;
                        let pane_node_id =
                            NodeId::Pane(peer.id.clone(), window.id.clone(), pane.id.clone());
                        entries.push(FlatEntry {
                            node_id: maybe_wrap_recent(pane_node_id, wrap_recent),
                            depth: 2,
                            has_children: false,
                            is_last_sibling: pane_is_last,
                            ancestor_is_last: vec![window_is_last],
                            text: pane_text(pane),
                        });
                    }
                }
            }
        }

        entries.push(FlatEntry {
            node_id: group_node_id,
            depth: 0,
            has_children: true,
            is_last_sibling: false,
            ancestor_is_last: vec![],
            text: format!("{} ({})", prefix, count),
        });

        if is_expanded {
            flatten_group_sessions(
                group_sessions,
                windows,
                panes,
                opened,
                entries,
                separator,
                wrap_recent,
            );
        }
    }

    flatten_session_list(
        &truly_ungrouped,
        windows,
        panes,
        opened,
        entries,
        wrap_recent,
        0,
        &[],
    );
}

pub fn flatten_recents(
    sessions: &[&tmux::Session],
    windows: &[tmux::Window],
    panes: &[tmux::Pane],
    opened: &HashSet<NodeId>,
    entries: &mut Vec<FlatEntry>,
    group_separator: Option<&str>,
) {
    if sessions.is_empty() {
        return;
    }

    flatten_recents_header(entries);
    match group_separator {
        Some(separator) => {
            flatten_recents_grouped(sessions, windows, panes, opened, entries, separator)
        }
        None => flatten_session_list(sessions, windows, panes, opened, entries, true, 0, &[]),
    }
}

pub fn flatten(
    sessions: &[tmux::Session],
    windows: &[tmux::Window],
    panes: &[tmux::Pane],
    opened: &HashSet<NodeId>,
    pinned: &[String],
    hidden: &[String],
    show_hidden: bool,
    group_separator: Option<&str>,
    recent_max_age_secs: Option<u64>,
) -> Vec<FlatEntry> {
    let mut entries = Vec::new();

    let is_visible = |name: &String| show_hidden || !hidden.contains(name);
    let pinned_sessions: Vec<&tmux::Session> = pinned
        .iter()
        .filter_map(|name| sessions.iter().find(|s| s.name == *name))
        .filter(|s| is_visible(&s.name))
        .collect();
    let unpinned_sessions: Vec<&tmux::Session> = sessions
        .iter()
        .filter(|s| !pinned.contains(&s.name) && is_visible(&s.name))
        .collect();
    let recent_sessions: Vec<&tmux::Session> = match recent_max_age_secs {
        Some(max_age_secs) => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock must be after unix epoch")
                .as_secs();
            let mut eligible: Vec<&tmux::Session> = sessions
                .iter()
                .filter(|session| {
                    !pinned.contains(&session.name)
                        && !hidden.contains(&session.name)
                        && now.saturating_sub(session.activity) <= max_age_secs
                })
                .collect();
            eligible.sort_by(|a, b| b.activity.cmp(&a.activity));
            eligible
        }
        None => Vec::new(),
    };

    flatten_session_list(
        &pinned_sessions,
        windows,
        panes,
        opened,
        &mut entries,
        false,
        0,
        &[],
    );

    let mut next_separator_id = 0usize;
    let mut push_separator = |entries: &mut Vec<FlatEntry>| {
        entries.push(FlatEntry {
            node_id: NodeId::Separator(next_separator_id),
            depth: 0,
            has_children: false,
            is_last_sibling: false,
            ancestor_is_last: vec![],
            text: String::new(),
        });
        next_separator_id += 1;
    };

    if !pinned_sessions.is_empty() && (!recent_sessions.is_empty() || !unpinned_sessions.is_empty())
    {
        push_separator(&mut entries);
    }

    flatten_recents(
        &recent_sessions,
        windows,
        panes,
        opened,
        &mut entries,
        group_separator,
    );

    if !recent_sessions.is_empty() && !unpinned_sessions.is_empty() {
        push_separator(&mut entries);
    }

    match group_separator {
        Some(sep) => flatten_grouped(
            &unpinned_sessions,
            windows,
            panes,
            opened,
            &mut entries,
            sep,
            false,
        ),
        None => flatten_session_list(
            &unpinned_sessions,
            windows,
            panes,
            opened,
            &mut entries,
            false,
            0,
            &[],
        ),
    }

    entries
}
