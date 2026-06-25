use std::collections::{HashMap, HashSet};
use std::io;
use std::time::{SystemTime, UNIX_EPOCH};

use ratatui::style::{Color, Style};
use ratatui::widgets::ListState;

use crate::config;
use crate::event::{Action, Mode};
use crate::history::{self, HistoryEntry};
use crate::procs::{self, ProcessRow};
use crate::tmux;
use crate::tree::{self, DeadSessionRef, FlatEntry, NodeId};

#[derive(Clone, Copy, PartialEq)]
pub enum MonitorSort {
    Mem,
    Cpu,
}

fn pins_path() -> io::Result<String> {
    let home = std::env::var("HOME")
        .map_err(|_| io::Error::new(io::ErrorKind::NotFound, "$HOME not set"))?;
    Ok(format!("{}/.config/tmux-sessions/pins.json", home))
}

fn load_pins() -> Vec<String> {
    let path = match pins_path() {
        Ok(p) => p,
        Err(_) => return Vec::new(),
    };
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    match serde_json::from_str::<Vec<String>>(&content) {
        Ok(v) => v,
        Err(e) => {
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let backup = format!("{}.broken.{}", path, ts);
            let _ = std::fs::rename(&path, &backup);
            eprintln!("tmux-sessions: pins.json was corrupt ({e}); moved to {backup}");
            Vec::new()
        }
    }
}

fn save_pins(pinned: &[String]) {
    let path = match pins_path() {
        Ok(p) => p,
        Err(_) => return,
    };
    if let Ok(json) = serde_json::to_string(pinned) {
        let _ = std::fs::write(&path, json);
    }
}

fn hidden_path() -> io::Result<String> {
    let home = std::env::var("HOME")
        .map_err(|_| io::Error::new(io::ErrorKind::NotFound, "$HOME not set"))?;
    Ok(format!("{}/.config/tmux-sessions/hidden.json", home))
}

fn load_hidden() -> Vec<String> {
    let path = match hidden_path() {
        Ok(p) => p,
        Err(_) => return Vec::new(),
    };
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    match serde_json::from_str::<Vec<String>>(&content) {
        Ok(v) => v,
        Err(e) => {
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let backup = format!("{}.broken.{}", path, ts);
            let _ = std::fs::rename(&path, &backup);
            eprintln!("tmux-sessions: hidden.json was corrupt ({e}); moved to {backup}");
            Vec::new()
        }
    }
}

fn save_hidden(hidden: &[String]) {
    let path = match hidden_path() {
        Ok(p) => p,
        Err(_) => return,
    };
    if let Ok(json) = serde_json::to_string(hidden) {
        let _ = std::fs::write(&path, json);
    }
}

pub struct PreviewPane {
    pub label: String,
    pub content: Vec<u8>,
    pub is_active: bool,
}

pub struct PreviewFullPane {
    pub session_id: String,
    pub window_id: String,
    pub pane_id: String,
    pub session_name: String,
    pub window_label: String,
    pub pane_label: String,
    pub content: Vec<u8>,
}

#[derive(Clone)]
pub enum RenameTarget {
    Session(String), // session id ($N)
    Window(String),  // window id (@N)
}

#[derive(Clone)]
pub enum MoveTarget {
    Existing(String),
    Dead { name: String, cwd: String },
    New { name: String, cwd: String },
}

#[derive(Clone)]
pub struct MoveCandidate {
    pub label: String,
    pub dim: bool,
    pub target: MoveTarget,
}

pub struct DeadSession {
    pub name: String,
    pub display_name: String,
    pub cwd: String,
    pub last_seen: u64,
}

pub struct App {
    pub config: Option<config::Config>,
    pub current_session_id: String,
    pub sessions: Vec<tmux::Session>,
    pub windows: Vec<tmux::Window>,
    pub panes: Vec<tmux::Pane>,
    pub flat_entries: Vec<FlatEntry>,
    pub opened: HashSet<NodeId>,
    pub seen_groups: HashSet<String>,
    pub list_state: ListState,
    pub preview_panes: Vec<PreviewPane>,
    pub preview_title: String,
    pub preview_full_panes: Vec<PreviewFullPane>,
    pub preview_full_index: usize,
    pub mode: Mode,
    pub confirming_node: Option<NodeId>,
    pub should_quit: bool,
    pub highlight_style: Style,
    pub primary_color: Color,
    pub filter_query: String,
    pub filter_cursor: usize,
    pub pinned: Vec<String>,
    pub hidden: Vec<String>,
    pub show_hidden: bool,
    pub renaming_target: Option<RenameTarget>,
    pub rename_buffer: String,
    pub rename_cursor: usize,
    pub marked_windows: Vec<String>,
    pub selecting: bool,
    pub selection_anchor: Option<usize>,
    pub move_query: String,
    pub move_cursor: usize,
    pub move_candidates: Vec<MoveCandidate>,
    pub move_selected: usize,
    pub move_source_session_cwd: String,
    pub dead_sessions: Vec<DeadSession>,
    pub monitor_rows: Vec<ProcessRow>,
    pub monitor_selected: usize,
    pub monitor_sort: MonitorSort,
    pub monitor_list_state: ListState,
    pub confirming_process: Option<(u32, String)>,
}

fn current_unix_secs() -> io::Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("system clock error: {}", e)))
}

fn compute_dead_sessions(
    history: &[HistoryEntry],
    live_sessions: &[tmux::Session],
    config: &Option<config::Config>,
) -> Vec<DeadSession> {
    let live_names: HashSet<&str> = live_sessions.iter().map(|s| s.name.as_str()).collect();
    history
        .iter()
        .filter(|e| !live_names.contains(e.name.as_str()))
        .map(|e| {
            let mut display_name = e.name.clone();
            config::apply_formatter_to_name(&mut display_name, config);
            DeadSession {
                name: e.name.clone(),
                display_name,
                cwd: e.cwd.clone(),
                last_seen: e.last_seen,
            }
        })
        .collect()
}

