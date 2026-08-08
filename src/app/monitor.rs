use std::collections::HashMap;
use std::io;

use crate::app::App;
use crate::event::Mode;
use crate::procs::{self, ProcessRow};
use crate::tmux;

#[derive(Clone, Copy, PartialEq)]
pub enum MonitorSort {
    Mem,
    Cpu,
}

impl App {
    fn rebuild_monitor_entries(&mut self) {
        self.monitor_entries =
            procs::flatten_process_tree(&self.monitor_rows, &self.monitor_collapsed, self.monitor_sort);
    }

    fn reselect_monitor(&mut self, prev_pid: Option<u32>) {
        let new_index = prev_pid
            .and_then(|pid| self.monitor_entries.iter().position(|entry| entry.pid == pid))
            .unwrap_or_else(|| {
                self.monitor_selected
                    .min(self.monitor_entries.len().saturating_sub(1))
            });
        self.monitor_selected = new_index;
        self.monitor_list_state
            .select(if self.monitor_entries.is_empty() {
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

    /// Resolves the currently-selected tree row back to its underlying
    /// `ProcessRow`. `monitor_selected` indexes into `monitor_entries`
    /// (the flattened tree), not `monitor_rows` directly.
    pub fn monitor_selected_row(&self) -> Option<&ProcessRow> {
        let entry = self.monitor_entries.get(self.monitor_selected)?;
        self.monitor_rows.iter().find(|row| row.pid == entry.pid)
    }

    pub fn refresh_monitor(&mut self) -> io::Result<()> {
        let prev_pid = self.monitor_entries.get(self.monitor_selected).map(|e| e.pid);
        let mut rows = procs::collect_process_rows()?;
        self.apply_session_display_names(&mut rows);
        self.monitor_rows = rows;
        self.rebuild_monitor_entries();
        self.reselect_monitor(prev_pid);
        Ok(())
    }

    fn select_monitor_process(&mut self) {
        let row = match self.monitor_selected_row() {
            Some(row) => row,
            None => return,
        };
        let result = tmux::switch_client(&row.pane.session_name)
            .and_then(|_| tmux::select_pane(&row.pane.pane_id));
        if result.is_ok() {
            self.should_quit = true;
        }
    }

    /// `h` in monitor mode: collapses the selected node if it's expanded,
    /// otherwise collapses the nearest ancestor and jumps to it. Mirrors
    /// `handle_collapse_or_parent`'s semantics for the session tree.
    pub fn handle_monitor_collapse_or_parent(&mut self) {
        let i = self.monitor_selected;
        let (pid, has_children, depth) = match self.monitor_entries.get(i) {
            Some(entry) => (entry.pid, entry.has_children, entry.depth),
            None => return,
        };

        if has_children && !self.monitor_collapsed.contains(&pid) {
            self.monitor_collapsed.insert(pid);
            self.rebuild_monitor_entries();
            if let Some(new_i) = self.monitor_entries.iter().position(|e| e.pid == pid) {
                self.monitor_selected = new_i;
                self.monitor_list_state.select(Some(new_i));
            }
            return;
        }

        if depth == 0 {
            return;
        }
        for j in (0..i).rev() {
            if self.monitor_entries[j].depth < depth {
                let parent_pid = self.monitor_entries[j].pid;
                self.monitor_collapsed.insert(parent_pid);
                self.rebuild_monitor_entries();
                if let Some(new_i) = self.monitor_entries.iter().position(|e| e.pid == parent_pid) {
                    self.monitor_selected = new_i;
                    self.monitor_list_state.select(Some(new_i));
                }
                break;
            }
        }
    }

    /// `l` in monitor mode: expands the selected node if collapsed, then
    /// descends to its first child. Mirrors `handle_expand_or_child`'s
    /// semantics for the session tree.
    pub fn handle_monitor_expand_or_child(&mut self) {
        let i = self.monitor_selected;
        let (pid, has_children, depth) = match self.monitor_entries.get(i) {
            Some(entry) => (entry.pid, entry.has_children, entry.depth),
            None => return,
        };
        if !has_children {
            return;
        }

        if self.monitor_collapsed.contains(&pid) {
            self.monitor_collapsed.remove(&pid);
            self.rebuild_monitor_entries();
        }

        if i + 1 < self.monitor_entries.len() && self.monitor_entries[i + 1].depth > depth {
            self.monitor_selected = i + 1;
            self.monitor_list_state.select(Some(i + 1));
        }
    }

    pub fn handle_enter_monitor(&mut self) {
        self.mode = Mode::Monitor;
        let _ = self.refresh_monitor();
    }

    pub fn handle_exit_monitor(&mut self) {
        self.mode = Mode::Normal;
    }

    pub fn handle_toggle_monitor_sort(&mut self) {
        if self.mode != Mode::Monitor {
            return;
        }
        self.monitor_sort = match self.monitor_sort {
            MonitorSort::Mem => MonitorSort::Cpu,
            MonitorSort::Cpu => MonitorSort::Mem,
        };
        let prev_pid = self.monitor_entries.get(self.monitor_selected).map(|e| e.pid);
        self.rebuild_monitor_entries();
        self.reselect_monitor(prev_pid);
    }

    pub fn handle_open_process_detail(&mut self) {
        if self.mode != Mode::Monitor {
            return;
        }
        if self.monitor_selected_row().is_none() {
            return;
        }
        self.mode = Mode::ProcessDetail;
    }

    pub fn handle_close_process_detail(&mut self) {
        self.mode = Mode::Monitor;
    }

    pub fn handle_tick(&mut self) {
        if self.mode == Mode::Monitor {
            let _ = self.refresh_monitor();
        }
    }

    pub fn handle_select_dispatch(&mut self) {
        if self.mode == Mode::Monitor {
            self.select_monitor_process();
        } else {
            self.select_current();
        }
    }
}
