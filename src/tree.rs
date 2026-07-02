use std::collections::{HashMap, HashSet};

use fuzzy_matcher::FuzzyMatcher;
use ratatui::style::{Color, Style};
use ratatui::text::Line;

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

fn flatten_session_list(
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

fn flatten_grouped(
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

    for group_sessions in group_map.values_mut() {
        group_sessions.sort_by(|a, b| b.activity.cmp(&a.activity));
    }
    ungrouped.sort_by(|a, b| b.activity.cmp(&a.activity));

    let mut top_level_groups: Vec<(String, u64)> = group_map
        .iter()
        .map(|entry| {
            let max_activity = entry
                .1
                .iter()
                .map(|session| session.activity)
                .max()
                .unwrap_or(0);
            (entry.0.clone(), max_activity)
        })
        .collect();
    top_level_groups.sort_by(|a, b| b.1.cmp(&a.1));

    let top_level_ungrouped: Vec<&tmux::Session> = ungrouped;

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
            let group_node_id = NodeId::Recent(Box::new(NodeId::Group(prefix.clone())));
            entries.push(FlatEntry {
                node_id: group_node_id.clone(),
                depth: 0,
                has_children: true,
                is_last_sibling: false,
                ancestor_is_last: vec![],
                text: format!("{} ({})", prefix, group_sessions.len()),
            });
            if opened.contains(&group_node_id) {
                flatten_group_sessions(
                    group_sessions,
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

        flatten_session_list(
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

fn flatten_recents(
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

pub struct DeadSessionRef<'a> {
    pub name: &'a str,
    pub display_name: &'a str,
    pub last_seen: u64,
}

pub fn fuzzy_match_multi(
    matcher: &fuzzy_matcher::skim::SkimMatcherV2,
    query: &str,
    text: &str,
) -> Option<(i64, Vec<usize>)> {
    let terms: Vec<&str> = query.split_whitespace().collect();
    if terms.is_empty() {
        return Some((0, Vec::new()));
    }

    let mut total_score = 0i64;
    let mut match_indices: Vec<usize> = Vec::new();

    for term in terms {
        let (score, indices) = matcher.fuzzy_indices(text, term)?;
        total_score += score;
        match_indices.extend(indices);
    }

    match_indices.sort_unstable();
    match_indices.dedup();

    Some((total_score, match_indices))
}

pub fn match_live_sessions<'a>(
    sessions: &'a [tmux::Session],
    query: &str,
) -> Vec<&'a tmux::Session> {
    let matcher = fuzzy_matcher::skim::SkimMatcherV2::default();
    let mut matched_sessions = Vec::new();

    for session in sessions.iter() {
        let text = session_text(session);
        if fuzzy_match_multi(&matcher, query, &text).is_some() {
            matched_sessions.push(session);
        }
    }

    matched_sessions
}

pub fn match_dead_sessions(dead_sessions: &[DeadSessionRef<'_>], query: &str) -> Vec<FlatEntry> {
    let matcher = fuzzy_matcher::skim::SkimMatcherV2::default();
    let mut dead_scored: Vec<(i64, u64, FlatEntry)> = Vec::new();
    for dead in dead_sessions.iter() {
        let text = format!("{}: (dead)", dead.display_name);
        if let Some((score, _)) = fuzzy_match_multi(&matcher, query, &text) {
            dead_scored.push((
                score,
                dead.last_seen,
                FlatEntry {
                    node_id: NodeId::DeadSession(dead.name.to_string()),
                    depth: 0,
                    has_children: false,
                    is_last_sibling: false,
                    ancestor_is_last: vec![],
                    text,
                },
            ));
        }
    }
    dead_scored.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));

    dead_scored.into_iter().map(|(_, _, entry)| entry).collect()
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
