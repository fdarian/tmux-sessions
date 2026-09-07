use std::path::PathBuf;

use fuzzy_matcher::skim::SkimMatcherV2;

use crate::app::App;
use crate::create::{self, CreateCandidate, CreateTab, CreateTarget};
use crate::event::Mode;
use crate::tmux;
use crate::tree::NodeId;

/// Sent to the worktree worker thread to run the configured `worktree_create_command`
/// off the UI thread. Mirrors `CaptureRequest`/`FormatRequest`'s request/response shape.
pub struct WorktreeCreateRequest {
    pub generation: u64,
    pub command: String,
    pub branch: String,
    pub cwd: PathBuf,
}

impl App {
    /// cwd for the currently highlighted tree row, if it resolves to a live session
    /// (via `session_for_node`) or a dead session (via its own recorded cwd).
    fn selected_create_cwd(&self) -> Option<String> {
        let index = self.list_state.selected()?;
        let node_id = &self.flat_entries.get(index)?.node_id;
        if let NodeId::DeadSession(name) = node_id.target() {
            return self
                .dead_sessions
                .iter()
                .find(|dead_session| dead_session.name == *name)
                .map(|dead_session| dead_session.cwd.clone());
        }
        self.session_for_node(node_id)
            .map(|session| session.cwd.clone())
    }

    fn reset_create_state(&mut self) {
        self.create_query = String::new();
        self.create_cursor = 0;
        self.create_tab = CreateTab::History;
        self.create_available_tabs.clear();
        self.create_candidates.clear();
        self.create_selected = 0;
        self.create_worktrees.clear();
        self.create_zoxide_entries.clear();
        self.create_current_session_cwd = String::new();
        self.create_load_error = None;
        self.create_worktree_branch = None;
    }

