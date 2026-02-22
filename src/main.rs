use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::{widgets::Paragraph, Frame};

fn main() {
    let mut terminal = ratatui::init();

    loop {
        terminal
            .draw(|frame: &mut Frame| {
                frame.render_widget(Paragraph::new("Hello from tmux-sessions. Press q to quit."), frame.area());
            })
            .expect("failed to draw");

        if let Ok(Event::Key(key)) = event::read() {
            if key.kind == KeyEventKind::Press && key.code == KeyCode::Char('q') {
                break;
            }
        }
    }

    ratatui::restore();
}
