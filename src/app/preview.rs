use std::time::{Duration, Instant};

use crate::app::App;
use crate::event::Mode;
use crate::tmux;
use crate::tree::NodeId;

const PREVIEW_DEBOUNCE: Duration = Duration::from_millis(40);

#[derive(Clone)]
pub struct PreviewPane {
    pub label: String,
    pub content: Vec<u8>,
    pub is_active: bool,
}

pub struct CapturePaneTarget {
    pub label: String,
    pub pane_id: Option<String>,
    pub is_active: bool,
}

pub struct CaptureRequest {
    pub generation: u64,
    pub node_id: NodeId,
    pub panes: Vec<CapturePaneTarget>,
}

pub struct PendingCaptureRequest {
    pub deadline: Instant,
    pub request: CaptureRequest,
}

pub struct PreviewFullPane {
    pub session_id: String,
    pub window_id: String,
    pub pane_id: String,
    pub session_name: String,
    pub window_label: String,
    pub pane_label: String,
    pub content: Vec<u8>,
}

fn full_preview_window_panes<'a>(
    panes: &'a [tmux::Pane],
    session_id: &str,
    window_id: &str,
) -> Vec<&'a tmux::Pane> {
    let mut window_panes: Vec<&tmux::Pane> = panes
        .iter()
        .filter(|pane| pane.session_id == session_id && pane.window_id == window_id)
        .collect();
    window_panes.sort_by_key(|pane| pane.index);
    window_panes
}

fn full_preview_initial_index(
    panes: &[&tmux::Pane],
    selected_pane_id: Option<&str>,
) -> Option<usize> {
    match selected_pane_id {
        Some(selected_pane_id) => panes.iter().position(|pane| pane.id == selected_pane_id),
        None => panes
            .iter()
            .position(|pane| pane.active)
            .or_else(|| (!panes.is_empty()).then_some(0)),
    }
}

impl App {
    pub fn update_preview(&mut self) {
        self.preview_generation = self.preview_generation.wrapping_add(1);

        let selected_index = match self.list_state.selected() {
            Some(selected_index) if selected_index < self.flat_entries.len() => selected_index,
            _ => {
                self.pending_preview_request = None;
                self.preview_notice = None;
                self.preview_panes.clear();
                self.preview_title.clear();
                return;
            }
        };

        let node_id = self.flat_entries[selected_index].node_id.clone();
        self.preview_title = self.preview_title_for_node(&node_id, selected_index);

        let panes = match self.capture_targets_for_node(&node_id) {
            Some(panes) => panes,
            None => {
                self.pending_preview_request = None;
                self.preview_notice = None;
                self.preview_panes.clear();
                return;
            }
        };

        if let Some(cached) = self.preview_cache.get(&node_id) {
            self.preview_panes = cached.clone();
            self.preview_notice = None;
        } else {
            self.preview_panes.clear();
            self.preview_notice = Some("capturing...".to_string());
        }
        self.pending_preview_request = Some(PendingCaptureRequest {
            deadline: Instant::now() + PREVIEW_DEBOUNCE,
            request: CaptureRequest {
                generation: self.preview_generation,
                node_id,
                panes,
            },
        });
    }

    pub fn pending_capture_deadline(&self) -> Option<Instant> {
        self.pending_preview_request.as_ref().map(|p| p.deadline)
    }

    pub fn take_pending_capture_request(&mut self) -> Option<CaptureRequest> {
        self.pending_preview_request.take().map(|p| p.request)
    }

    pub fn apply_capture_result(
        &mut self,
        generation: u64,
        node_id: NodeId,
        panes: Result<Vec<PreviewPane>, String>,
    ) {
        if generation != self.preview_generation {
            return;
        }

        match panes {
            Ok(panes) => {
                self.preview_cache.insert(node_id, panes.clone());
                self.preview_panes = panes;
                self.preview_notice = None;
            }
            Err(error) => {
                self.preview_panes.clear();
                self.preview_notice = Some(format!("capture failed: {}", error));
            }
        }
    }

    fn preview_title_for_node(&self, node_id: &NodeId, selected_index: usize) -> String {
        match node_id {
            NodeId::Recent(inner) => self.preview_title_for_node(inner, selected_index),
            NodeId::Separator(_)
            | NodeId::DeadSession(_)
            | NodeId::Group(_)
            | NodeId::Header(_) => String::new(),
            NodeId::Session(session_id) => {
                let session = self
                    .sessions
                    .iter()
                    .find(|session| session.id == *session_id);
                match session {
                    Some(session) => session.display_name.clone(),
                    None => String::new(),
                }
            }
            NodeId::Window(_, _) | NodeId::Pane(_, _, _) => {
                format!(" {} (sort: index) ", selected_index)
            }
        }
    }

