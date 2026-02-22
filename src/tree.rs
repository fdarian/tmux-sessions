use tui_tree_widget::TreeItem;

use crate::tmux;

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum NodeId {
    Session(String),
    Window(String, String),
    Pane(String, String, String),
}

pub fn build_tree_items(
    sessions: &[tmux::Session],
    windows: &[tmux::Window],
    panes: &[tmux::Pane],
) -> Vec<TreeItem<'static, NodeId>> {
    let mut items = Vec::new();
    for session in sessions {
        let session_windows: Vec<TreeItem<'static, NodeId>> = windows
            .iter()
            .filter(|w| w.session_id == session.id)
            .map(|window| {
                let window_panes: Vec<TreeItem<'static, NodeId>> = panes
                    .iter()
                    .filter(|p| p.session_id == session.id && p.window_id == window.id)
                    .map(|pane| {
                        let text = format!(
                            "{}: {}: \"{}\"",
                            pane.index, pane.current_command, pane.title,
                        );
                        TreeItem::new_leaf(
                            NodeId::Pane(
                                session.id.clone(),
                                window.id.clone(),
                                pane.id.clone(),
                            ),
                            text,
                        )
                    })
                    .collect();

                let text = format!(
                    "{}: {}: \"{}\"",
                    window.index, window.name, window.pane_title,
                );
                TreeItem::new(
                    NodeId::Window(session.id.clone(), window.id.clone()),
                    text,
                    window_panes,
                )
                .expect("duplicate pane id in window")
            })
            .collect();

        let mut text = format!("{}: {} windows", session.name, session.window_count);
        if session.attached {
            text.push_str(" (attached)");
        }
        let item = TreeItem::new(
            NodeId::Session(session.id.clone()),
            text,
            session_windows,
        )
        .expect("duplicate window id in session");
        items.push(item);
    }
    items
}

pub fn resolve_preview_pane_id(
    selected: &NodeId,
    windows: &[tmux::Window],
    panes: &[tmux::Pane],
) -> Option<String> {
    match selected {
        NodeId::Pane(_, _, pane_id) => Some(pane_id.clone()),
        NodeId::Window(session_id, window_id) => panes
            .iter()
            .find(|p| p.session_id == *session_id && p.window_id == *window_id)
            .map(|p| p.id.clone()),
        NodeId::Session(session_id) => {
            let first_window = windows.iter().find(|w| w.session_id == *session_id)?;
            panes
                .iter()
                .find(|p| p.session_id == *session_id && p.window_id == first_window.id)
                .map(|p| p.id.clone())
        }
    }
}
