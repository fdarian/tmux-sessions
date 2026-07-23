use std::collections::HashSet;
use std::io;
use std::time::{SystemTime, UNIX_EPOCH};

use fuzzy_matcher::skim::SkimMatcherV2;

use crate::config;
use crate::tmux;
use crate::tree::{self, FlatEntry, NodeId};

pub fn current_unix_secs() -> io::Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("system clock error: {}", e)))
}

pub fn path_buf_to_string(path: std::path::PathBuf, label: &str) -> io::Result<String> {
    path.into_os_string().into_string().map_err(|path| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{label} is not valid UTF-8: {path:?}"),
        )
    })
}

/// Resolves a jump-key label index (as rendered in ui.rs, which skips
/// `NodeId::Separator(_)`, `NodeId::Header(_)`, and `NodeId::Group(_)` rows) to the corresponding
/// index into `flat_entries`.
pub fn resolve_shortcut_index(flat_entries: &[FlatEntry], shortcut_index: usize) -> Option<usize> {
    flat_entries
        .iter()
        .enumerate()
        .filter(|(_, e)| {
            !matches!(e.node_id, NodeId::Separator(_) | NodeId::Header(_))
                && !matches!(e.node_id.target(), NodeId::Group(_))
        })
        .nth(shortcut_index)
        .map(|(idx, _)| idx)
}

pub fn create_match_result(
    matcher: &SkimMatcherV2,
    query: &str,
    text: &str,
) -> Option<(i64, Vec<usize>)> {
    tree::fuzzy_match_multi(matcher, query, text)
}

pub fn extract_group_prefixes(sessions: &[tmux::Session], separator: Option<&str>) -> Vec<String> {
    let sep = match separator {
        Some(s) => s,
        None => return Vec::new(),
    };
    let mut seen = HashSet::new();
    let mut prefixes = Vec::new();
    for session in sessions.iter() {
        if let Some((prefix, suffix)) = session.display_name.split_once(sep)
            && !prefix.is_empty()
            && !suffix.is_empty()
            && seen.insert(prefix.to_string())
        {
            prefixes.push(prefix.to_string());
        }
    }
    prefixes
}

pub fn recent_max_age_secs(config: Option<&config::Config>) -> Option<u64> {
    let recents = config.and_then(|cfg| cfg.recents.as_ref())?;
    if recents.enabled == Some(true) {
        return Some(recents.max_age_secs());
    }
    None
}

pub fn extract_session_id(node_id: &NodeId) -> Option<&String> {
    match node_id.target() {
        NodeId::Session(id) => Some(id),
        NodeId::Window(session_id, _) => Some(session_id),
        NodeId::Pane(session_id, _, _) => Some(session_id),
        _ => None,
    }
}

pub fn is_non_selectable(node_id: &NodeId) -> bool {
    matches!(node_id, NodeId::Separator(_) | NodeId::Header(_))
}

pub fn first_selectable_index(flat_entries: &[FlatEntry]) -> Option<usize> {
    flat_entries.iter().position(|e| !is_non_selectable(&e.node_id))
}

#[cfg(test)]
mod shortcut_index_tests {
    use super::resolve_shortcut_index;
    use crate::tree::{FlatEntry, NodeId};

    fn entry(node_id: NodeId) -> FlatEntry {
        FlatEntry {
            node_id,
            depth: 0,
            has_children: false,
            is_last_sibling: false,
            ancestor_is_last: Vec::new(),
            text: String::new(),
        }
    }

    #[test]
    fn skips_separator_when_resolving_label_index() {
        let flat_entries = vec![
            entry(NodeId::Session("session-a".to_string())),
            entry(NodeId::Separator(0)),
            entry(NodeId::Session("session-b".to_string())),
        ];

        assert_eq!(resolve_shortcut_index(&flat_entries, 0), Some(0));
        assert_eq!(resolve_shortcut_index(&flat_entries, 1), Some(2));
    }

    #[test]
    fn skips_group_rows_when_resolving_label_index() {
        let flat_entries = vec![
            entry(NodeId::Group("work".to_string())),
            entry(NodeId::Session("session-a".to_string())),
            entry(NodeId::Session("session-b".to_string())),
        ];

        assert_eq!(resolve_shortcut_index(&flat_entries, 0), Some(1));
        assert_eq!(resolve_shortcut_index(&flat_entries, 1), Some(2));
    }

    #[test]
    fn out_of_range_label_resolves_to_none() {
        let flat_entries = vec![entry(NodeId::Session("session-a".to_string()))];
        assert_eq!(resolve_shortcut_index(&flat_entries, 5), None);
    }
}
