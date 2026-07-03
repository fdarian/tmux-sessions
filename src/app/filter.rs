use crate::app::App;
use crate::event::Mode;

impl App {
    pub(crate) fn clear_filter(&mut self) {
        let selected_node_id = self
            .list_state
            .selected()
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

    pub fn handle_enter_filter(&mut self) {
        self.mode = Mode::Filtering;
        self.filter_cursor = self.filter_query.chars().count();
        self.update_preview();
    }

    pub fn handle_detach_filter(&mut self) {
        self.mode = Mode::Normal;
    }

    pub fn handle_filter_char(&mut self, c: char) {
        let byte_offset = self
            .filter_query
            .char_indices()
            .nth(self.filter_cursor)
            .map(|(i, _)| i)
            .unwrap_or(self.filter_query.len());
        self.filter_query.insert(byte_offset, c);
        self.filter_cursor += 1;
        self.rebuild_flat_entries();
        self.list_state.select(Some(0));
        self.update_preview();
    }

    pub fn handle_filter_backspace(&mut self) {
        if self.filter_cursor > 0 {
            let byte_before = self
                .filter_query
                .char_indices()
                .nth(self.filter_cursor - 1)
                .map(|(i, _)| i)
                .unwrap_or(self.filter_query.len());
            let byte_at = self
                .filter_query
                .char_indices()
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

    pub fn handle_filter_delete_forward(&mut self) {
        let len = self.filter_query.chars().count();
        if self.filter_cursor < len {
            let byte_at = self
                .filter_query
                .char_indices()
                .nth(self.filter_cursor)
                .map(|(i, _)| i)
                .unwrap_or(self.filter_query.len());
            let byte_next = self
                .filter_query
                .char_indices()
                .nth(self.filter_cursor + 1)
                .map(|(i, _)| i)
                .unwrap_or(self.filter_query.len());
            self.filter_query.drain(byte_at..byte_next);
            self.rebuild_flat_entries();
            self.list_state.select(Some(0));
            self.update_preview();
        }
    }

    pub fn handle_filter_kill_word(&mut self) {
        let chars: Vec<char> = self.filter_query.chars().collect();
        let mut pos = self.filter_cursor;
        while pos > 0 && chars[pos - 1].is_whitespace() {
            pos -= 1;
        }
        while pos > 0 && !chars[pos - 1].is_whitespace() {
            pos -= 1;
        }
        let start_byte = self
            .filter_query
            .char_indices()
            .nth(pos)
            .map(|(i, _)| i)
            .unwrap_or(self.filter_query.len());
        let end_byte = self
            .filter_query
            .char_indices()
            .nth(self.filter_cursor)
            .map(|(i, _)| i)
            .unwrap_or(self.filter_query.len());
        self.filter_query.drain(start_byte..end_byte);
        self.filter_cursor = pos;
        self.rebuild_flat_entries();
        self.list_state.select(Some(0));
        self.update_preview();
    }

    pub fn handle_filter_kill_line(&mut self) {
        let byte_offset = self
            .filter_query
            .char_indices()
            .nth(self.filter_cursor)
            .map(|(i, _)| i)
            .unwrap_or(self.filter_query.len());
        self.filter_query.drain(..byte_offset);
        self.filter_cursor = 0;
        self.rebuild_flat_entries();
        self.list_state.select(Some(0));
        self.update_preview();
    }

    pub fn handle_filter_kill_line_forward(&mut self) {
        let byte_offset = self
            .filter_query
            .char_indices()
            .nth(self.filter_cursor)
            .map(|(i, _)| i)
            .unwrap_or(self.filter_query.len());
        self.filter_query.truncate(byte_offset);
        self.rebuild_flat_entries();
        self.list_state.select(Some(0));
        self.update_preview();
    }

    pub fn handle_filter_cursor_left(&mut self) {
        if self.filter_cursor > 0 {
            self.filter_cursor -= 1;
        }
    }

    pub fn handle_filter_cursor_right(&mut self) {
        let len = self.filter_query.chars().count();
        if self.filter_cursor < len {
            self.filter_cursor += 1;
        }
    }

    pub fn handle_filter_cursor_word_left(&mut self) {
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

    pub fn handle_filter_cursor_word_right(&mut self) {
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

    pub fn handle_filter_cursor_start(&mut self) {
        self.filter_cursor = 0;
    }

    pub fn handle_filter_cursor_end(&mut self) {
        self.filter_cursor = self.filter_query.chars().count();
    }

    pub fn handle_exit_filter(&mut self) {
        self.clear_filter();
    }
}
