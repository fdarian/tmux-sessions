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
    fn sort_monitor_rows(rows: &mut [ProcessRow], sort: MonitorSort) {
        match sort {
            MonitorSort::Mem => rows.sort_by(|a, b| b.rss_kb.cmp(&a.rss_kb)),
            MonitorSort::Cpu => rows.sort_by(|a, b| {
                b.pcpu
                    .partial_cmp(&a.pcpu)
                    .unwrap_or(std::cmp::Ordering::Equal)
            }),
        }
    }

    fn reselect_monitor(&mut self, prev_pid: Option<u32>) {
        let new_index = prev_pid
            .and_then(|pid| self.monitor_rows.iter().position(|row| row.pid == pid))
            .unwrap_or_else(|| {
                self.monitor_selected
                    .min(self.monitor_rows.len().saturating_sub(1))
            });
        self.monitor_selected = new_index;
        self.monitor_list_state
            .select(if self.monitor_rows.is_empty() {
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
        let prev_pid = self
            .monitor_rows
            .get(self.monitor_selected)
            .map(|row| row.pid);
        let mut rows = procs::collect_process_rows()?;
        self.apply_session_display_names(&mut rows);
        Self::sort_monitor_rows(&mut rows, self.monitor_sort);
        self.monitor_rows = rows;
        self.reselect_monitor(prev_pid);
        Ok(())
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
        let prev_pid = self
            .monitor_rows
            .get(self.monitor_selected)
            .map(|row| row.pid);
        Self::sort_monitor_rows(&mut self.monitor_rows, self.monitor_sort);
        self.reselect_monitor(prev_pid);
    }

    pub fn handle_open_process_detail(&mut self) {
        if self.mode != Mode::Monitor {
            return;
        }
        if self.monitor_rows.get(self.monitor_selected).is_none() {
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
