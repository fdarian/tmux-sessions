use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};

use crate::app::App;
use crate::event::Mode;
use crate::tmux;

const ADJECTIVES: &[&str] = &[
    "swift", "quiet", "brave", "calm", "eager", "fuzzy", "gentle", "happy", "icy", "jolly",
    "keen", "lively", "mellow", "nimble", "odd", "proud", "quick", "rusty", "sunny", "tidy",
    "vivid", "witty", "young", "zesty", "bold", "crisp", "dusty", "electric", "fluffy", "golden",
    "humble", "iron", "jumpy", "kind", "lucky", "misty", "noble", "plucky", "quirky", "rowdy",
];

const NOUNS: &[&str] = &[
    "otter", "falcon", "badger", "heron", "lynx", "panda", "raven", "sparrow", "tiger", "walrus",
    "beetle", "cobra", "dolphin", "eagle", "ferret", "gecko", "hawk", "ibis", "jaguar", "koala",
    "llama", "mantis", "newt", "orca", "puffin", "quail", "rabbit", "seal", "toad", "urchin",
    "viper", "wombat", "yak", "zebra", "cougar", "dingo", "egret", "finch", "gopher", "marten",
];

/// Generates a random `adjective-noun` slug (e.g. `swift-otter`) for sessions created
/// with an empty name. Seeded from `RandomState` to avoid pulling in a `rand` dependency;
/// collisions are harmless since `tmux::new_session_with_actual_name` returns the name
/// tmux actually assigned.
fn random_slug() -> String {
    let adjective_index = (RandomState::new().build_hasher().finish() as usize) % ADJECTIVES.len();
    let noun_index = (RandomState::new().build_hasher().finish() as usize) % NOUNS.len();
    format!("{}-{}", ADJECTIVES[adjective_index], NOUNS[noun_index])
}

impl App {
    pub fn handle_start_quick_create(&mut self) {
        self.quick_create_buffer = String::new();
        self.quick_create_cursor = 0;
        self.quick_create_error = None;
        self.mode = Mode::QuickCreate;
    }

    pub fn handle_quick_create_char(&mut self, c: char) {
        let byte_offset = self
            .quick_create_buffer
            .char_indices()
            .nth(self.quick_create_cursor)
            .map(|(i, _)| i)
            .unwrap_or(self.quick_create_buffer.len());
        self.quick_create_buffer.insert(byte_offset, c);
        self.quick_create_cursor += 1;
    }

    pub fn handle_quick_create_backspace(&mut self) {
        if self.quick_create_cursor > 0 {
            let byte_before = self
                .quick_create_buffer
                .char_indices()
                .nth(self.quick_create_cursor - 1)
                .map(|(i, _)| i)
                .unwrap_or(self.quick_create_buffer.len());
            let byte_at = self
                .quick_create_buffer
                .char_indices()
                .nth(self.quick_create_cursor)
                .map(|(i, _)| i)
                .unwrap_or(self.quick_create_buffer.len());
            self.quick_create_buffer.drain(byte_before..byte_at);
            self.quick_create_cursor -= 1;
        }
    }

    pub fn handle_quick_create_delete_forward(&mut self) {
        let len = self.quick_create_buffer.chars().count();
        if self.quick_create_cursor < len {
            let byte_at = self
                .quick_create_buffer
                .char_indices()
                .nth(self.quick_create_cursor)
                .map(|(i, _)| i)
                .unwrap_or(self.quick_create_buffer.len());
            let byte_next = self
                .quick_create_buffer
                .char_indices()
                .nth(self.quick_create_cursor + 1)
                .map(|(i, _)| i)
                .unwrap_or(self.quick_create_buffer.len());
            self.quick_create_buffer.drain(byte_at..byte_next);
        }
    }

    pub fn handle_quick_create_cursor_left(&mut self) {
        if self.quick_create_cursor > 0 {
            self.quick_create_cursor -= 1;
        }
    }

    pub fn handle_quick_create_cursor_right(&mut self) {
        let len = self.quick_create_buffer.chars().count();
        if self.quick_create_cursor < len {
            self.quick_create_cursor += 1;
        }
    }

    pub fn handle_quick_create_cursor_start(&mut self) {
        self.quick_create_cursor = 0;
    }

    pub fn handle_quick_create_cursor_end(&mut self) {
        self.quick_create_cursor = self.quick_create_buffer.chars().count();
    }

    /// On failure, stays in `Mode::QuickCreate` with the buffer intact and
    /// `quick_create_error` set, so the user sees why nothing happened and can
    /// retry or `Esc` out, mirroring how `create_load_error` surfaces failures
    /// in the `o` popup (src/ui/create.rs).
    pub fn handle_confirm_quick_create(&mut self) {
        let trimmed = self.quick_create_buffer.trim().to_string();
        let name = if trimmed.is_empty() { random_slug() } else { trimmed };

        let current_dir = match std::env::current_dir() {
            Ok(dir) => dir,
            Err(err) => {
                self.quick_create_error = Some(format!("failed to read cwd: {err}"));
                return;
            }
        };
        let cwd = match crate::app::path_buf_to_string(current_dir, "current directory") {
            Ok(path) => path,
            Err(err) => {
                self.quick_create_error = Some(err.to_string());
                return;
            }
        };

        let result = tmux::new_session_with_actual_name(&name, &cwd)
            .and_then(|created_name| tmux::switch_client(&created_name));
        match result {
            Ok(()) => {
                self.mode = Mode::Normal;
                self.quick_create_buffer = String::new();
                self.quick_create_cursor = 0;
                self.quick_create_error = None;
                self.should_quit = true;
            }
            Err(err) => {
                self.quick_create_error = Some(err.to_string());
            }
        }
    }

    pub fn handle_cancel_quick_create(&mut self) {
        self.mode = Mode::Normal;
        self.quick_create_buffer = String::new();
        self.quick_create_cursor = 0;
        self.quick_create_error = None;
    }
}

#[cfg(test)]
mod random_slug_tests {
    use super::random_slug;

    #[test]
    fn has_adjective_noun_shape() {
        let slug = random_slug();
        let parts: Vec<&str> = slug.split('-').collect();
        assert_eq!(parts.len(), 2, "expected `adjective-noun`, got {slug:?}");
        assert!(super::ADJECTIVES.contains(&parts[0]));
        assert!(super::NOUNS.contains(&parts[1]));
    }

    #[test]
    fn is_not_constant_across_calls() {
        let slugs: std::collections::HashSet<String> = (0..50).map(|_| random_slug()).collect();
        assert!(slugs.len() > 1, "expected variation across calls, got only {slugs:?}");
    }
}