    fn capture_targets_for_node(&self, node_id: &NodeId) -> Option<Vec<CapturePaneTarget>> {
        match node_id {
            NodeId::Recent(inner) => self.capture_targets_for_node(inner),
            NodeId::Separator(_)
            | NodeId::DeadSession(_)
            | NodeId::Group(_)
            | NodeId::Header(_) => None,
            NodeId::Session(session_id) => {
                let mut panes = Vec::new();
                for window in self
                    .windows
                    .iter()
                    .filter(|window| window.session_id == *session_id)
                {
                    panes.push(CapturePaneTarget {
                        label: format!("{}:{}", window.index, window.name),
                        pane_id: self.active_or_first_pane_id(session_id, &window.id),
                        is_active: window.active,
                    });
                }
                Some(panes)
            }
            NodeId::Window(session_id, window_id) => {
                let window = self.windows.iter().find(|window| window.id == *window_id)?;
                Some(vec![CapturePaneTarget {
                    label: format!("{}:{}", window.index, window.name),
                    pane_id: self.active_or_first_pane_id(session_id, window_id),
                    is_active: true,
                }])
            }
            NodeId::Pane(_, _, pane_id) => {
                let pane = self.panes.iter().find(|pane| pane.id == *pane_id)?;
                Some(vec![CapturePaneTarget {
                    label: format!("{}:{}", pane.index, pane.current_command),
                    pane_id: Some(pane.id.clone()),
                    is_active: true,
                }])
            }
        }
    }

    fn active_or_first_pane_id(&self, session_id: &str, window_id: &str) -> Option<String> {
        let active_pane = self.panes.iter().find(|pane| {
            pane.session_id == session_id && pane.window_id == window_id && pane.active
        });
        if let Some(active_pane) = active_pane {
            return Some(active_pane.id.clone());
        }

        let first_pane = self
            .panes
            .iter()
            .find(|pane| pane.session_id == session_id && pane.window_id == window_id)?;
        Some(first_pane.id.clone())
    }

    fn build_full_preview(&self) -> (Vec<PreviewFullPane>, usize) {
        let i = match self.list_state.selected() {
            Some(i) if i < self.flat_entries.len() => i,
            _ => return (Vec::new(), 0),
        };

        let node_id = &self.flat_entries[i].node_id;
        self.build_full_preview_from_target(node_id)
    }

    fn build_full_preview_from_target(&self, node_id: &NodeId) -> (Vec<PreviewFullPane>, usize) {
        match node_id.target() {
            NodeId::Separator(_)
            | NodeId::DeadSession(_)
            | NodeId::Group(_)
            | NodeId::Header(_) => (Vec::new(), 0),
            NodeId::Pane(session_id, window_id, pane_id) => {
                self.build_full_preview_for_window(session_id, window_id, Some(pane_id))
            }
            NodeId::Window(session_id, window_id) => {
                self.build_full_preview_for_window(session_id, window_id, None)
            }
            NodeId::Session(session_id) => self.build_full_preview_for_session(session_id),
            NodeId::Recent(_) => unreachable!(),
        }
    }

    fn build_full_preview_for_window(
        &self,
        session_id: &str,
        window_id: &str,
        selected_pane_id: Option<&str>,
    ) -> (Vec<PreviewFullPane>, usize) {
        let session_name = self
            .sessions
            .iter()
            .find(|session| session.id == session_id)
            .map(|session| session.display_name.clone())
            .unwrap_or_else(|| session_id.to_string());
        let window_label = self
            .windows
            .iter()
            .find(|window| window.id == window_id)
            .map(|window| format!("{}:{}", window.index, window.name))
            .unwrap_or_else(|| window_id.to_string());
        let window_panes = full_preview_window_panes(&self.panes, session_id, window_id);
        let initial_index = match full_preview_initial_index(&window_panes, selected_pane_id) {
            Some(initial_index) => initial_index,
            None => return (Vec::new(), 0),
        };

        let previews: Vec<PreviewFullPane> = window_panes
            .iter()
            .map(|pane| PreviewFullPane {
                session_id: session_id.to_string(),
                window_id: window_id.to_string(),
                pane_id: pane.id.clone(),
                session_name: session_name.clone(),
                window_label: window_label.clone(),
                pane_label: format!("{}:{}", pane.index, pane.current_command),
                content: tmux::capture_pane_raw(&pane.id).unwrap_or_default(),
            })
            .collect();

        (previews, initial_index)
    }

