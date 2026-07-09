use std::collections::{HashMap, HashSet};

use crate::app::App;
use crate::event::Mode;
use crate::history::HistoryEntry;
use crate::tmux;
use crate::tree::NodeId;

pub struct DeadSession {
    pub name: String,
    pub display_name: String,
    pub cwd: String,
    pub last_seen: u64,
}

pub fn compute_dead_sessions(
    history: &[HistoryEntry],
    live_sessions: &[tmux::Session],
    formatter_cache: &HashMap<String, String>,
) -> Vec<DeadSession> {
    let live_names: HashSet<&str> = live_sessions.iter().map(|s| s.name.as_str()).collect();
    history
        .iter()
        .filter(|e| !live_names.contains(e.name.as_str()))
        .map(|e| {
            let display_name = formatter_cache
                .get(&e.name)
                .cloned()
                .unwrap_or_else(|| e.name.clone());
            DeadSession {
                name: e.name.clone(),
                display_name,
                cwd: e.cwd.clone(),
                last_seen: e.last_seen,
            }
        })
        .collect()
}

impl App {
    pub fn uncached_dead_session_names(&self) -> Vec<String> {
        self.dead_sessions
            .iter()
            .filter(|d| !self.formatter_cache.contains_key(&d.name))
            .map(|d| d.name.clone())
            .collect()
    }

    pub fn apply_name_formatted(&mut self, raw_name: String, formatted: String) {
        self.formatter_cache
            .insert(raw_name.clone(), formatted.clone());
        for session in self.sessions.iter_mut() {
            if session.name == raw_name {
                session.display_name = formatted.clone();
            }
        }
        for dead in self.dead_sessions.iter_mut() {
            if dead.name == raw_name {
                dead.display_name = formatted.clone();
            }
        }
        if self.mode == Mode::CreateSession {
            let selected = self.create_selected;
            self.rebuild_create_candidates();
            self.create_selected = selected.min(self.create_candidates.len().saturating_sub(1));
        }
        let group_sep = self
            .config
            .as_ref()
            .and_then(|c| c.group_name_separator.as_deref());
        let recent_age = crate::app::recent_max_age_secs(self.config.as_ref());
        for prefix in crate::app::extract_group_prefixes(&self.sessions, group_sep) {
            if !self.seen_groups.contains(&prefix) {
                self.opened.insert(NodeId::Group(prefix.clone()));
                if recent_age.is_some() {
                    self.opened
                        .insert(NodeId::Recent(Box::new(NodeId::Group(prefix.clone()))));
                }
                self.seen_groups.insert(prefix);
            }
        }
        self.rebuild_flat_entries();
    }
}
