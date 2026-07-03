use crate::app::App;
use crate::event::Mode;
use crate::procs;
use crate::tmux;
use crate::tree::NodeId;

#[derive(Clone)]
pub enum ConfirmKillTarget {
    Node(NodeId),
    Windows(Vec<String>),
}

impl App {
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

        let selected_windows = self.selected_window_ids().to_vec();
        if !selected_windows.is_empty() {
            self.confirming_kill_target = Some(ConfirmKillTarget::Windows(selected_windows));
            self.mode = Mode::Confirming;
            return;
        }

        let i = match self.list_state.selected() {
            Some(i) if i < self.flat_entries.len() => i,
            _ => return,
        };

        match self.flat_entries[i].node_id.target() {
            NodeId::Separator(_)
            | NodeId::DeadSession(_)
            | NodeId::Group(_)
            | NodeId::Header(_) => return,
            _ => {}
        }

        self.confirming_kill_target = Some(ConfirmKillTarget::Node(
            self.flat_entries[i].node_id.clone(),
        ));
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

        let target = match self.confirming_kill_target.clone() {
            Some(target) => target,
            None => return,
        };

        match target {
            ConfirmKillTarget::Windows(window_ids) => {
                for window_id in window_ids.iter() {
                    let _ = tmux::kill_window(window_id);
                }
                self.mode = Mode::Normal;
                self.confirming_kill_target = None;
                self.marked_windows.clear();
                self.selecting = false;
                self.selection_anchor = None;
                let _ = self.refresh();
            }
            ConfirmKillTarget::Node(node_id) => {
                let is_current_session = match node_id.target() {
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
                    self.confirming_kill_target = None;
                    self.should_quit = true;
                    return;
                }

                let result = match node_id.target() {
                    NodeId::Session(id) => tmux::kill_session(id),
                    NodeId::Window(_, window_id) => tmux::kill_window(window_id),
                    NodeId::Pane(_, _, pane_id) => tmux::kill_pane(pane_id),
                    NodeId::Separator(_)
                    | NodeId::DeadSession(_)
                    | NodeId::Group(_)
                    | NodeId::Header(_) => return,
                    NodeId::Recent(_) => unreachable!(),
                };

                self.mode = Mode::Normal;
                self.confirming_kill_target = None;

                if result.is_ok() {
                    let _ = self.refresh();
                }
            }
        }
    }

    pub fn confirming_message(&self) -> Option<String> {
        if let Some(entry) = self.confirming_process.as_ref() {
            let label = Self::truncate_confirmation_label(&format!("{} ({})", entry.1, entry.0));
            return Some(format!("Kill {}?\n[enter] confirm  [esc] cancel", label));
        }
        let target = self.confirming_kill_target.as_ref()?;
        match target {
            ConfirmKillTarget::Windows(window_ids) => {
                let count = window_ids.len();
                if count == 1 {
                    Some("Kill 1 selected window?\n[enter] confirm  [esc] cancel".to_string())
                } else {
                    Some(format!(
                        "Kill {} selected windows?\n[enter] confirm  [esc] cancel",
                        count
                    ))
                }
            }
            ConfirmKillTarget::Node(node_id) => {
                let label = match node_id.target() {
                    NodeId::Session(id) => {
                        let session = match self.sessions.iter().find(|session| session.id == *id) {
                            Some(session) => session,
                            None => return None,
                        };
                        format!("session \"{}\"", session.display_name)
                    }
                    NodeId::Window(_, window_id) => {
                        let window =
                            match self.windows.iter().find(|window| window.id == *window_id) {
                                Some(window) => window,
                                None => return None,
                            };
                        format!("window \"{}\"", window.name)
                    }
                    NodeId::Pane(_, _, pane_id) => {
                        format!("pane {}", pane_id)
                    }
                    NodeId::Separator(_)
                    | NodeId::DeadSession(_)
                    | NodeId::Group(_)
                    | NodeId::Header(_) => return None,
                    NodeId::Recent(_) => unreachable!(),
                };
                let label = Self::truncate_confirmation_label(&label);
                Some(format!("Kill {}?\n[enter] confirm  [esc] cancel", label))
            }
        }
    }

    fn truncate_confirmation_label(label: &str) -> String {
        if label.chars().count() <= 24 {
            return label.to_string();
        }
        let prefix: String = label.chars().take(21).collect();
        format!("{}...", prefix)
    }

    pub fn handle_kill(&mut self) {
        self.start_kill();
    }

    pub fn handle_confirm_kill(&mut self) {
        self.confirm_kill();
    }

    pub fn handle_cancel_kill(&mut self) {
        if self.confirming_process.is_some() {
            self.confirming_process = None;
            self.mode = Mode::Monitor;
        } else {
            self.mode = Mode::Normal;
            self.confirming_kill_target = None;
        }
    }
}
