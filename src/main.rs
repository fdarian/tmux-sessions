mod app;
mod config;
mod event;
mod tmux;
mod tree;
mod ui;

use std::env;

use std::time::Duration;

use crossterm::event::{poll, read, Event};

fn main() {
    if env::var("TMUX").is_err() {
        eprintln!("error: must be run inside a tmux session");
        std::process::exit(1);
    }

    let mut app = match app::App::new() {
        Ok(app) => app,
        Err(e) => {
            eprintln!("error: failed to initialize: {}", e);
            std::process::exit(1);
        }
    };

    let mut terminal = ratatui::init();

    loop {
        terminal
            .draw(|frame| ui::render(frame, &mut app))
            .expect("failed to draw");

        if let Ok(Event::Key(key)) = read() {
            let action = event::map_key(key, &app.mode);
            app.handle_action(action);
            // Drain any immediately-available events (e.g. modifier key + char from same keystroke).
            while poll(Duration::ZERO).unwrap_or(false) {
                if let Ok(Event::Key(key)) = read() {
                    let action = event::map_key(key, &app.mode);
                    app.handle_action(action);
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    ratatui::restore();
}
