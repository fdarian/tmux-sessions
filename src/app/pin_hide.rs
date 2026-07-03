use crate::app::App;

impl App {
    fn move_pin(&mut self, direction: i8) {
        let i = match self.list_state.selected() {
            Some(i) if i < self.flat_entries.len() => i,
            _ => return,
        };
        let session_id = match crate::app::extract_session_id(&self.flat_entries[i].node_id) {
            Some(session_id) => session_id.clone(),
            None => return,
        };
        let session_name = match self.sessions.iter().find(|s| s.id == session_id) {
            Some(s) => s.name.clone(),
            None => return,
        };
        let pos = match self.pinned.iter().position(|p| *p == session_name) {
            Some(p) => p,
            None => return,
        };
        let new_pos = match direction {
            -1 if pos > 0 => pos - 1,
            1 if pos + 1 < self.pinned.len() => pos + 1,
            _ => return,
        };
        self.pinned.swap(pos, new_pos);
        crate::app::save_pins(&self.pinned);
        let current_node_id = self.flat_entries[i].node_id.clone();
        self.rebuild_flat_entries();
        if let Some(new_i) = self
            .flat_entries
            .iter()
            .position(|e| e.node_id == current_node_id)
        {
            self.list_state.select(Some(new_i));
        }
        self.update_preview();
    }

    pub fn handle_toggle_pin(&mut self) {
        let i = match self.list_state.selected() {
            Some(i) if i < self.flat_entries.len() => i,
            _ => return,
        };
        let session_id = match crate::app::extract_session_id(&self.flat_entries[i].node_id) {
            Some(session_id) => session_id.clone(),
            None => return,
        };
        let session_name = match self.sessions.iter().find(|s| s.id == session_id) {
            Some(s) => s.name.clone(),
            None => return,
        };
        if self.pinned.contains(&session_name) {
            self.pinned.retain(|p| *p != session_name);
        } else {
            self.pinned.push(session_name);
        }
        crate::app::save_pins(&self.pinned);
        let current_node_id = self.flat_entries[i].node_id.clone();
        self.rebuild_flat_entries();
        if let Some(new_i) = self
            .flat_entries
            .iter()
            .position(|e| e.node_id == current_node_id)
        {
            self.list_state.select(Some(new_i));
        }
        self.update_preview();
    }

    pub fn handle_toggle_hide(&mut self) {
        let i = match self.list_state.selected() {
            Some(i) if i < self.flat_entries.len() => i,
            _ => return,
        };
        let session_id = match crate::app::extract_session_id(&self.flat_entries[i].node_id) {
            Some(session_id) => session_id.clone(),
            None => return,
        };
        let session_name = match self.sessions.iter().find(|s| s.id == session_id) {
            Some(s) => s.name.clone(),
            None => return,
        };
        if self.hidden.contains(&session_name) {
            self.hidden.retain(|h| *h != session_name);
        } else {
            self.hidden.push(session_name);
        }
        crate::app::save_hidden(&self.hidden);
        let current_node_id = self.flat_entries[i].node_id.clone();
        self.rebuild_flat_entries();
        if let Some(new_i) = self
            .flat_entries
            .iter()
            .position(|e| e.node_id == current_node_id)
        {
            self.list_state.select(Some(new_i));
        } else if self.flat_entries.is_empty() {
            self.list_state.select(None);
        } else {
            let clamped = i.min(self.flat_entries.len() - 1);
            self.list_state.select(Some(clamped));
        }
        self.update_preview();
    }

    pub fn handle_toggle_show_hidden(&mut self) {
        let (current_node_id, i) = match self.list_state.selected() {
            Some(i) if i < self.flat_entries.len() => {
                (Some(self.flat_entries[i].node_id.clone()), i)
            }
            _ => (None, 0),
        };
        self.show_hidden = !self.show_hidden;
        self.rebuild_flat_entries();
        if let Some(node_id) = current_node_id {
            if let Some(new_i) = self.flat_entries.iter().position(|e| e.node_id == node_id) {
                self.list_state.select(Some(new_i));
            } else if self.flat_entries.is_empty() {
                self.list_state.select(None);
            } else {
                let clamped = i.min(self.flat_entries.len() - 1);
                self.list_state.select(Some(clamped));
            }
        }
        self.update_preview();
    }

    pub fn handle_move_pin_up(&mut self) {
        self.move_pin(-1);
    }

    pub fn handle_move_pin_down(&mut self) {
        self.move_pin(1);
    }
}
