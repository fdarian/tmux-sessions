use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::tmux::Session;

const HISTORY_MAX_AGE_SECS: u64 = 30 * 24 * 60 * 60;

#[derive(Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub name: String,
    pub cwd: String,
    pub last_seen: u64,
}

fn history_path() -> io::Result<String> {
    let home = std::env::var("HOME")
        .map_err(|_| io::Error::new(io::ErrorKind::NotFound, "$HOME not set"))?;
    Ok(format!("{}/.config/tmux-sessions/history.json", home))
}

pub fn load_history() -> Vec<HistoryEntry> {
    let path = match history_path() {
        Ok(p) => p,
        Err(_) => return Vec::new(),
    };
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Vec::new(),
        Err(_) => return Vec::new(),
    };
    let entries: Vec<HistoryEntry> = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            let ts = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let backup = format!("{}.broken.{}", path, ts);
            let _ = std::fs::rename(&path, &backup);
            eprintln!("tmux-sessions: history.json was corrupt ({e}); moved to {backup}");
            return Vec::new();
        }
    };
    // If the clock can't be read, skip age-based pruning rather than dropping
    // everything; the directory-existence check still applies.
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).ok();
    entries
        .into_iter()
        .filter(|entry| {
            let within_age = match now {
                Some(now) => now.saturating_sub(entry.last_seen) <= HISTORY_MAX_AGE_SECS,
                None => true,
            };
            within_age && Path::new(&entry.cwd).is_dir()
        })
        .collect()
}

pub fn save_history(entries: &[HistoryEntry]) {
    let path = match history_path() {
        Ok(p) => p,
        Err(_) => return,
    };
    if let Ok(json) = serde_json::to_string(entries) {
        let _ = std::fs::write(&path, json);
    }
}

pub fn upsert_live_sessions(
    history: &mut Vec<HistoryEntry>,
    sessions: &[Session],
    now: u64,
) {
    for session in sessions.iter() {
        match history.iter_mut().find(|e| e.name == session.name) {
            Some(entry) => {
                entry.cwd = session.cwd.clone();
                entry.last_seen = now;
            }
            None => {
                history.push(HistoryEntry {
                    name: session.name.clone(),
                    cwd: session.cwd.clone(),
                    last_seen: now,
                });
            }
        }
    }
    save_history(history);
}
