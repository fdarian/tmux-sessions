use std::collections::HashSet;

use crate::app::App;
use crate::event::Mode;
use crate::tmux;
use crate::tree::NodeId;

impl App {
    pub(crate) fn selected_window_ids(&self) -> &[String] {
        self.marked_windows.as_slice()
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
        let hi = anchor
            .max(cursor)
            .min(self.flat_entries.len().saturating_sub(1));

        self.marked_windows.clear();
        let mut seen_window_ids = HashSet::new();
        for entry in self.flat_entries[lo..=hi].iter() {
            if let NodeId::Window(_, window_id) = entry.node_id.target() {
                if !seen_window_ids.insert(window_id.clone()) {
                    continue;
                }
                self.marked_windows.push(window_id.clone());
            }
        }
    }

    pub(crate) fn select_current(&mut self) {
        let i = match self.list_state.selected() {
            Some(i) if i < self.flat_entries.len() => i,
            _ => return,
        };

        let entry = &self.flat_entries[i];
        let node_id = &entry.node_id;
        let result = match node_id.target() {
            NodeId::Session(id) => tmux::switch_client(id),
            NodeId::Window(session_id, window_id) => {
                tmux::switch_client(session_id).and_then(|_| tmux::select_window(window_id))
            }
            NodeId::Pane(session_id, window_id, pane_id) => tmux::switch_client(session_id)
                .and_then(|_| tmux::select_window(window_id))
                .and_then(|_| tmux::select_pane(pane_id)),
            NodeId::Group(prefix) => match self.sessions.iter().find(|s| s.display_name == *prefix) {
                Some(peer) => tmux::switch_client(&peer.id),
                None => return,
            },
            NodeId::Separator(_) | NodeId::Header(_) => return,
            NodeId::DeadSession(name) => {
                let cwd = match self.dead_sessions.iter().find(|d| d.name == *name) {
                    Some(d) => d.cwd.clone(),
                    None => return,
                };
                tmux::new_session_with_actual_name(name, &cwd)
                    .and_then(|created_name| tmux::switch_client(&created_name))
            }
            NodeId::Recent(_) => unreachable!(),
        };

        if result.is_ok() {
            self.should_quit = true;
        }
    }

    pub fn handle_quit(&mut self) {
        self.should_quit = true;
    }

    pub fn handle_clear_marks_or_quit(&mut self) {
        if self.selecting || !self.marked_windows.is_empty() {
            self.marked_windows.clear();
            self.selecting = false;
            self.selection_anchor = None;
        } else if !self.filter_query.is_empty() {
            self.clear_filter();
        } else {
            self.should_quit = true;
        }
    }

    pub fn handle_move_up(&mut self) {
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
                if !crate::app::is_non_selectable(&self.flat_entries[target].node_id) {
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

    pub fn handle_move_down(&mut self) {
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
                if !crate::app::is_non_selectable(&self.flat_entries[target].node_id) {
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

    pub fn handle_collapse_or_parent(&mut self) {
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
                            if let Some(new_i) = self
                                .flat_entries
                                .iter()
                                .position(|e| e.node_id == parent_node_id)
                            {
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

    pub fn handle_expand_or_child(&mut self) {
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
                if i + 1 < self.flat_entries.len() && self.flat_entries[i + 1].depth > entry_depth {
                    self.list_state.select(Some(i + 1));
                }
            }
            self.update_preview();
        }
    }

    pub fn handle_toggle_mark_window(&mut self) {
        if !self.selecting {
            self.selecting = true;
            self.selection_anchor = self.list_state.selected();
            self.recompute_selection_range();
        } else {
            self.selecting = false;
            self.selection_anchor = None;
        }
    }

    pub fn handle_select_index(&mut self, i: usize) {
        if let Some(idx) = crate::app::resolve_shortcut_index(&self.flat_entries, i) {
            self.list_state.select(Some(idx));
            self.select_current();
        }
    }
}
