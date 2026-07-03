use crate::app::App;
use crate::event::Mode;
use crate::tmux;
use crate::tree::NodeId;

#[derive(Clone)]
pub enum RenameTarget {
    Session(String),
    Window(String),
}

impl App {
    pub fn handle_start_rename(&mut self) {
        let i = match self.list_state.selected() {
            Some(i) if i < self.flat_entries.len() => i,
            _ => return,
        };
        let (target, prefill) = match self.flat_entries[i].node_id.target() {
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
            NodeId::Group(_)
            | NodeId::Separator(_)
            | NodeId::DeadSession(_)
            | NodeId::Header(_) => return,
            NodeId::Recent(_) => unreachable!(),
        };
        self.renaming_target = Some(target);
        self.rename_cursor = prefill.chars().count();
        self.rename_buffer = prefill;
        self.mode = Mode::Renaming;
    }

    pub fn handle_rename_char(&mut self, c: char) {
        let byte_offset = self
            .rename_buffer
            .char_indices()
            .nth(self.rename_cursor)
            .map(|(i, _)| i)
            .unwrap_or(self.rename_buffer.len());
        self.rename_buffer.insert(byte_offset, c);
        self.rename_cursor += 1;
    }

    pub fn handle_rename_backspace(&mut self) {
        if self.rename_cursor > 0 {
            let byte_before = self
                .rename_buffer
                .char_indices()
                .nth(self.rename_cursor - 1)
                .map(|(i, _)| i)
                .unwrap_or(self.rename_buffer.len());
            let byte_at = self
                .rename_buffer
                .char_indices()
                .nth(self.rename_cursor)
                .map(|(i, _)| i)
                .unwrap_or(self.rename_buffer.len());
            self.rename_buffer.drain(byte_before..byte_at);
            self.rename_cursor -= 1;
        }
    }

    pub fn handle_rename_delete_forward(&mut self) {
        let len = self.rename_buffer.chars().count();
        if self.rename_cursor < len {
            let byte_at = self
                .rename_buffer
                .char_indices()
                .nth(self.rename_cursor)
                .map(|(i, _)| i)
                .unwrap_or(self.rename_buffer.len());
            let byte_next = self
                .rename_buffer
                .char_indices()
                .nth(self.rename_cursor + 1)
                .map(|(i, _)| i)
                .unwrap_or(self.rename_buffer.len());
            self.rename_buffer.drain(byte_at..byte_next);
        }
    }

    pub fn handle_rename_kill_word(&mut self) {
        let chars: Vec<char> = self.rename_buffer.chars().collect();
        let mut pos = self.rename_cursor;
        while pos > 0 && chars[pos - 1].is_whitespace() {
            pos -= 1;
        }
        while pos > 0 && !chars[pos - 1].is_whitespace() {
            pos -= 1;
        }
        let start_byte = self
            .rename_buffer
            .char_indices()
            .nth(pos)
            .map(|(i, _)| i)
            .unwrap_or(self.rename_buffer.len());
        let end_byte = self
            .rename_buffer
            .char_indices()
            .nth(self.rename_cursor)
            .map(|(i, _)| i)
            .unwrap_or(self.rename_buffer.len());
        self.rename_buffer.drain(start_byte..end_byte);
        self.rename_cursor = pos;
    }

    pub fn handle_rename_kill_line(&mut self) {
        let byte_offset = self
            .rename_buffer
            .char_indices()
            .nth(self.rename_cursor)
            .map(|(i, _)| i)
            .unwrap_or(self.rename_buffer.len());
        self.rename_buffer.drain(..byte_offset);
        self.rename_cursor = 0;
    }

    pub fn handle_rename_kill_line_forward(&mut self) {
        let byte_offset = self
            .rename_buffer
            .char_indices()
            .nth(self.rename_cursor)
            .map(|(i, _)| i)
            .unwrap_or(self.rename_buffer.len());
        self.rename_buffer.truncate(byte_offset);
    }

    pub fn handle_rename_cursor_left(&mut self) {
        if self.rename_cursor > 0 {
            self.rename_cursor -= 1;
        }
    }

    pub fn handle_rename_cursor_right(&mut self) {
        let len = self.rename_buffer.chars().count();
        if self.rename_cursor < len {
            self.rename_cursor += 1;
        }
    }

    pub fn handle_rename_cursor_word_left(&mut self) {
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

    pub fn handle_rename_cursor_word_right(&mut self) {
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

    pub fn handle_rename_cursor_start(&mut self) {
        self.rename_cursor = 0;
    }

    pub fn handle_rename_cursor_end(&mut self) {
        self.rename_cursor = self.rename_buffer.chars().count();
    }

    pub fn handle_confirm_rename(&mut self) {
        let target = match self.renaming_target.clone() {
            Some(target) => target,
            None => {
                self.mode = Mode::Normal;
                return;
            }
        };
        let trimmed = self.rename_buffer.trim().to_string();
        let current_name = match &target {
            RenameTarget::Session(id) => self
                .sessions
                .iter()
                .find(|s| s.id == *id)
                .map(|s| s.name.clone()),
            RenameTarget::Window(id) => self
                .windows
                .iter()
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

    pub fn handle_cancel_rename(&mut self) {
        self.mode = Mode::Normal;
        self.renaming_target = None;
        self.rename_buffer = String::new();
        self.rename_cursor = 0;
    }
}
