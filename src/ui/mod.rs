mod create;
mod full_preview;
mod modal;
mod monitor;
mod tree;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};

use crate::app::App;
use crate::event::Mode;

pub use create::render_create_session;
pub use full_preview::render_full_preview;
pub use modal::{render_about, render_confirmation, render_move_window, render_rename_input};
pub use monitor::render_monitor;
pub use tree::{render_preview, render_tree};

pub fn render(frame: &mut Frame, app: &mut App) {
    if app.mode == Mode::Previewing {
        render_full_preview(frame, app, frame.area());
        return;
    }

    if app.mode == Mode::Monitor
        || app.mode == Mode::ProcessDetail
        || (app.mode == Mode::Confirming && app.confirming_process.is_some())
    {
        render_monitor(frame, app, frame.area());
        if app.mode == Mode::Confirming {
            render_confirmation(frame, app);
        }
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(frame.area());

    render_tree(frame, app, chunks[0]);
    render_preview(frame, app, chunks[1]);

    if app.mode == Mode::Confirming {
        render_confirmation(frame, app);
    }

    if app.mode == Mode::Renaming {
        render_rename_input(frame, app);
    }

    if app.mode == Mode::MoveWindow {
        render_move_window(frame, app);
    }

    if app.mode == Mode::CreateSession {
        render_create_session(frame, app);
    }

    if app.mode == Mode::About {
        render_about(frame);
    }
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length((area.height.saturating_sub(height)) / 2),
            Constraint::Length(height),
            Constraint::Min(0),
        ])
        .split(area);

    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(width),
            Constraint::Min(0),
        ])
        .split(vertical[1]);

    horizontal[1]
}
