use std::collections::HashSet;
use std::io;

use ratatui::style::{Color, Style};
use ratatui::widgets::ListState;

use crate::config;
use crate::event::{Action, Mode};
use crate::tmux;
use crate::tree::{self, FlatEntry, NodeId};

pub struct PreviewPane {
    pub label: String,
    pub content: Vec<u8>,
    pub is_active: bool,
}

pub struct App {
    pub config: Option<config::Config>,
    pub sessions: Vec<tmux::Session>,
    pub windows: Vec<tmux::Window>,
    pub panes: Vec<tmux::Pane>,
    pub flat_entries: Vec<FlatEntry>,
    pub opened: HashSet<NodeId>,
    pub list_state: ListState,
    pub preview_panes: Vec<PreviewPane>,
    pub preview_title: String,
    pub mode: Mode,
    pub confirming_node: Option<NodeId>,
    pub should_quit: bool,
    pub highlight_style: Style,
    pub primary_color: Color,
    pub filter_query: String,
    pub filter_cursor: usize,
}

impl App {
    pub fn new() -> io::Result<Self> {
        let config = config::load_config()?;
        let mut sessions = tmux::list_sessions()?;
        config::apply_formatter_to_sessions(&mut sessions, &config);
        let windows = tmux::list_windows()?;
        let panes = tmux::list_panes()?;

        let mut opened = HashSet::new();
        for session in &sessions {
            if session.attached {
                opened.insert(NodeId::Session(session.id.clone()));
                for window in &windows {
                    if window.session_id == session.id && window.active {
                        opened.insert(NodeId::Window(session.id.clone(), window.id.clone()));
                    }
                }
            }
        }

        let flat_entries = tree::flatten(&sessions, &windows, &panes, &opened);
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
            sessions,
            windows,
            panes,
            flat_entries,
            opened,
            list_state,
            preview_panes: Vec::new(),
            preview_title: String::new(),
            mode: Mode::Normal,
            confirming_node: None,
            should_quit: false,
            highlight_style,
            primary_color,
            filter_query: String::new(),
            filter_cursor: 0,
        };
        app.update_preview();
        Ok(app)
    }

    pub fn refresh(&mut self) -> io::Result<()> {
        self.sessions = tmux::list_sessions()?;
        config::apply_formatter_to_sessions(&mut self.sessions, &self.config);
        self.windows = tmux::list_windows()?;
        self.panes = tmux::list_panes()?;

        if self.sessions.is_empty() {
            self.should_quit = true;
            return Ok(());
        }

        self.rebuild_flat_entries();
        self.list_state.select(Some(0));
        self.update_preview();
        Ok(())
    }

    fn rebuild_flat_entries(&mut self) {
        if self.filter_query.is_empty() {
            self.flat_entries = tree::flatten(&self.sessions, &self.windows, &self.panes, &self.opened);
        } else {
            self.flat_entries = tree::flatten_filtered(&self.sessions, &self.windows, &self.panes, &self.filter_query);
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

    pub fn handle_action(&mut self, action: Action) {
        match action {
            Action::Quit => self.should_quit = true,
            Action::MoveUp => {
                if let Some(i) = self.list_state.selected() {
                    if i > 0 {
                        self.list_state.select(Some(i - 1));
                        self.update_preview();
                    }
                }
            }
            Action::MoveDown => {
                if let Some(i) = self.list_state.selected() {
                    if i + 1 < self.flat_entries.len() {
                        self.list_state.select(Some(i + 1));
                        self.update_preview();
                    }
                }
            }
            Action::CollapseOrParent => {
                if let Some(i) = self.list_state.selected() {
                    let node_id = self.flat_entries[i].node_id.clone();
                    if self.flat_entries[i].has_children && self.opened.contains(&node_id) {
                        self.opened.remove(&node_id);
                        self.rebuild_flat_entries();
                    } else {
                        self.move_to_parent(i);
                    }
                    self.update_preview();
                }
            }
            Action::ExpandOrChild => {
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
            Action::Toggle => {
                if let Some(i) = self.list_state.selected() {
                    if self.flat_entries[i].has_children {
                        let node_id = self.flat_entries[i].node_id.clone();
                        if self.opened.contains(&node_id) {
                            self.opened.remove(&node_id);
                        } else {
                            self.opened.insert(node_id);
                        }
                        self.rebuild_flat_entries();
                        if i >= self.flat_entries.len() {
                            self.list_state
                                .select(Some(self.flat_entries.len().saturating_sub(1)));
                        }
                    }
                }
            }
            Action::Select => self.select_current(),
            Action::Kill => self.start_kill(),
            Action::ConfirmKill => self.confirm_kill(),
            Action::CancelKill => {
                self.mode = Mode::Normal;
                self.confirming_node = None;
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
                self.filter_query = String::new();
                self.filter_cursor = 0;
                self.mode = Mode::Normal;
                self.rebuild_flat_entries();
                self.list_state.select(Some(0));
                self.update_preview();
            }
            Action::None => {}
        }
    }

    fn move_to_parent(&mut self, current_index: usize) {
        let current_depth = self.flat_entries[current_index].depth;
        if current_depth == 0 {
            return;
        }
        for j in (0..current_index).rev() {
            if self.flat_entries[j].depth < current_depth {
                self.list_state.select(Some(j));
                return;
            }
        }
    }

    fn select_current(&mut self) {
        let i = match self.list_state.selected() {
            Some(i) if i < self.flat_entries.len() => i,
            _ => return,
        };

        let node_id = &self.flat_entries[i].node_id;
        let result = match node_id {
            NodeId::Session(id) => tmux::switch_client(id),
            NodeId::Window(session_id, window_id) => tmux::switch_client(session_id)
                .and_then(|_| tmux::select_window(window_id)),
            NodeId::Pane(session_id, window_id, pane_id) => tmux::switch_client(session_id)
                .and_then(|_| tmux::select_window(window_id))
                .and_then(|_| tmux::select_pane(pane_id)),
        };

        if result.is_ok() {
            self.should_quit = true;
        }
    }

    fn start_kill(&mut self) {
        let i = match self.list_state.selected() {
            Some(i) if i < self.flat_entries.len() => i,
            _ => return,
        };

        self.confirming_node = Some(self.flat_entries[i].node_id.clone());
        self.mode = Mode::Confirming;
    }

    fn confirm_kill(&mut self) {
        let node_id = match &self.confirming_node {
            Some(id) => id.clone(),
            None => return,
        };

        let result = match &node_id {
            NodeId::Session(id) => tmux::kill_session(id),
            NodeId::Window(_, window_id) => tmux::kill_window(window_id),
            NodeId::Pane(_, _, pane_id) => tmux::kill_pane(pane_id),
        };

        self.mode = Mode::Normal;
        self.confirming_node = None;

        if result.is_ok() {
            let _ = self.refresh();
        }
    }

    pub fn confirming_label(&self) -> Option<String> {
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
        };
        Some(label)
    }
}