    pub(crate) fn rebuild_create_candidates(&mut self) {
        let matcher = SkimMatcherV2::default();
        let mut candidates = Vec::new();

        match self.create_tab {
            CreateTab::History => {
                let mut scored_dead_sessions: Vec<(i64, u64, usize, Vec<usize>)> = Vec::new();
                for index in 0..self.dead_sessions.len() {
                    let dead_session = &self.dead_sessions[index];
                    if let Some((score, match_indices)) = crate::app::create_match_result(
                        &matcher,
                        &self.create_query,
                        &dead_session.display_name,
                    ) {
                        scored_dead_sessions.push((
                            score,
                            dead_session.last_seen,
                            index,
                            match_indices,
                        ));
                    }
                }
                scored_dead_sessions.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));

                for entry in scored_dead_sessions.into_iter() {
                    let dead_session = &self.dead_sessions[entry.2];
                    let secondary = if dead_session.name != dead_session.display_name {
                        Some(dead_session.name.clone())
                    } else {
                        None
                    };
                    candidates.push(CreateCandidate {
                        primary: dead_session.display_name.clone(),
                        secondary,
                        match_indices: entry.3,
                        frecency: None,
                        target: CreateTarget::ResumeDead {
                            name: dead_session.name.clone(),
                            cwd: dead_session.cwd.clone(),
                        },
                    });
                }

                if !self.create_query.is_empty()
                    && !self
                        .dead_sessions
                        .iter()
                        .any(|dead_session| dead_session.name == self.create_query)
                {
                    let primary = format!("+ Create new session \"{}\"", self.create_query);
                    if let Some(create_match) =
                        crate::app::create_match_result(&matcher, &self.create_query, &primary)
                    {
                        candidates.push(CreateCandidate {
                            primary,
                            secondary: None,
                            match_indices: create_match.1,
                            frecency: None,
                            target: CreateTarget::NewNamed {
                                name: self.create_query.clone(),
                                cwd: self.create_current_session_cwd.clone(),
                            },
                        });
                    }
                }
            }
            CreateTab::Worktree => {
                let mut scored_worktrees: Vec<(i64, usize, Vec<usize>)> = Vec::new();
                for index in 0..self.create_worktrees.len() {
                    let worktree = &self.create_worktrees[index];
                    if let Some((score, match_indices)) = crate::app::create_match_result(
                        &matcher,
                        &self.create_query,
                        &worktree.branch,
                    ) {
                        scored_worktrees.push((score, index, match_indices));
                    }
                }
                if !self.create_query.is_empty() {
                    scored_worktrees.sort_by_key(|entry| std::cmp::Reverse(entry.0));
                }

                for entry in scored_worktrees.into_iter() {
                    let worktree = &self.create_worktrees[entry.1];
                    candidates.push(CreateCandidate {
                        primary: worktree.branch.clone(),
                        secondary: Some(worktree.path.clone()),
                        match_indices: entry.2,
                        frecency: None,
                        target: CreateTarget::PathDir {
                            path: worktree.path.clone(),
                        },
                    });
                }

                let worktree_create_command = self
                    .config
                    .as_ref()
                    .and_then(|config| config.worktree_create_command.as_ref());
                if worktree_create_command.is_some()
                    && !self.create_query.is_empty()
                    && !self
                        .create_worktrees
                        .iter()
                        .any(|w| w.branch == self.create_query)
                {
                    let primary = format!("+ Create worktree \"{}\"", self.create_query);
                    let synthetic_match =
                        crate::app::create_match_result(&matcher, &self.create_query, &primary);
                    let match_indices = synthetic_match.map(|m| m.1).unwrap_or_default();
                    candidates.push(CreateCandidate {
                        primary,
                        secondary: None,
                        match_indices,
                        frecency: None,
                        target: CreateTarget::NewWorktree {
                            branch: self.create_query.clone(),
                        },
                    });
                }
            }
            CreateTab::Zoxide => {
                let mut scored_paths: Vec<(i64, usize, Vec<usize>)> = Vec::new();
                for index in 0..self.create_zoxide_entries.len() {
                    let entry = &self.create_zoxide_entries[index];
                    if let Some((score, match_indices)) =
                        crate::app::create_match_result(&matcher, &self.create_query, &entry.path)
                    {
                        scored_paths.push((score, index, match_indices));
                    }
                }
                if !self.create_query.is_empty() {
                    scored_paths.sort_by_key(|entry| std::cmp::Reverse(entry.0));
                }

                for entry in scored_paths.into_iter() {
                    let zoxide_entry = &self.create_zoxide_entries[entry.1];
                    candidates.push(CreateCandidate {
                        primary: zoxide_entry.path.clone(),
                        secondary: None,
                        match_indices: entry.2,
                        frecency: Some(zoxide_entry.frecency),
                        target: CreateTarget::PathDir {
                            path: zoxide_entry.path.clone(),
                        },
                    });
                }
            }
        }

        self.create_candidates = candidates;
        self.create_selected = 0;
    }

    pub fn create_total_count(&self) -> usize {
        match self.create_tab {
            CreateTab::History => {
                let mut total = self.dead_sessions.len();
                if !self.create_query.is_empty()
                    && !self
                        .dead_sessions
                        .iter()
                        .any(|dead_session| dead_session.name == self.create_query)
                {
                    total += 1;
                }
                total
            }
            CreateTab::Worktree => {
                let mut total = self.create_worktrees.len();
                let has_create_command = self
                    .config
                    .as_ref()
                    .and_then(|config| config.worktree_create_command.as_ref())
                    .is_some();
                if has_create_command
                    && !self.create_query.is_empty()
                    && !self
                        .create_worktrees
                        .iter()
                        .any(|w| w.branch == self.create_query)
                {
                    total += 1;
                }
                total
            }
            CreateTab::Zoxide => self.create_zoxide_entries.len(),
        }
    }

    fn cycle_create_tab(&mut self, forward: bool) {
        if self.create_available_tabs.is_empty() {
            return;
        }

        let current_index = match self
            .create_available_tabs
            .iter()
            .position(|tab| *tab == self.create_tab)
        {
            Some(index) => index,
            None => return,
        };
        let next_index = if forward {
            (current_index + 1) % self.create_available_tabs.len()
        } else if current_index == 0 {
            self.create_available_tabs.len() - 1
        } else {
            current_index - 1
        };
        self.create_tab = self.create_available_tabs[next_index];
        self.rebuild_create_candidates();
    }

    pub fn handle_enter_create(&mut self) {
        let cwd_string = match self.selected_create_cwd().or_else(|| {
            std::env::current_dir()
                .ok()
                .and_then(|dir| crate::app::path_buf_to_string(dir, "current directory").ok())
        }) {
            Some(cwd) => cwd,
            None => return,
        };
        let current_dir = std::path::PathBuf::from(&cwd_string);

        self.reset_create_state();
        self.create_available_tabs.push(CreateTab::History);

        let mut load_errors: Vec<String> = Vec::new();

        let worktree_create_command = self
            .config
            .as_ref()
            .and_then(|config| config.worktree_create_command.as_deref());
        let worktree_result = if worktree_create_command.is_some() {
            create::list_worktrees_if_git(&current_dir)
        } else {
            create::list_linked_worktree_paths(&current_dir)
        };
        match worktree_result {
            Ok(Some(worktrees)) => {
                self.create_worktrees = worktrees;
                self.create_available_tabs.push(CreateTab::Worktree);
            }
            Ok(None) => {}
            Err(e) => {
                load_errors.push(format!("worktree: {e}"));
            }
        }

        let zoxide_enabled = self
            .config
            .as_ref()
            .and_then(|config| config.zoxide)
            .unwrap_or_default();
        if zoxide_enabled {
            match create::list_zoxide_dirs() {
                Ok(Some(entries)) => {
                    self.create_zoxide_entries = entries;
                    self.create_available_tabs.push(CreateTab::Zoxide);
                }
                Ok(None) => {}
                Err(e) => {
                    load_errors.push(format!("zoxide: {e}"));
                }
            }
        }

        if !load_errors.is_empty() {
            self.create_load_error = Some(load_errors.join("  "));
        }

        self.create_current_session_cwd = cwd_string;
        self.create_tab = self.create_available_tabs[0];
        self.rebuild_create_candidates();
        self.mode = Mode::CreateSession;
    }

    pub fn handle_create_char(&mut self, c: char) {
        let byte_offset = self
            .create_query
            .char_indices()
            .nth(self.create_cursor)
            .map(|(i, _)| i)
            .unwrap_or(self.create_query.len());
        self.create_query.insert(byte_offset, c);
        self.create_cursor += 1;
        self.rebuild_create_candidates();
    }

    pub fn handle_create_backspace(&mut self) {
        if self.create_cursor > 0 {
            let byte_before = self
                .create_query
                .char_indices()
                .nth(self.create_cursor - 1)
                .map(|(i, _)| i)
                .unwrap_or(self.create_query.len());
            let byte_at = self
                .create_query
                .char_indices()
                .nth(self.create_cursor)
                .map(|(i, _)| i)
                .unwrap_or(self.create_query.len());
            self.create_query.drain(byte_before..byte_at);
            self.create_cursor -= 1;
            self.rebuild_create_candidates();
        }
    }

    pub fn handle_create_delete_forward(&mut self) {
        let len = self.create_query.chars().count();
        if self.create_cursor < len {
            let byte_at = self
                .create_query
                .char_indices()
                .nth(self.create_cursor)
                .map(|(i, _)| i)
                .unwrap_or(self.create_query.len());
            let byte_next = self
                .create_query
                .char_indices()
                .nth(self.create_cursor + 1)
                .map(|(i, _)| i)
                .unwrap_or(self.create_query.len());
            self.create_query.drain(byte_at..byte_next);
            self.rebuild_create_candidates();
        }
    }

    pub fn handle_create_kill_word(&mut self) {
        let chars: Vec<char> = self.create_query.chars().collect();
        let mut pos = self.create_cursor;
        while pos > 0 && chars[pos - 1].is_whitespace() {
            pos -= 1;
        }
        while pos > 0 && !chars[pos - 1].is_whitespace() {
            pos -= 1;
        }
        let start_byte = self
            .create_query
            .char_indices()
            .nth(pos)
            .map(|(i, _)| i)
            .unwrap_or(self.create_query.len());
        let end_byte = self
            .create_query
            .char_indices()
            .nth(self.create_cursor)
            .map(|(i, _)| i)
            .unwrap_or(self.create_query.len());
        self.create_query.drain(start_byte..end_byte);
        self.create_cursor = pos;
        self.rebuild_create_candidates();
    }

    pub fn handle_create_kill_line(&mut self) {
        let byte_offset = self
            .create_query
            .char_indices()
            .nth(self.create_cursor)
            .map(|(i, _)| i)
            .unwrap_or(self.create_query.len());
        self.create_query.drain(..byte_offset);
        self.create_cursor = 0;
        self.rebuild_create_candidates();
    }

    pub fn handle_create_kill_line_forward(&mut self) {
        let byte_offset = self
            .create_query
            .char_indices()
            .nth(self.create_cursor)
            .map(|(i, _)| i)
            .unwrap_or(self.create_query.len());
        self.create_query.truncate(byte_offset);
        self.rebuild_create_candidates();
    }

    pub fn handle_create_cursor_left(&mut self) {
        if self.create_cursor > 0 {
            self.create_cursor -= 1;
        }
    }

    pub fn handle_create_cursor_right(&mut self) {
        let len = self.create_query.chars().count();
        if self.create_cursor < len {
            self.create_cursor += 1;
        }
    }

    pub fn handle_create_cursor_word_left(&mut self) {
        let chars: Vec<char> = self.create_query.chars().collect();
        let mut pos = self.create_cursor;
        while pos > 0 && chars[pos - 1].is_whitespace() {
            pos -= 1;
        }
        while pos > 0 && !chars[pos - 1].is_whitespace() {
            pos -= 1;
        }
        self.create_cursor = pos;
    }

    pub fn handle_create_cursor_word_right(&mut self) {
        let chars: Vec<char> = self.create_query.chars().collect();
        let len = chars.len();
        let mut pos = self.create_cursor;
        while pos < len && !chars[pos].is_whitespace() {
            pos += 1;
        }
        while pos < len && chars[pos].is_whitespace() {
            pos += 1;
        }
        self.create_cursor = pos;
    }

    pub fn handle_create_cursor_start(&mut self) {
        self.create_cursor = 0;
    }

    pub fn handle_create_cursor_end(&mut self) {
        self.create_cursor = self.create_query.chars().count();
    }

    pub fn handle_create_next(&mut self) {
        if self.create_selected + 1 < self.create_candidates.len() {
            self.create_selected += 1;
        }
    }

    pub fn handle_create_prev(&mut self) {
        if self.create_selected > 0 {
            self.create_selected -= 1;
        }
    }

    pub fn handle_create_tab_next(&mut self) {
        self.cycle_create_tab(true);
    }

    pub fn handle_create_tab_prev(&mut self) {
        self.cycle_create_tab(false);
    }

    pub fn handle_confirm_create(&mut self) {
        let candidate = match self.create_candidates.get(self.create_selected).cloned() {
            Some(candidate) => candidate,
            None => return,
        };

        if let CreateTarget::NewWorktree { branch } = candidate.target {
            let command = match self
                .config
                .as_ref()
                .and_then(|config| config.worktree_create_command.as_deref())
            {
                Some(cmd) => cmd.to_string(),
                None => {
                    self.reset_create_state();
                    self.mode = Mode::Normal;
                    return;
                }
            };
            self.create_worktree_generation = self.create_worktree_generation.wrapping_add(1);
            self.create_worktree_branch = Some(branch.clone());
            self.create_load_error = None;
            self.pending_worktree_create_request = Some(WorktreeCreateRequest {
                generation: self.create_worktree_generation,
                command,
                branch,
                cwd: PathBuf::from(&self.create_current_session_cwd),
            });
            self.mode = Mode::CreatingWorktree;
            return;
        }

        // `PathDir` candidates name the session after its full path, but tmux sanitizes `.`
        // to `_` in session names created some other way (e.g. `wt switch`), so a name-based
        // lookup can miss a live session that already owns this directory. Look it up by cwd
        // instead; the other targets keep matching by name since they're not path-derived.
        let live_session_id = match &candidate.target {
            CreateTarget::PathDir { path } => {
                self.session_for_cwd(path).map(|session| session.id.clone())
            }
            CreateTarget::ResumeDead { name, .. } | CreateTarget::NewNamed { name, .. } => self
                .sessions
                .iter()
                .find(|session| session.name == *name)
                .map(|session| session.id.clone()),
            CreateTarget::NewWorktree { .. } => unreachable!(),
        };

        let (name, cwd) = match candidate.target {
            CreateTarget::ResumeDead { name, cwd } => (name, cwd),
            CreateTarget::NewNamed { name, cwd } => (name, cwd),
            CreateTarget::PathDir { path } => (path.clone(), path),
            CreateTarget::NewWorktree { .. } => unreachable!(),
        };

        let result = match live_session_id {
            Some(id) => tmux::switch_client(&id),
            None => tmux::new_session(&name, &cwd)
                .and_then(|created| tmux::switch_client(&created.session_id)),
        };

        if result.is_ok() {
            self.should_quit = true;
        } else {
            self.reset_create_state();
            self.mode = Mode::Normal;
        }
    }

    pub fn handle_cancel_create(&mut self) {
        self.reset_create_state();
        self.mode = Mode::Normal;
    }

    /// Hands the queued worktree-create request (set by `handle_confirm_create`'s
    /// `NewWorktree` arm) to the worker thread. Called from the main loop right after
    /// `handle_action`, mirroring `take_pending_capture_request`.
    pub fn take_pending_worktree_create_request(&mut self) -> Option<WorktreeCreateRequest> {
        self.pending_worktree_create_request.take()
    }

    /// Applies the worktree worker's result. Always refreshes the session list first so the
    /// tree reflects reality whether the command succeeded or failed. `generation` guards
    /// against a stale result from a request superseded by a later one; the mode check guards
    /// against a result arriving after the user already backed out with `Esc` — in either case
    /// we still refresh but must not switch or quit.
    pub fn apply_worktree_create_result(
        &mut self,
        generation: u64,
        branch: &str,
        result: Result<PathBuf, String>,
    ) {
        let _ = self.refresh();

        if self.mode != Mode::CreatingWorktree || generation != self.create_worktree_generation {
            return;
        }

        let worktree_path = match result {
            Ok(worktree_path) => worktree_path,
            Err(message) => {
                self.create_load_error = Some(format!("worktree {branch:?}: {message}"));
                self.create_worktree_branch = None;
                self.mode = Mode::CreateSession;
                return;
            }
        };

        let cwd_str = match crate::app::path_buf_to_string(worktree_path, "worktree path") {
            Ok(cwd_str) => cwd_str,
            Err(err) => {
                self.create_load_error = Some(format!("worktree {branch:?}: {err}"));
                self.create_worktree_branch = None;
                self.mode = Mode::CreateSession;
                return;
            }
        };

        // `wt switch -y -c <branch>` (or whatever the configured command does) already
        // created a tmux session for the new worktree; switch to it instead of creating
        // a duplicate. Only fall back to creating one when no live session claims that cwd.
        let existing_session_id = self
            .session_for_cwd(&cwd_str)
            .map(|session| session.id.clone());
        let switch_result = match existing_session_id {
            Some(id) => tmux::switch_client(&id),
            None => tmux::new_session(&cwd_str, &cwd_str)
                .and_then(|created| tmux::switch_client(&created.session_id)),
        };

        match switch_result {
            Ok(()) => {
                self.should_quit = true;
            }
            Err(err) => {
                self.create_load_error = Some(format!("worktree {branch:?}: {err}"));
                self.create_worktree_branch = None;
                self.mode = Mode::CreateSession;
            }
        }
    }
}
