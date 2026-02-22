use std::io;

use tui_tree_widget::{TreeItem, TreeState};

use crate::event::{Action, Mode};
use crate::tmux;
use crate::tree::{self, NodeId};

pub struct App {
    pub sessions: Vec<tmux::Session>,
    pub windows: Vec<tmux::Window>,
    pub panes: Vec<tmux::Pane>,
    pub tree_items: Vec<TreeItem<'static, NodeId>>,
    pub tree_state: TreeState<NodeId>,
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
        let tree_items = tree::build_tree_items(&sessions, &windows, &panes);

        let mut tree_state = TreeState::default();
        tree_state.select_first();

        let mut app = App {
            sessions,
            windows,
            panes,
            tree_items,
            tree_state,
            preview_content: String::new(),
            mode: Mode::Normal,
            confirming_node: None,
            should_quit: false,
        };
        app.update_preview();
        Ok(app)
    }

    pub fn refresh(&mut self) -> io::Result<()> {
        self.sessions = tmux::list_sessions()?;
        self.windows = tmux::list_windows()?;
        self.panes = tmux::list_panes()?;
        self.tree_items = tree::build_tree_items(&self.sessions, &self.windows, &self.panes);

        if self.sessions.is_empty() {
            self.should_quit = true;
            return Ok(());
        }

        self.tree_state.select_first();
        self.update_preview();
        Ok(())
    }

    pub fn update_preview(&mut self) {
        let selected = self.tree_state.selected();
        if selected.is_empty() {
            self.preview_content.clear();
            return;
        }

        let node_id = &selected[selected.len() - 1];
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
                self.tree_state.key_up();
                self.update_preview();
            }
            Action::MoveDown => {
                self.tree_state.key_down();
                self.update_preview();
            }
            Action::CollapseOrParent => {
                self.tree_state.key_left();
                self.update_preview();
            }
            Action::ExpandOrChild => {
                self.tree_state.key_right();
                self.update_preview();
            }
            Action::Toggle => {
                self.tree_state.toggle_selected();
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

    fn select_current(&mut self) {
        let selected = self.tree_state.selected();
        if selected.is_empty() {
            return;
        }

        let node_id = &selected[selected.len() - 1];
        let result = match node_id {
            NodeId::Session(id) => tmux::switch_client(id),
            NodeId::Window(session_id, window_id) => {
                tmux::switch_client(session_id)
                    .and_then(|_| tmux::select_window(window_id))
            }
            NodeId::Pane(session_id, window_id, pane_id) => {
                tmux::switch_client(session_id)
                    .and_then(|_| tmux::select_window(window_id))
                    .and_then(|_| tmux::select_pane(pane_id))
            }
        };

        if result.is_ok() {
            self.should_quit = true;
        }
    }

    fn start_kill(&mut self) {
        let selected = self.tree_state.selected();
        if selected.is_empty() {
            return;
        }

        let node_id = selected[selected.len() - 1].clone();
        self.confirming_node = Some(node_id);
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