fn extract_group_prefixes(sessions: &[tmux::Session], separator: Option<&str>) -> Vec<String> {
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

impl App {
    pub fn new() -> io::Result<Self> {
        let config = config::load_config()?;
        let current_session_id = tmux::get_current_session_id()?;
        let mut sessions = tmux::list_sessions(&current_session_id)?;
        config::apply_formatter_to_sessions(&mut sessions, &config);
        sessions.sort_by(|a, b| b.activity.cmp(&a.activity));
        let windows = tmux::list_windows()?;
        let panes = tmux::list_panes()?;

        let mut history_entries = history::load_history();
        let now = current_unix_secs()?;
        history::upsert_live_sessions(&mut history_entries, &sessions, now);
        let dead_sessions = compute_dead_sessions(&history_entries, &sessions, &config);

        let pinned = load_pins();
        let hidden = load_hidden();
        let show_hidden = false;
        let group_sep = config.as_ref().and_then(|c| c.group_name_separator.as_deref());
        let group_prefixes = extract_group_prefixes(&sessions, group_sep);
        let opened: HashSet<NodeId> = group_prefixes.iter()
            .map(|p| NodeId::Group(p.clone()))
            .collect();
        let seen_groups: HashSet<String> = group_prefixes.into_iter().collect();
        let flat_entries = tree::flatten(&sessions, &windows, &panes, &opened, &pinned, &hidden, show_hidden, group_sep);
        let mut list_state = ListState::default();
        let initial_index = flat_entries
            .iter()
            .position(|e| {
                sessions.iter().any(|s| s.attached && {
                    windows.iter().any(|w| {
                        w.session_id == s.id
                            && w.active
                            && e.node_id == NodeId::Window(s.id.clone(), w.id.clone())
                    })
                })
            })
            .or_else(|| {
                flat_entries.iter().position(|e| {
                    sessions.iter().any(|s| s.attached && e.node_id == NodeId::Session(s.id.clone()))
                })
            })
            .or_else(|| if flat_entries.is_empty() { None } else { Some(0) });
        list_state.select(initial_index);

        let mode_style = tmux::get_mode_style()
            .ok()
            .map(|s| tmux::parse_style(&s))
            .unwrap_or_default();
        let primary_color = mode_style.bg.unwrap_or(Color::Yellow);
        let highlight_style = Style::default()
            .bg(primary_color)
            .fg(mode_style.fg.unwrap_or(Color::Black));

        let mut app = App {
            config,
            current_session_id,
            sessions,
            windows,
            panes,
            flat_entries,
            opened,
            seen_groups,
            list_state,
            preview_panes: Vec::new(),
            preview_title: String::new(),
            preview_full_panes: Vec::new(),
            preview_full_index: 0,
            mode: Mode::Normal,
            confirming_node: None,
            should_quit: false,
            highlight_style,
            primary_color,
            filter_query: String::new(),
            filter_cursor: 0,
            pinned,
            hidden,
            show_hidden,
            renaming_target: None,
            rename_buffer: String::new(),
            rename_cursor: 0,
            marked_windows: Vec::new(),
            selecting: false,
            selection_anchor: None,
            move_query: String::new(),
            move_cursor: 0,
            move_candidates: Vec::new(),
            move_selected: 0,
            move_source_session_cwd: String::new(),
            dead_sessions,
            monitor_rows: Vec::new(),
            monitor_selected: 0,
            monitor_sort: MonitorSort::Mem,
            monitor_list_state: ListState::default(),
            confirming_process: None,
        };
        app.update_preview();
        Ok(app)
    }

    fn sort_monitor_rows(rows: &mut [ProcessRow], sort: MonitorSort) {
        match sort {
            MonitorSort::Mem => rows.sort_by(|a, b| b.rss_kb.cmp(&a.rss_kb)),
            MonitorSort::Cpu => rows.sort_by(|a, b| {
                b.pcpu.partial_cmp(&a.pcpu)
                    .unwrap_or(std::cmp::Ordering::Equal)
            }),
        }
    }

    fn reselect_monitor(&mut self, prev_pid: Option<u32>) {
        let new_index = prev_pid
            .and_then(|pid| self.monitor_rows.iter().position(|row| row.pid == pid))
            .unwrap_or_else(|| {
                self.monitor_selected.min(self.monitor_rows.len().saturating_sub(1))
            });
        self.monitor_selected = new_index;
        self.monitor_list_state.select(if self.monitor_rows.is_empty() {
            None
        } else {
            Some(new_index)
        });
    }

    fn apply_session_display_names(&self, rows: &mut [ProcessRow]) {
        let mut session_display_by_name: HashMap<String, String> = HashMap::new();
        for session in self.sessions.iter() {
            session_display_by_name.insert(session.name.clone(), session.display_name.clone());
        }
        for row in rows.iter_mut() {
            if let Some(display) = session_display_by_name.get(&row.pane.session_name) {
                row.pane.session_display = display.clone();
            }
        }
    }

    pub fn refresh_monitor(&mut self) -> io::Result<()> {
        let prev_pid = self.monitor_rows
            .get(self.monitor_selected)
            .map(|row| row.pid);
        let mut rows = procs::collect_process_rows()?;
        self.apply_session_display_names(&mut rows);
        Self::sort_monitor_rows(&mut rows, self.monitor_sort);
        self.monitor_rows = rows;
        self.reselect_monitor(prev_pid);
        Ok(())
    }

    pub fn refresh(&mut self) -> io::Result<()> {
        let prev_node_id = self.list_state.selected()
            .and_then(|i| self.flat_entries.get(i))
            .map(|e| e.node_id.clone());
        let prev_index = self.list_state.selected().unwrap_or(0);

        self.sessions = tmux::list_sessions(&self.current_session_id)?;
        config::apply_formatter_to_sessions(&mut self.sessions, &self.config);
        self.sessions.sort_by(|a, b| b.activity.cmp(&a.activity));
        self.windows = tmux::list_windows()?;
        self.panes = tmux::list_panes()?;

        if self.sessions.is_empty() {
            self.should_quit = true;
            return Ok(());
        }

        let mut history_entries = history::load_history();
        let now = current_unix_secs()?;
        history::upsert_live_sessions(&mut history_entries, &self.sessions, now);
        self.dead_sessions = compute_dead_sessions(&history_entries, &self.sessions, &self.config);

        let group_sep = self.config.as_ref().and_then(|c| c.group_name_separator.as_deref());
        for prefix in extract_group_prefixes(&self.sessions, group_sep) {
            if !self.seen_groups.contains(&prefix) {
                self.opened.insert(NodeId::Group(prefix.clone()));
                self.seen_groups.insert(prefix);
            }
        }

        self.rebuild_flat_entries();

        let new_index = prev_node_id
            .and_then(|id| self.flat_entries.iter().position(|e| e.node_id == id))
            .unwrap_or_else(|| prev_index.min(self.flat_entries.len().saturating_sub(1)));
        self.list_state.select(Some(new_index));

        self.update_preview();
        Ok(())
    }

    fn rebuild_flat_entries(&mut self) {
        if self.filter_query.is_empty() {
            let sep = self.config.as_ref().and_then(|c| c.group_name_separator.as_deref());
            self.flat_entries = tree::flatten(&self.sessions, &self.windows, &self.panes, &self.opened, &self.pinned, &self.hidden, self.show_hidden, sep);
        } else {
            let dead_refs: Vec<DeadSessionRef<'_>> = self.dead_sessions.iter().map(|d| DeadSessionRef {
                name: &d.name,
                display_name: &d.display_name,
                last_seen: d.last_seen,
            }).collect();
            self.flat_entries = tree::flatten_filtered(&self.sessions, &self.windows, &dead_refs, &self.filter_query);
        }
    }

    fn reset_move_window_state(&mut self) {
        self.move_query = String::new();
        self.move_cursor = 0;
        self.move_candidates.clear();
        self.move_selected = 0;
        self.move_source_session_cwd = String::new();
    }

    fn recompute_selection_range(&mut self) {
        let anchor = match self.selection_anchor {
            Some(anchor) => anchor,
            None => return,
        };
        let cursor = match self.list_state.selected() {
            Some(cursor) => cursor,
            None => return,
        };
        if self.flat_entries.is_empty() {
            self.marked_windows.clear();
            return;
        }

        let lo = anchor.min(cursor);
        let hi = anchor.max(cursor).min(self.flat_entries.len().saturating_sub(1));

        self.marked_windows.clear();
        for entry in self.flat_entries[lo..=hi].iter() {
            if let NodeId::Window(_, window_id) = &entry.node_id {
                self.marked_windows.push(window_id.clone());
            }
        }
    }

    fn rebuild_move_candidates(&mut self) {
        let query_lc = self.move_query.to_lowercase();
        let mut source_session_ids = HashSet::new();
        for window_id in self.marked_windows.iter() {
            let source_window = self.windows.iter()
                .find(|window| window.id == *window_id);
            if let Some(source_window) = source_window {
                source_session_ids.insert(source_window.session_id.clone());
            }
        }
        let excluded_session_id = if source_session_ids.len() == 1 {
            source_session_ids.iter().next().cloned()
        } else {
            None
        };

        let mut candidates = Vec::new();
        let mut shown_names = HashSet::new();

        for session in self.sessions.iter() {
            if let Some(id) = excluded_session_id.as_ref() {
                if session.id == *id {
                    continue;
                }
            }
            if !self.move_query.is_empty()
                && !session.display_name.to_lowercase().contains(&query_lc)
            {
                continue;
            }
            shown_names.insert(session.name.clone());
            candidates.push(MoveCandidate {
                label: session.display_name.clone(),
                dim: false,
                target: MoveTarget::Existing(session.name.clone()),
            });
        }

        for dead_session in self.dead_sessions.iter() {
            if shown_names.contains(&dead_session.name) {
                continue;
            }
            if !self.move_query.is_empty()
                && !dead_session.display_name.to_lowercase().contains(&query_lc)
            {
                continue;
            }
            shown_names.insert(dead_session.name.clone());
            candidates.push(MoveCandidate {
                label: dead_session.display_name.clone(),
                dim: true,
                target: MoveTarget::Dead {
                    name: dead_session.name.clone(),
                    cwd: dead_session.cwd.clone(),
                },
            });
        }

        let trimmed = self.move_query.trim();
        if !trimmed.is_empty() && !shown_names.contains(trimmed) {
            candidates.push(MoveCandidate {
                label: format!("+ Create new session \"{}\"", trimmed),
                dim: false,
                target: MoveTarget::New {
                    name: trimmed.to_string(),
                    cwd: self.move_source_session_cwd.clone(),
                },
            });
        }

        self.move_candidates = candidates;
        if self.move_candidates.is_empty() {
            self.move_selected = 0;
        } else if self.move_selected >= self.move_candidates.len() {
            self.move_selected = self.move_candidates.len() - 1;
        }
    }

    pub fn update_preview(&mut self) {
        let i = match self.list_state.selected() {
            Some(i) if i < self.flat_entries.len() => i,
            _ => {
                self.preview_panes.clear();
                self.preview_title.clear();
                return;
            }
        };

        let node_id = &self.flat_entries[i].node_id;
        match node_id {
            NodeId::Separator | NodeId::DeadSession(_) | NodeId::Group(_) => {
                self.preview_panes.clear();
                self.preview_title.clear();
            }
            NodeId::Session(session_id) => {
                let session_name = self.sessions.iter()
                    .find(|s| s.id == *session_id)
                    .map(|s| s.display_name.clone())
                    .unwrap_or_else(|| session_id.clone());
                self.preview_title = session_name;

                let session_windows: Vec<&tmux::Window> = self.windows.iter()
                    .filter(|w| w.session_id == *session_id)
                    .collect();

                self.preview_panes = session_windows.iter().map(|window| {
                    let pane_id = self.panes.iter()
                        .find(|p| p.session_id == *session_id && p.window_id == window.id && p.active)
                        .or_else(|| self.panes.iter().find(|p| p.session_id == *session_id && p.window_id == window.id))
                        .map(|p| p.id.clone());

                    let content = match pane_id {
                        Some(id) => tmux::capture_pane_raw(&id).unwrap_or_default(),
                        None => Vec::new(),
                    };

                    PreviewPane {
                        label: format!("{}:{}", window.index, window.name),
                        content,
                        is_active: window.active,
                    }
                }).collect();
            }
            NodeId::Window(session_id, window_id) => {
                let window = self.windows.iter().find(|w| w.id == *window_id);
                self.preview_title = format!(" {} (sort: index) ", i);

                let pane_id = self.panes.iter()
                    .find(|p| p.session_id == *session_id && p.window_id == *window_id && p.active)
                    .or_else(|| self.panes.iter().find(|p| p.session_id == *session_id && p.window_id == *window_id))
                    .map(|p| p.id.clone());

                let content = match pane_id {
                    Some(id) => tmux::capture_pane_raw(&id).unwrap_or_default(),
                    None => Vec::new(),
                };

                let label = window.map(|w| format!("{}:{}", w.index, w.name))
                    .unwrap_or_else(|| format!("{}", i));

                self.preview_panes = vec![PreviewPane {
                    label,
                    content,
                    is_active: true,
                }];
            }
            NodeId::Pane(_session_id, _window_id, pane_id) => {
                self.preview_title = format!(" {} (sort: index) ", i);

                let content = tmux::capture_pane_raw(pane_id).unwrap_or_default();
                let pane = self.panes.iter().find(|p| p.id == *pane_id);
                let label = pane.map(|p| format!("{}:{}", p.index, p.current_command))
                    .unwrap_or_else(|| format!("{}", i));

                self.preview_panes = vec![PreviewPane {
                    label,
                    content,
                    is_active: true,
                }];
            }
        }
    }

    fn build_full_preview(&self) -> (Vec<PreviewFullPane>, usize) {
        let i = match self.list_state.selected() {
            Some(i) if i < self.flat_entries.len() => i,
            _ => return (Vec::new(), 0),
        };

        let node_id = &self.flat_entries[i].node_id;
        match node_id {
            NodeId::Separator | NodeId::DeadSession(_) | NodeId::Group(_) => (Vec::new(), 0),
            NodeId::Pane(session_id, window_id, pane_id) => {
                let session = self.sessions.iter().find(|s| s.id == *session_id);
                let window = self.windows.iter().find(|w| w.id == *window_id);
                let pane = self.panes.iter().find(|p| p.id == *pane_id);

                let session_name = session.map(|s| s.display_name.clone()).unwrap_or_else(|| session_id.clone());
                let window_label = window.map(|w| format!("{}:{}", w.index, w.name)).unwrap_or_else(|| window_id.clone());
                let pane_label = pane.map(|p| format!("{}:{}", p.index, p.current_command)).unwrap_or_else(|| pane_id.clone());
                let content = tmux::capture_pane_raw(pane_id).unwrap_or_default();

                let preview = PreviewFullPane {
                    session_id: session_id.clone(),
                    window_id: window_id.clone(),
                    pane_id: pane_id.clone(),
                    session_name,
                    window_label,
                    pane_label,
                    content,
                };
                (vec![preview], 0)
            }
            NodeId::Window(session_id, window_id) => {
                let session = self.sessions.iter().find(|s| s.id == *session_id);
                let session_name = session.map(|s| s.display_name.clone()).unwrap_or_else(|| session_id.clone());

                let mut window_panes: Vec<&tmux::Pane> = self.panes.iter()
                    .filter(|p| p.session_id == *session_id && p.window_id == *window_id)
                    .collect();
                window_panes.sort_by(|a, b| a.index.cmp(&b.index));

                let initial_index = window_panes.iter()
                    .position(|p| p.active)
                    .unwrap_or(0);

                let previews: Vec<PreviewFullPane> = window_panes.iter().map(|pane| {
                    let window = self.windows.iter().find(|w| w.id == *window_id);
                    let window_label = window.map(|w| format!("{}:{}", w.index, w.name)).unwrap_or_else(|| window_id.clone());
                    let pane_label = format!("{}:{}", pane.index, pane.current_command);
                    let content = tmux::capture_pane_raw(&pane.id).unwrap_or_default();

                    PreviewFullPane {
                        session_id: session_id.clone(),
                        window_id: window_id.clone(),
                        pane_id: pane.id.clone(),
                        session_name: session_name.clone(),
                        window_label,
                        pane_label,
                        content,
                    }
                }).collect();

                (previews, initial_index)
            }
            NodeId::Session(session_id) => self.build_full_preview_for_session(session_id),
        }
    }

    fn build_full_preview_for_session(&self, session_id: &str) -> (Vec<PreviewFullPane>, usize) {
        let session = self.sessions.iter().find(|s| s.id == session_id);
        let session_name = session.map(|s| s.display_name.clone()).unwrap_or_else(|| session_id.to_string());

        let mut session_windows: Vec<&tmux::Window> = self.windows.iter()
            .filter(|w| w.session_id == session_id)
            .collect();
        session_windows.sort_by(|a, b| a.index.cmp(&b.index));

        let mut previews = Vec::new();
        let mut initial_index = 0;
        let mut found_active = false;
        let mut first_active_fallback = None;

        for window in &session_windows {
            let mut window_panes: Vec<&tmux::Pane> = self.panes.iter()
                .filter(|p| p.session_id == session_id && p.window_id == window.id)
                .collect();
            window_panes.sort_by(|a, b| a.index.cmp(&b.index));

            for pane in &window_panes {
                if !found_active && window.active && pane.active {
                    initial_index = previews.len();
                    found_active = true;
                }
                if first_active_fallback.is_none() && pane.active {
                    first_active_fallback = Some(previews.len());
                }

                let window_label = format!("{}:{}", window.index, window.name);
                let pane_label = format!("{}:{}", pane.index, pane.current_command);
                let content = tmux::capture_pane_raw(&pane.id).unwrap_or_default();

                previews.push(PreviewFullPane {
                    session_id: session_id.to_string(),
                    window_id: window.id.clone(),
                    pane_id: pane.id.clone(),
                    session_name: session_name.clone(),
                    window_label,
                    pane_label,
                    content,
                });
            }
        }

        if !found_active {
            initial_index = first_active_fallback.unwrap_or(0);
        }

        (previews, initial_index)
    }

    pub fn handle_action(&mut self, action: Action) {
        match action {
            Action::Quit => self.should_quit = true,
            Action::ClearMarksOrQuit => {
                if self.selecting || !self.marked_windows.is_empty() {
                    self.marked_windows.clear();
                    self.selecting = false;
                    self.selection_anchor = None;
                } else {
                    self.should_quit = true;
                }
            }
            Action::MoveUp => {
                if self.mode == Mode::Monitor {
                    if self.monitor_selected > 0 {
                        self.monitor_selected -= 1;
                        self.monitor_list_state.select(Some(self.monitor_selected));
                    }
                    return;
                }
                if let Some(i) = self.list_state.selected() {
                    let mut target = i;
                    while target > 0 {
                        target -= 1;
                        if self.flat_entries[target].node_id != NodeId::Separator {
                            self.list_state.select(Some(target));
                            self.update_preview();
                            if self.mode == Mode::Normal && self.selecting {
                                self.recompute_selection_range();
                            }
                            break;
                        }
                    }
                }
            }
            Action::MoveDown => {
                if self.mode == Mode::Monitor {
                    if self.monitor_selected + 1 < self.monitor_rows.len() {
                        self.monitor_selected += 1;
                        self.monitor_list_state.select(Some(self.monitor_selected));
                    }
                    return;
                }
                if let Some(i) = self.list_state.selected() {
                    let mut target = i;
                    while target + 1 < self.flat_entries.len() {
                        target += 1;
                        if self.flat_entries[target].node_id != NodeId::Separator {
                            self.list_state.select(Some(target));
                            self.update_preview();
                            if self.mode == Mode::Normal && self.selecting {
                                self.recompute_selection_range();
                            }
                            break;
                        }
                    }
                }
            }
            Action::TogglePin => {
                let i = match self.list_state.selected() {
                    Some(i) if i < self.flat_entries.len() => i,
                    _ => return,
                };
                let session_id = match &self.flat_entries[i].node_id {
                    NodeId::Session(id) => id.clone(),
                    NodeId::Window(session_id, _) => session_id.clone(),
                    NodeId::Pane(session_id, _, _) => session_id.clone(),
                    NodeId::Separator | NodeId::DeadSession(_) | NodeId::Group(_) => return,
                };
                let session_name = match self.sessions.iter().find(|s| s.id == session_id) {
                    Some(s) => s.name.clone(),
                    None => return,
                };
                if self.pinned.contains(&session_name) {
                    self.pinned.retain(|p| *p != session_name);
                } else {
                    self.pinned.push(session_name);
                }
                save_pins(&self.pinned);
                let current_node_id = self.flat_entries[i].node_id.clone();
                self.rebuild_flat_entries();
                if let Some(new_i) = self.flat_entries.iter().position(|e| e.node_id == current_node_id) {
                    self.list_state.select(Some(new_i));
                }
                self.update_preview();
            }
            Action::ToggleHide => {
                let i = match self.list_state.selected() {
                    Some(i) if i < self.flat_entries.len() => i,
                    _ => return,
                };
                let session_id = match &self.flat_entries[i].node_id {
                    NodeId::Session(id) => id.clone(),
                    NodeId::Window(session_id, _) => session_id.clone(),
                    NodeId::Pane(session_id, _, _) => session_id.clone(),
                    NodeId::Separator | NodeId::DeadSession(_) | NodeId::Group(_) => return,
                };
                let session_name = match self.sessions.iter().find(|s| s.id == session_id) {
                    Some(s) => s.name.clone(),
                    None => return,
                };
                if self.hidden.contains(&session_name) {
                    self.hidden.retain(|h| *h != session_name);
                } else {
                    self.hidden.push(session_name);
                }
                save_hidden(&self.hidden);
                let current_node_id = self.flat_entries[i].node_id.clone();
                self.rebuild_flat_entries();
                if let Some(new_i) = self.flat_entries.iter().position(|e| e.node_id == current_node_id) {
                    self.list_state.select(Some(new_i));
                } else if self.flat_entries.is_empty() {
                    self.list_state.select(None);
                } else {
                    let clamped = i.min(self.flat_entries.len() - 1);
                    self.list_state.select(Some(clamped));
                }
                self.update_preview();
            }
            Action::ToggleShowHidden => {
                let (current_node_id, i) = match self.list_state.selected() {
                    Some(i) if i < self.flat_entries.len() => {
                        (Some(self.flat_entries[i].node_id.clone()), i)
                    }
                    _ => (None, 0),
                };
                self.show_hidden = !self.show_hidden;
                self.rebuild_flat_entries();
                if let Some(node_id) = current_node_id {
                    if let Some(new_i) = self.flat_entries.iter().position(|e| e.node_id == node_id) {
                        self.list_state.select(Some(new_i));
                    } else if self.flat_entries.is_empty() {
                        self.list_state.select(None);
                    } else {
                        let clamped = i.min(self.flat_entries.len() - 1);
                        self.list_state.select(Some(clamped));
                    }
                }
                self.update_preview();
            }
            Action::MovePinUp => self.move_pin(-1),
            Action::MovePinDown => self.move_pin(1),
            Action::CollapseOrParent => {
                if self.selecting {
                    return;
                }
                if let Some(i) = self.list_state.selected() {
                    let node_id = self.flat_entries[i].node_id.clone();
                    if self.flat_entries[i].has_children && self.opened.contains(&node_id) {
                        self.opened.remove(&node_id);
                        self.rebuild_flat_entries();
                    } else {
                        let current_depth = self.flat_entries[i].depth;
                        if current_depth > 0 {
                            for j in (0..i).rev() {
                                if self.flat_entries[j].depth < current_depth {
                                    let parent_node_id = self.flat_entries[j].node_id.clone();
                                    self.opened.remove(&parent_node_id);
                                    self.rebuild_flat_entries();
                                    if let Some(new_i) = self.flat_entries.iter().position(|e| e.node_id == parent_node_id) {
                                        self.list_state.select(Some(new_i));
                                    }
                                    break;
                                }
                            }
                        }
                    }
                    self.update_preview();
                }
            }
            Action::ExpandOrChild => {
                if self.selecting {
                    return;
                }
                if let Some(i) = self.list_state.selected() {
                    let entry_has_children = self.flat_entries[i].has_children;
                    let entry_depth = self.flat_entries[i].depth;
                    let node_id = self.flat_entries[i].node_id.clone();
                    if entry_has_children {
                        if !self.opened.contains(&node_id) {
                            self.opened.insert(node_id);
                            self.rebuild_flat_entries();
                        }
                        if i + 1 < self.flat_entries.len()
                            && self.flat_entries[i + 1].depth > entry_depth
                        {
                            self.list_state.select(Some(i + 1));
                        }
                    }
                    self.update_preview();
                }
            }
            Action::EnterFullPreview => {
                let (panes, initial_index) = self.build_full_preview();
                if !panes.is_empty() {
                    self.preview_full_panes = panes;
                    self.preview_full_index = initial_index;
                    self.mode = Mode::Previewing;
                }
            }
            Action::ExitFullPreview => {
                self.mode = Mode::Normal;
                self.preview_full_panes.clear();
                self.preview_full_index = 0;
            }
            Action::PreviewPrev => {
                if !self.preview_full_panes.is_empty() {
                    let len = self.preview_full_panes.len();
                    self.preview_full_index = (self.preview_full_index + len - 1) % len;
                }
            }
            Action::PreviewNext => {
                if !self.preview_full_panes.is_empty() {
                    let len = self.preview_full_panes.len();
                    self.preview_full_index = (self.preview_full_index + 1) % len;
                }
            }
            Action::SelectPreviewPane => {
                if let Some(preview) = self.preview_full_panes.get(self.preview_full_index) {
                    let result = tmux::switch_client(&preview.session_id)
                        .and_then(|_| tmux::select_window(&preview.window_id))
                        .and_then(|_| tmux::select_pane(&preview.pane_id));
                    if result.is_ok() {
                        self.should_quit = true;
                    }
                }
            }
            Action::Select => {
                if self.mode == Mode::Monitor {
                    self.select_monitor_process();
                } else {
                    self.select_current();
                }
            }
            Action::Kill => self.start_kill(),
            Action::ConfirmKill => self.confirm_kill(),
            Action::CancelKill => {
                if self.confirming_process.is_some() {
                    self.confirming_process = None;
                    self.mode = Mode::Monitor;
                } else {
                    self.mode = Mode::Normal;
                    self.confirming_node = None;
                }
            }
            Action::OpenAbout => {
                self.mode = Mode::About;
            }
            Action::CloseAbout => {
                self.mode = Mode::Normal;
            }
            Action::Refresh => {
                let _ = self.refresh();
            }
            Action::EnterFilter => {
                self.mode = Mode::Filtering;
                self.filter_query = String::new();
                self.filter_cursor = 0;
                self.list_state.select(Some(0));
                self.rebuild_flat_entries();
                self.update_preview();
            }
            Action::ToggleMarkWindow => {
                if !self.selecting {
                    self.selecting = true;
                    self.selection_anchor = self.list_state.selected();
                    self.recompute_selection_range();
                } else {
                    self.selecting = false;
                    self.selection_anchor = None;
                }
            }
            Action::EnterMoveWindow => {
                let first_marked_window_id = match self.marked_windows.first() {
                    Some(window_id) => window_id.clone(),
                    None => return,
                };
                self.selecting = false;
                self.selection_anchor = None;
                let source_window = match self.windows.iter().find(|window| window.id == first_marked_window_id) {
                    Some(window) => window,
                    None => return,
                };
                let source_session_cwd = match self.sessions.iter().find(|session| session.id == source_window.session_id) {
                    Some(session) => session.cwd.clone(),
                    None => return,
                };
                self.reset_move_window_state();
                self.move_source_session_cwd = source_session_cwd;
                self.rebuild_move_candidates();
                self.mode = Mode::MoveWindow;
            }
            Action::FilterChar(c) => {
                let byte_offset = self.filter_query.char_indices()
                    .nth(self.filter_cursor)
                    .map(|(i, _)| i)
                    .unwrap_or(self.filter_query.len());
                self.filter_query.insert(byte_offset, c);
                self.filter_cursor += 1;
                self.rebuild_flat_entries();
                self.list_state.select(Some(0));
                self.update_preview();
            }
            Action::FilterBackspace => {
                if self.filter_cursor > 0 {
                    let byte_before = self.filter_query.char_indices()
                        .nth(self.filter_cursor - 1)
                        .map(|(i, _)| i)
                        .unwrap_or(self.filter_query.len());
                    let byte_at = self.filter_query.char_indices()
                        .nth(self.filter_cursor)
                        .map(|(i, _)| i)
                        .unwrap_or(self.filter_query.len());
                    self.filter_query.drain(byte_before..byte_at);
                    self.filter_cursor -= 1;
                    self.rebuild_flat_entries();
                    self.list_state.select(Some(0));
                    self.update_preview();
                }
            }
            Action::FilterDeleteForward => {
                let len = self.filter_query.chars().count();
                if self.filter_cursor < len {
                    let byte_at = self.filter_query.char_indices()
                        .nth(self.filter_cursor)
                        .map(|(i, _)| i)
                        .unwrap_or(self.filter_query.len());
                    let byte_next = self.filter_query.char_indices()
                        .nth(self.filter_cursor + 1)
                        .map(|(i, _)| i)
                        .unwrap_or(self.filter_query.len());
                    self.filter_query.drain(byte_at..byte_next);
                    self.rebuild_flat_entries();
                    self.list_state.select(Some(0));
                    self.update_preview();
                }
            }
            Action::FilterKillWord => {
                let chars: Vec<char> = self.filter_query.chars().collect();
                let mut pos = self.filter_cursor;
                while pos > 0 && chars[pos - 1].is_whitespace() {
                    pos -= 1;
                }
                while pos > 0 && !chars[pos - 1].is_whitespace() {
                    pos -= 1;
                }
                let start_byte = self.filter_query.char_indices()
                    .nth(pos)
                    .map(|(i, _)| i)
                    .unwrap_or(self.filter_query.len());
                let end_byte = self.filter_query.char_indices()
                    .nth(self.filter_cursor)
                    .map(|(i, _)| i)
                    .unwrap_or(self.filter_query.len());
                self.filter_query.drain(start_byte..end_byte);
                self.filter_cursor = pos;
                self.rebuild_flat_entries();
                self.list_state.select(Some(0));
                self.update_preview();
            }
            Action::FilterKillLine => {
                let byte_offset = self.filter_query.char_indices()
                    .nth(self.filter_cursor)
                    .map(|(i, _)| i)
                    .unwrap_or(self.filter_query.len());
                self.filter_query.drain(..byte_offset);
                self.filter_cursor = 0;
                self.rebuild_flat_entries();
                self.list_state.select(Some(0));
                self.update_preview();
            }
            Action::FilterKillLineForward => {
                let byte_offset = self.filter_query.char_indices()
                    .nth(self.filter_cursor)
                    .map(|(i, _)| i)
                    .unwrap_or(self.filter_query.len());
                self.filter_query.truncate(byte_offset);
                self.rebuild_flat_entries();
                self.list_state.select(Some(0));
                self.update_preview();
            }
            Action::FilterCursorLeft => {
                if self.filter_cursor > 0 {
                    self.filter_cursor -= 1;
                }
            }
            Action::FilterCursorRight => {
                let len = self.filter_query.chars().count();
                if self.filter_cursor < len {
                    self.filter_cursor += 1;
                }
            }
            Action::FilterCursorWordLeft => {
                let chars: Vec<char> = self.filter_query.chars().collect();
                let mut pos = self.filter_cursor;
                while pos > 0 && chars[pos - 1].is_whitespace() {
                    pos -= 1;
                }
                while pos > 0 && !chars[pos - 1].is_whitespace() {
                    pos -= 1;
                }
                self.filter_cursor = pos;
            }
            Action::FilterCursorWordRight => {
                let chars: Vec<char> = self.filter_query.chars().collect();
                let len = chars.len();
                let mut pos = self.filter_cursor;
                while pos < len && !chars[pos].is_whitespace() {
                    pos += 1;
                }
                while pos < len && chars[pos].is_whitespace() {
                    pos += 1;
                }
                self.filter_cursor = pos;
            }
            Action::FilterCursorStart => {
                self.filter_cursor = 0;
            }
            Action::FilterCursorEnd => {
                self.filter_cursor = self.filter_query.chars().count();
            }
            Action::ExitFilter => {
                let selected_node_id = self.list_state.selected()
                    .and_then(|i| self.flat_entries.get(i))
                    .map(|e| e.node_id.clone());
                self.filter_query = String::new();
                self.filter_cursor = 0;
                self.mode = Mode::Normal;
                self.rebuild_flat_entries();
                let new_index = selected_node_id
                    .and_then(|id| self.flat_entries.iter().position(|e| e.node_id == id))
                    .unwrap_or(0);
                self.list_state.select(Some(new_index));
                self.update_preview();
            }
            Action::MoveWindowChar(c) => {
                let byte_offset = self.move_query.char_indices()
                    .nth(self.move_cursor)
                    .map(|(i, _)| i)
                    .unwrap_or(self.move_query.len());
                self.move_query.insert(byte_offset, c);
                self.move_cursor += 1;
                self.rebuild_move_candidates();
                self.move_selected = 0;
            }
            Action::MoveWindowBackspace => {
                if self.move_cursor > 0 {
                    let byte_before = self.move_query.char_indices()
                        .nth(self.move_cursor - 1)
                        .map(|(i, _)| i)
                        .unwrap_or(self.move_query.len());
                    let byte_at = self.move_query.char_indices()
                        .nth(self.move_cursor)
                        .map(|(i, _)| i)
                        .unwrap_or(self.move_query.len());
                    self.move_query.drain(byte_before..byte_at);
                    self.move_cursor -= 1;
                    self.rebuild_move_candidates();
                    self.move_selected = 0;
                }
            }
            Action::MoveWindowDeleteForward => {
                let len = self.move_query.chars().count();
                if self.move_cursor < len {
                    let byte_at = self.move_query.char_indices()
                        .nth(self.move_cursor)
                        .map(|(i, _)| i)
                        .unwrap_or(self.move_query.len());
                    let byte_next = self.move_query.char_indices()
                        .nth(self.move_cursor + 1)
                        .map(|(i, _)| i)
                        .unwrap_or(self.move_query.len());
                    self.move_query.drain(byte_at..byte_next);
                    self.rebuild_move_candidates();
                    self.move_selected = 0;
                }
            }
            Action::MoveWindowKillWord => {
                let chars: Vec<char> = self.move_query.chars().collect();
                let mut pos = self.move_cursor;
                while pos > 0 && chars[pos - 1].is_whitespace() {
                    pos -= 1;
                }
                while pos > 0 && !chars[pos - 1].is_whitespace() {
                    pos -= 1;
                }
                let start_byte = self.move_query.char_indices()
                    .nth(pos)
                    .map(|(i, _)| i)
                    .unwrap_or(self.move_query.len());
                let end_byte = self.move_query.char_indices()
                    .nth(self.move_cursor)
                    .map(|(i, _)| i)
                    .unwrap_or(self.move_query.len());
                self.move_query.drain(start_byte..end_byte);
                self.move_cursor = pos;
                self.rebuild_move_candidates();
                self.move_selected = 0;
            }
            Action::MoveWindowKillLine => {
                let byte_offset = self.move_query.char_indices()
                    .nth(self.move_cursor)
                    .map(|(i, _)| i)
                    .unwrap_or(self.move_query.len());
                self.move_query.drain(..byte_offset);
                self.move_cursor = 0;
                self.rebuild_move_candidates();
                self.move_selected = 0;
            }
            Action::MoveWindowKillLineForward => {
                let byte_offset = self.move_query.char_indices()
                    .nth(self.move_cursor)
                    .map(|(i, _)| i)
                    .unwrap_or(self.move_query.len());
                self.move_query.truncate(byte_offset);
                self.rebuild_move_candidates();
                self.move_selected = 0;
            }
            Action::MoveWindowCursorLeft => {
                if self.move_cursor > 0 {
                    self.move_cursor -= 1;
                }
            }
            Action::MoveWindowCursorRight => {
                let len = self.move_query.chars().count();
                if self.move_cursor < len {
                    self.move_cursor += 1;
                }
            }
            Action::MoveWindowCursorWordLeft => {
                let chars: Vec<char> = self.move_query.chars().collect();
                let mut pos = self.move_cursor;
                while pos > 0 && chars[pos - 1].is_whitespace() {
                    pos -= 1;
                }
                while pos > 0 && !chars[pos - 1].is_whitespace() {
                    pos -= 1;
                }
                self.move_cursor = pos;
            }
            Action::MoveWindowCursorWordRight => {
                let chars: Vec<char> = self.move_query.chars().collect();
                let len = chars.len();
                let mut pos = self.move_cursor;
                while pos < len && !chars[pos].is_whitespace() {
                    pos += 1;
                }
                while pos < len && chars[pos].is_whitespace() {
                    pos += 1;
                }
                self.move_cursor = pos;
            }
            Action::MoveWindowCursorStart => {
                self.move_cursor = 0;
            }
            Action::MoveWindowCursorEnd => {
                self.move_cursor = self.move_query.chars().count();
            }
            Action::MoveWindowNext => {
                if self.move_selected + 1 < self.move_candidates.len() {
                    self.move_selected += 1;
                }
            }
            Action::MoveWindowPrev => {
                if self.move_selected > 0 {
                    self.move_selected -= 1;
                }
            }
            Action::ConfirmMoveWindow => {
                let sources = self.marked_windows.clone();
                if sources.is_empty() {
                    self.reset_move_window_state();
                    self.mode = Mode::Normal;
                    return;
                }
                let candidate = match self.move_candidates.get(self.move_selected).cloned() {
                    Some(candidate) => candidate,
                    None => return,
                };
                let mut target_session_id = None;
                let mut cleanup_window_id: Option<String> = None;
                let move_target = match candidate.target {
                    MoveTarget::Existing(name) => {
                        target_session_id = self.sessions.iter()
                            .find(|session| session.name == name)
                            .map(|session| session.id.clone());
                        name
                    }
                    MoveTarget::Dead { name, cwd } => {
                        match tmux::new_session(&name, &cwd) {
                            Ok(created) => {
                                target_session_id = Some(created.session_id.clone());
                                cleanup_window_id = Some(created.initial_window_id.clone());
                                created.session_id
                            }
                            Err(_) => {
                                self.reset_move_window_state();
                                self.mode = Mode::Normal;
                                return;
                            }
                        }
                    }
                    MoveTarget::New { name, cwd } => {
                        match tmux::new_session(&name, &cwd) {
                            Ok(created) => {
                                target_session_id = Some(created.session_id.clone());
                                cleanup_window_id = Some(created.initial_window_id.clone());
                                created.session_id
                            }
                            Err(_) => {
                                self.reset_move_window_state();
                                self.mode = Mode::Normal;
                                return;
                            }
                        }
                    }
                };
                let mut moved_any = false;
                for window_id in sources.iter() {
                    let current_session_id = self.windows.iter()
                        .find(|window| window.id == *window_id)
                        .map(|window| window.session_id.clone());
                    if let Some(existing_target_session_id) = target_session_id.as_ref() {
                        if let Some(current_session_id) = current_session_id.as_ref() {
                            if *current_session_id == *existing_target_session_id {
                                continue;
                            }
                        }
                    }
                    if tmux::move_window(window_id, &move_target).is_ok() {
                        moved_any = true;
                    }
                }
                if let Some(window_id) = cleanup_window_id {
                    if moved_any {
                        let _ = tmux::kill_window(&window_id);
                    }
                }
                self.marked_windows.clear();
                self.selecting = false;
                self.selection_anchor = None;
                self.reset_move_window_state();
                self.mode = Mode::Normal;
                let _ = self.refresh();
            }
            Action::CancelMoveWindow => {
                self.selecting = false;
                self.selection_anchor = None;
                self.reset_move_window_state();
                self.mode = Mode::Normal;
            }
            Action::SelectIndex(i) => {
                if i < self.flat_entries.len() {
                    self.list_state.select(Some(i));
                    self.select_current();
                }
            }
            Action::StartRename => {
                let i = match self.list_state.selected() {
                    Some(i) if i < self.flat_entries.len() => i,
                    _ => return,
                };
                let (target, prefill) = match &self.flat_entries[i].node_id {
                    NodeId::Session(id) => {
                        let name = match self.sessions.iter().find(|s| s.id == *id) {
                            Some(s) => s.name.clone(),
                            None => return,
                        };
                        (RenameTarget::Session(id.clone()), name)
                    }
                    NodeId::Window(_, window_id) | NodeId::Pane(_, window_id, _) => {
                        let name = match self.windows.iter().find(|w| w.id == *window_id) {
                            Some(w) => w.name.clone(),
                            None => return,
                        };
                        (RenameTarget::Window(window_id.clone()), name)
                    }
                    NodeId::Group(_) | NodeId::Separator | NodeId::DeadSession(_) => return,
                };
                self.renaming_target = Some(target);
                self.rename_cursor = prefill.chars().count();
                self.rename_buffer = prefill;
                self.mode = Mode::Renaming;
            }
            Action::RenameChar(c) => {
                let byte_offset = self.rename_buffer.char_indices()
                    .nth(self.rename_cursor)
                    .map(|(i, _)| i)
                    .unwrap_or(self.rename_buffer.len());
                self.rename_buffer.insert(byte_offset, c);
                self.rename_cursor += 1;
            }
            Action::RenameBackspace => {
                if self.rename_cursor > 0 {
                    let byte_before = self.rename_buffer.char_indices()
                        .nth(self.rename_cursor - 1)
                        .map(|(i, _)| i)
                        .unwrap_or(self.rename_buffer.len());
                    let byte_at = self.rename_buffer.char_indices()
                        .nth(self.rename_cursor)
                        .map(|(i, _)| i)
                        .unwrap_or(self.rename_buffer.len());
                    self.rename_buffer.drain(byte_before..byte_at);
                    self.rename_cursor -= 1;
                }
            }
            Action::RenameDeleteForward => {
                let len = self.rename_buffer.chars().count();
                if self.rename_cursor < len {
                    let byte_at = self.rename_buffer.char_indices()
                        .nth(self.rename_cursor)
                        .map(|(i, _)| i)
                        .unwrap_or(self.rename_buffer.len());
                    let byte_next = self.rename_buffer.char_indices()
                        .nth(self.rename_cursor + 1)
                        .map(|(i, _)| i)
                        .unwrap_or(self.rename_buffer.len());
                    self.rename_buffer.drain(byte_at..byte_next);
                }
            }
            Action::RenameKillWord => {
                let chars: Vec<char> = self.rename_buffer.chars().collect();
                let mut pos = self.rename_cursor;
                while pos > 0 && chars[pos - 1].is_whitespace() {
                    pos -= 1;
                }
                while pos > 0 && !chars[pos - 1].is_whitespace() {
                    pos -= 1;
                }
                let start_byte = self.rename_buffer.char_indices()
                    .nth(pos)
                    .map(|(i, _)| i)
                    .unwrap_or(self.rename_buffer.len());
                let end_byte = self.rename_buffer.char_indices()
                    .nth(self.rename_cursor)
                    .map(|(i, _)| i)
                    .unwrap_or(self.rename_buffer.len());
                self.rename_buffer.drain(start_byte..end_byte);
                self.rename_cursor = pos;
            }
            Action::RenameKillLine => {
                let byte_offset = self.rename_buffer.char_indices()
                    .nth(self.rename_cursor)
                    .map(|(i, _)| i)
                    .unwrap_or(self.rename_buffer.len());
                self.rename_buffer.drain(..byte_offset);
                self.rename_cursor = 0;
            }
            Action::RenameKillLineForward => {
                let byte_offset = self.rename_buffer.char_indices()
                    .nth(self.rename_cursor)
                    .map(|(i, _)| i)
                    .unwrap_or(self.rename_buffer.len());
                self.rename_buffer.truncate(byte_offset);
            }
            Action::RenameCursorLeft => {
                if self.rename_cursor > 0 {
                    self.rename_cursor -= 1;
                }
            }
            Action::RenameCursorRight => {
                let len = self.rename_buffer.chars().count();
                if self.rename_cursor < len {
                    self.rename_cursor += 1;
                }
            }
            Action::RenameCursorWordLeft => {
                let chars: Vec<char> = self.rename_buffer.chars().collect();
                let mut pos = self.rename_cursor;
                while pos > 0 && chars[pos - 1].is_whitespace() {
                    pos -= 1;
                }
                while pos > 0 && !chars[pos - 1].is_whitespace() {
                    pos -= 1;
                }
                self.rename_cursor = pos;
            }
            Action::RenameCursorWordRight => {
                let chars: Vec<char> = self.rename_buffer.chars().collect();
                let len = chars.len();
                let mut pos = self.rename_cursor;
                while pos < len && !chars[pos].is_whitespace() {
                    pos += 1;
                }
                while pos < len && chars[pos].is_whitespace() {
                    pos += 1;
                }
                self.rename_cursor = pos;
            }
            Action::RenameCursorStart => {
                self.rename_cursor = 0;
            }
            Action::RenameCursorEnd => {
                self.rename_cursor = self.rename_buffer.chars().count();
            }
            Action::ConfirmRename => {
                let target = match self.renaming_target.clone() {
                    Some(target) => target,
                    None => {
                        self.mode = Mode::Normal;
                        return;
                    }
                };
                let trimmed = self.rename_buffer.trim().to_string();
                let current_name = match &target {
                    RenameTarget::Session(id) => self.sessions.iter()
                        .find(|s| s.id == *id)
                        .map(|s| s.name.clone()),
                    RenameTarget::Window(id) => self.windows.iter()
                        .find(|w| w.id == *id)
                        .map(|w| w.name.clone()),
                };
                let should_rename = match &current_name {
                    Some(name) => !trimmed.is_empty() && trimmed != *name,
                    None => false,
                };
                let rename_result = if should_rename {
                    match &target {
                        RenameTarget::Session(id) => Some(tmux::rename_session(id, &trimmed)),
                        RenameTarget::Window(id) => Some(tmux::rename_window(id, &trimmed)),
                    }
                } else {
                    None
                };
                self.mode = Mode::Normal;
                self.renaming_target = None;
                self.rename_buffer = String::new();
                self.rename_cursor = 0;
                if let Some(Ok(())) = rename_result {
                    let _ = self.refresh();
                }
            }
            Action::CancelRename => {
                self.mode = Mode::Normal;
                self.renaming_target = None;
                self.rename_buffer = String::new();
                self.rename_cursor = 0;
            }
            Action::EnterMonitor => {
                self.mode = Mode::Monitor;
                let _ = self.refresh_monitor();
            }
            Action::ExitMonitor => {
                self.mode = Mode::Normal;
            }
            Action::ToggleMonitorSort => {
                if self.mode != Mode::Monitor {
                    return;
                }
                self.monitor_sort = match self.monitor_sort {
                    MonitorSort::Mem => MonitorSort::Cpu,
                    MonitorSort::Cpu => MonitorSort::Mem,
                };
                let prev_pid = self.monitor_rows
                    .get(self.monitor_selected)
                    .map(|row| row.pid);
                Self::sort_monitor_rows(&mut self.monitor_rows, self.monitor_sort);
                self.reselect_monitor(prev_pid);
            }
            Action::OpenProcessDetail => {
                if self.mode != Mode::Monitor {
                    return;
                }
                if self.monitor_rows.get(self.monitor_selected).is_none() {
                    return;
                }
                self.mode = Mode::ProcessDetail;
            }
            Action::CloseProcessDetail => {
                self.mode = Mode::Monitor;
            }
            Action::Tick => {
                if self.mode == Mode::Monitor {
                    let _ = self.refresh_monitor();
                }
            }
            Action::None => {}
        }
    }

    fn select_monitor_process(&mut self) {
        let row = match self.monitor_rows.get(self.monitor_selected) {
            Some(row) => row,
            None => return,
        };
        let result = tmux::switch_client(&row.pane.session_name)
            .and_then(|_| tmux::select_pane(&row.pane.pane_id));
        if result.is_ok() {
            self.should_quit = true;
        }
    }

    fn move_pin(&mut self, direction: i8) {
        let i = match self.list_state.selected() {
            Some(i) if i < self.flat_entries.len() => i,
            _ => return,
        };
        let session_id = match &self.flat_entries[i].node_id {
            NodeId::Session(id) => id.clone(),
            NodeId::Window(session_id, _) => session_id.clone(),
            NodeId::Pane(session_id, _, _) => session_id.clone(),
            NodeId::Group(_) | NodeId::Separator | NodeId::DeadSession(_) => return,
        };
        let session_name = match self.sessions.iter().find(|s| s.id == session_id) {
            Some(s) => s.name.clone(),
            None => return,
        };
        let pos = match self.pinned.iter().position(|p| *p == session_name) {
            Some(p) => p,
            None => return,
        };
        let new_pos = match direction {
            -1 if pos > 0 => pos - 1,
            1 if pos + 1 < self.pinned.len() => pos + 1,
            _ => return,
        };
        self.pinned.swap(pos, new_pos);
        save_pins(&self.pinned);
        let current_node_id = self.flat_entries[i].node_id.clone();
        self.rebuild_flat_entries();
        if let Some(new_i) = self.flat_entries.iter().position(|e| e.node_id == current_node_id) {
            self.list_state.select(Some(new_i));
        }
        self.update_preview();
    }

    fn select_current(&mut self) {
        let i = match self.list_state.selected() {
            Some(i) if i < self.flat_entries.len() => i,
            _ => return,
        };

        let entry = &self.flat_entries[i];
        let node_id = &entry.node_id;
        let result = match node_id {
            NodeId::Session(id) => tmux::switch_client(id),
            NodeId::Window(session_id, window_id) => tmux::switch_client(session_id)
                .and_then(|_| tmux::select_window(window_id)),
            NodeId::Pane(session_id, window_id, pane_id) => tmux::switch_client(session_id)
                .and_then(|_| tmux::select_window(window_id))
                .and_then(|_| tmux::select_pane(pane_id)),
            NodeId::Separator | NodeId::Group(_) => return,
            NodeId::DeadSession(name) => {
                let cwd = match self.dead_sessions.iter().find(|d| d.name == *name) {
                    Some(d) => d.cwd.clone(),
                    None => return,
                };
                tmux::new_session(name, &cwd)
                    .and_then(|_| tmux::switch_client(name))
            }
        };

        if result.is_ok() {
            self.should_quit = true;
        }
    }

    fn start_kill(&mut self) {
        if self.mode == Mode::Monitor {
            let row = match self.monitor_rows.get(self.monitor_selected) {
                Some(row) => row,
                None => return,
            };
            self.confirming_process = Some((row.pid, row.command.clone()));
            self.mode = Mode::Confirming;
            return;
        }

        let i = match self.list_state.selected() {
            Some(i) if i < self.flat_entries.len() => i,
            _ => return,
        };

        match &self.flat_entries[i].node_id {
            NodeId::Separator | NodeId::DeadSession(_) | NodeId::Group(_) => return,
            _ => {}
        }

        self.confirming_node = Some(self.flat_entries[i].node_id.clone());
        self.mode = Mode::Confirming;
    }

    fn confirm_kill(&mut self) {
        if let Some(entry) = self.confirming_process.clone() {
            let pid = entry.0;
            let result = procs::kill_process(pid);
            self.confirming_process = None;
            self.mode = Mode::Monitor;
            if result.is_ok() {
                let _ = self.refresh_monitor();
            }
            return;
        }

        let node_id = match &self.confirming_node {
            Some(id) => id.clone(),
            None => return,
        };

        let is_current_session = match &node_id {
            NodeId::Session(id) => *id == self.current_session_id,
            _ => false,
        };

        if is_current_session {
            let alternate = self
                .sessions
                .iter()
                .find(|s| s.id != self.current_session_id)
                .map(|s| s.id.clone());

            if let Some(target_id) = alternate {
                let _ = tmux::switch_client(&target_id);
            }
            let _ = tmux::kill_session(&self.current_session_id);
            self.should_quit = true;
            return;
        }

        let result = match &node_id {
            NodeId::Session(id) => tmux::kill_session(id),
            NodeId::Window(_, window_id) => tmux::kill_window(window_id),
            NodeId::Pane(_, _, pane_id) => tmux::kill_pane(pane_id),
            NodeId::Separator | NodeId::DeadSession(_) | NodeId::Group(_) => return,
        };

        self.mode = Mode::Normal;
        self.confirming_node = None;

        if result.is_ok() {
            let _ = self.refresh();
        }
    }

    pub fn confirming_label(&self) -> Option<String> {
        if let Some(entry) = self.confirming_process.as_ref() {
            return Some(format!("{} ({})", entry.1, entry.0));
        }
        let node_id = self.confirming_node.as_ref()?;
        let label = match node_id {
            NodeId::Session(id) => {
                let name = self
                    .sessions
                    .iter()
                    .find(|s| s.id == *id)
                    .map(|s| s.display_name.as_str())
                    .unwrap_or(id);
                format!("session \"{}\"", name)
            }
            NodeId::Window(_, window_id) => {
                let name = self
                    .windows
                    .iter()
                    .find(|w| w.id == *window_id)
                    .map(|w| w.name.as_str())
                    .unwrap_or(window_id);
                format!("window \"{}\"", name)
            }
            NodeId::Pane(_, _, pane_id) => {
                format!("pane {}", pane_id)
            }
            NodeId::Separator | NodeId::DeadSession(_) | NodeId::Group(_) => return None,
        };
        Some(label)
    }
}
