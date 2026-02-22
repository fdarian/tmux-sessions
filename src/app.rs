use std::collections::HashSet;
use std::io;

use ratatui::widgets::ListState;

use crate::event::{Action, Mode};
use crate::tmux;
use crate::tree::{self, FlatEntry, NodeId};

pub struct App {
    pub sessions: Vec<tmux::Session>,
    pub windows: Vec<tmux::Window>,
    pub panes: Vec<tmux::Pane>,
    pub flat_entries: Vec<FlatEntry>,
    pub opened: HashSet<NodeId>,
    pub list_state: ListState,
    pub preview_content: String,
    pub mode: Mode,
    pub confirming_node: Option<NodeId>,
    pub should_quit: bool,
}

impl App {
    pub fn new() -> io::Result<Self> {
        let sessions = tmux::list_sessions()?;
        let windows = tmux::list_windows()?;
        let panes = tmux::list_panes()?;

        let mut opened = HashSet::new();
        for session in &sessions {
            if session.attached {
                opened.insert(NodeId::Session(session.id.clone()));
            }
        }

        let flat_entries = tree::flatten(&sessions, &windows, &panes, &opened);
        let mut list_state = ListState::default();
        let initial_index = flat_entries
            .iter()
            .position(|e| {
                sessions.iter().any(|s| {
                    s.attached && e.node_id == NodeId::Session(s.id.clone())
                })
            })
            .or_else(|| if flat_entries.is_empty() { None } else { Some(0) });
        list_state.select(initial_index);

        let mut app = App {
            sessions,
            windows,
            panes,
            flat_entries,
            opened,
            list_state,
            preview_content: String::new(),
            mode: Mode::Normal,
            confirming_node: None,
            should_quit: false,
        };
        app.update_preview();
        Ok(app)
    }
        Ok(app)
    }

    pub fn refresh(&mut self) -> io::Result<()> {
        self.sessions = tmux::list_sessions()?;
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
        self.flat_entries = tree::flatten(&self.sessions, &self.windows, &self.panes, &self.opened);
    }

    pub fn update_preview(&mut self) {
        let i = match self.list_state.selected() {
            Some(i) if i < self.flat_entries.len() => i,
            _ => {
                self.preview_content.clear();
                return;
            }
        };

        let node_id = &self.flat_entries[i].node_id;
        let pane_id = tree::resolve_preview_pane_id(node_id, &self.windows, &self.panes);
        self.preview_content = match pane_id {
            Some(id) => tmux::capture_pane(&id).unwrap_or_default(),
            None => String::new(),
        };
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
                    .map(|s| s.name.as_str())
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