    fn build_full_preview_for_session(&self, session_id: &str) -> (Vec<PreviewFullPane>, usize) {
        let session = self.sessions.iter().find(|s| s.id == session_id);
        let session_name = session
            .map(|s| s.display_name.clone())
            .unwrap_or_else(|| session_id.to_string());

        let mut session_windows: Vec<&crate::tmux::Window> = self
            .windows
            .iter()
            .filter(|w| w.session_id == session_id)
            .collect();
        session_windows.sort_by(|a, b| a.index.cmp(&b.index));

        let mut previews = Vec::new();
        let mut initial_index = 0;
        let mut found_active = false;
        let mut first_active_fallback = None;

        for window in &session_windows {
            let mut window_panes: Vec<&crate::tmux::Pane> = self
                .panes
                .iter()
                .filter(|p| p.session_id == session_id && p.window_id == window.id)
                .collect();
            window_panes.sort_by(|a, b| a.index.cmp(&b.index));

            for pane in &window_panes {
                if !found_active && window.active && pane.active {
                    initial_index = previews.len();
                    found_active = true;
                }
                if first_active_fallback.is_none() && pane.active {
                    first_active_fallback = Some(previews.len());
                }

                let window_label = format!("{}:{}", window.index, window.name);
                let pane_label = format!("{}:{}", pane.index, pane.current_command);
                let content = tmux::capture_pane_raw(&pane.id).unwrap_or_default();

                previews.push(PreviewFullPane {
                    session_id: session_id.to_string(),
                    window_id: window.id.clone(),
                    pane_id: pane.id.clone(),
                    session_name: session_name.clone(),
                    window_label,
                    pane_label,
                    content,
                });
            }
        }

        if !found_active {
            initial_index = first_active_fallback.unwrap_or(0);
        }

        (previews, initial_index)
    }

    pub fn handle_enter_full_preview(&mut self) {
        let (panes, initial_index) = self.build_full_preview();
        if !panes.is_empty() {
            self.preview_full_panes = panes;
            self.preview_full_index = initial_index;
            self.mode = Mode::Previewing;
        }
    }

    pub fn handle_exit_full_preview(&mut self) {
        self.mode = Mode::Normal;
        self.preview_full_panes.clear();
        self.preview_full_index = 0;
    }

    pub fn handle_preview_prev(&mut self) {
        if !self.preview_full_panes.is_empty() {
            let len = self.preview_full_panes.len();
            self.preview_full_index = (self.preview_full_index + len - 1) % len;
        }
    }

    pub fn handle_preview_next(&mut self) {
        if !self.preview_full_panes.is_empty() {
            let len = self.preview_full_panes.len();
            self.preview_full_index = (self.preview_full_index + 1) % len;
        }
    }

    pub fn handle_select_preview_pane(&mut self) {
        if let Some(preview) = self.preview_full_panes.get(self.preview_full_index) {
            let result = tmux::switch_client(&preview.session_id)
                .and_then(|_| tmux::select_window(&preview.window_id))
                .and_then(|_| tmux::select_pane(&preview.pane_id));
            if result.is_ok() {
                self.should_quit = true;
            }
        }
    }
}

#[cfg(test)]
mod full_preview_tests {
    use super::full_preview_initial_index;
    use super::full_preview_window_panes;
    use crate::tmux::Pane;

    fn pane(id: &str, session_id: &str, window_id: &str, index: usize, active: bool) -> Pane {
        Pane {
            session_id: session_id.to_string(),
            window_id: window_id.to_string(),
            id: id.to_string(),
            index,
            title: String::new(),
            current_command: "zsh".to_string(),
            active,
        }
    }

    #[test]
    fn pane_preview_includes_window_siblings_and_starts_on_selected_pane() {
        let panes = vec![
            pane("%1", "$1", "@1", 1, false),
            pane("%0", "$1", "@1", 0, true),
            pane("%2", "$1", "@2", 0, true),
        ];

        let window_panes = full_preview_window_panes(&panes, "$1", "@1");
        let pane_ids: Vec<&str> = window_panes.iter().map(|pane| pane.id.as_str()).collect();

        assert_eq!(pane_ids, vec!["%0", "%1"]);
        assert_eq!(
            full_preview_initial_index(&window_panes, Some("%1")),
            Some(1)
        );
    }
}
