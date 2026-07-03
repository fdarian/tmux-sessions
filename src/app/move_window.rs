use std::collections::HashSet;

use crate::app::App;
use crate::event::Mode;
use crate::tmux;

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

impl App {
    fn reset_move_window_state(&mut self) {
        self.move_query = String::new();
        self.move_cursor = 0;
        self.move_candidates.clear();
        self.move_selected = 0;
        self.move_source_session_cwd = String::new();
    }

    fn rebuild_move_candidates(&mut self) {
        let query_lc = self.move_query.to_lowercase();
        let mut source_session_ids = HashSet::new();
        for window_id in self.marked_windows.iter() {
            let source_window = self.windows.iter().find(|window| window.id == *window_id);
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

    pub fn handle_enter_move_window(&mut self) {
        let first_marked_window_id = match self.selected_window_ids().first() {
            Some(window_id) => window_id.clone(),
            None => return,
        };
        self.selecting = false;
        self.selection_anchor = None;
        let source_window = match self
            .windows
            .iter()
            .find(|window| window.id == first_marked_window_id)
        {
            Some(window) => window,
            None => return,
        };
        let source_session_cwd = match self
            .sessions
            .iter()
            .find(|session| session.id == source_window.session_id)
        {
            Some(session) => session.cwd.clone(),
            None => return,
        };
        self.reset_move_window_state();
        self.move_source_session_cwd = source_session_cwd;
        self.rebuild_move_candidates();
        self.mode = Mode::MoveWindow;
    }

    pub fn handle_move_window_char(&mut self, c: char) {
        let byte_offset = self
            .move_query
            .char_indices()
            .nth(self.move_cursor)
            .map(|(i, _)| i)
            .unwrap_or(self.move_query.len());
        self.move_query.insert(byte_offset, c);
        self.move_cursor += 1;
        self.rebuild_move_candidates();
        self.move_selected = 0;
    }

    pub fn handle_move_window_backspace(&mut self) {
        if self.move_cursor > 0 {
            let byte_before = self
                .move_query
                .char_indices()
                .nth(self.move_cursor - 1)
                .map(|(i, _)| i)
                .unwrap_or(self.move_query.len());
            let byte_at = self
                .move_query
                .char_indices()
                .nth(self.move_cursor)
                .map(|(i, _)| i)
                .unwrap_or(self.move_query.len());
            self.move_query.drain(byte_before..byte_at);
            self.move_cursor -= 1;
            self.rebuild_move_candidates();
            self.move_selected = 0;
        }
    }

    pub fn handle_move_window_delete_forward(&mut self) {
        let len = self.move_query.chars().count();
        if self.move_cursor < len {
            let byte_at = self
                .move_query
                .char_indices()
                .nth(self.move_cursor)
                .map(|(i, _)| i)
                .unwrap_or(self.move_query.len());
            let byte_next = self
                .move_query
                .char_indices()
                .nth(self.move_cursor + 1)
                .map(|(i, _)| i)
                .unwrap_or(self.move_query.len());
            self.move_query.drain(byte_at..byte_next);
            self.rebuild_move_candidates();
            self.move_selected = 0;
        }
    }

    pub fn handle_move_window_kill_word(&mut self) {
        let chars: Vec<char> = self.move_query.chars().collect();
        let mut pos = self.move_cursor;
        while pos > 0 && chars[pos - 1].is_whitespace() {
            pos -= 1;
        }
        while pos > 0 && !chars[pos - 1].is_whitespace() {
            pos -= 1;
        }
        let start_byte = self
            .move_query
            .char_indices()
            .nth(pos)
            .map(|(i, _)| i)
            .unwrap_or(self.move_query.len());
        let end_byte = self
            .move_query
            .char_indices()
            .nth(self.move_cursor)
            .map(|(i, _)| i)
            .unwrap_or(self.move_query.len());
        self.move_query.drain(start_byte..end_byte);
        self.move_cursor = pos;
        self.rebuild_move_candidates();
        self.move_selected = 0;
    }

    pub fn handle_move_window_kill_line(&mut self) {
        let byte_offset = self
            .move_query
            .char_indices()
            .nth(self.move_cursor)
            .map(|(i, _)| i)
            .unwrap_or(self.move_query.len());
        self.move_query.drain(..byte_offset);
        self.move_cursor = 0;
        self.rebuild_move_candidates();
        self.move_selected = 0;
    }

    pub fn handle_move_window_kill_line_forward(&mut self) {
        let byte_offset = self
            .move_query
            .char_indices()
            .nth(self.move_cursor)
            .map(|(i, _)| i)
            .unwrap_or(self.move_query.len());
        self.move_query.truncate(byte_offset);
        self.rebuild_move_candidates();
        self.move_selected = 0;
    }

    pub fn handle_move_window_cursor_left(&mut self) {
        if self.move_cursor > 0 {
            self.move_cursor -= 1;
        }
    }

    pub fn handle_move_window_cursor_right(&mut self) {
        let len = self.move_query.chars().count();
        if self.move_cursor < len {
            self.move_cursor += 1;
        }
    }

    pub fn handle_move_window_cursor_word_left(&mut self) {
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

    pub fn handle_move_window_cursor_word_right(&mut self) {
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

    pub fn handle_move_window_cursor_start(&mut self) {
        self.move_cursor = 0;
    }

    pub fn handle_move_window_cursor_end(&mut self) {
        self.move_cursor = self.move_query.chars().count();
    }

    pub fn handle_move_window_next(&mut self) {
        if self.move_selected + 1 < self.move_candidates.len() {
            self.move_selected += 1;
        }
    }

    pub fn handle_move_window_prev(&mut self) {
        if self.move_selected > 0 {
            self.move_selected -= 1;
        }
    }

    pub fn handle_confirm_move_window(&mut self) {
        let sources = self.selected_window_ids().to_vec();
        if sources.is_empty() {
            self.reset_move_window_state();
            self.mode = Mode::Normal;
            return;
        }
        let candidate = match self.move_candidates.get(self.move_selected).cloned() {
            Some(candidate) => candidate,
            None => return,
        };
        let target_session_id;
        let mut cleanup_window_id: Option<String> = None;
        let move_target = match candidate.target {
            MoveTarget::Existing(name) => {
                target_session_id = self
                    .sessions
                    .iter()
                    .find(|session| session.name == name)
                    .map(|session| session.id.clone());
                name
            }
            MoveTarget::Dead { name, cwd } => match tmux::new_session(&name, &cwd) {
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
            },
            MoveTarget::New { name, cwd } => match tmux::new_session(&name, &cwd) {
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
            },
        };
        let mut moved_any = false;
        for window_id in sources.iter() {
            let current_session_id = self
                .windows
                .iter()
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

    pub fn handle_cancel_move_window(&mut self) {
        self.selecting = false;
        self.selection_anchor = None;
        self.reset_move_window_state();
        self.mode = Mode::Normal;
    }
}
