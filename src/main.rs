mod app;
mod config;
mod event;
mod history;
mod procs;
mod tmux;
mod tree;
mod ui;

use std::env;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crossterm::event::{self as crossterm_event, Event};

use crate::app::{CaptureRequest, PreviewPane};
use crate::event::Mode;
use crate::tree::NodeId;

const INPUT_POLL_RATE: Duration = Duration::from_millis(50);
const MONITOR_TICK_RATE: Duration = Duration::from_secs(2);

enum AppEvent {
    Input(Event),
    CaptureDone {
        generation: u64,
        node_id: NodeId,
        panes: Result<Vec<PreviewPane>, String>,
    },
}

fn spawn_input_thread(
    stop_requested: Arc<AtomicBool>,
    app_event_tx: mpsc::Sender<AppEvent>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        while !stop_requested.load(Ordering::Relaxed) {
            match crossterm_event::poll(INPUT_POLL_RATE) {
                Ok(true) => match crossterm_event::read() {
                    Ok(input_event) => {
                        if app_event_tx.send(AppEvent::Input(input_event)).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                },
                Ok(false) => {}
                Err(_) => break,
            }
        }
    })
}

fn spawn_capture_worker(
    capture_request_rx: mpsc::Receiver<CaptureRequest>,
    app_event_tx: mpsc::Sender<AppEvent>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        while let Ok(capture_request) = capture_request_rx.recv() {
            let generation = capture_request.generation;
            let node_id = capture_request.node_id;
            let capture_targets = capture_request.panes;
            let mut preview_panes = Vec::with_capacity(capture_targets.len());
            let mut capture_error = None;

            for capture_target in capture_targets {
                let content = match capture_target.pane_id {
                    Some(pane_id) => match tmux::capture_pane_raw(&pane_id) {
                        Ok(content) => content,
                        Err(error) => {
                            capture_error = Some(error.to_string());
                            break;
                        }
                    },
                    None => Vec::new(),
                };

                preview_panes.push(PreviewPane {
                    label: capture_target.label,
                    content,
                    is_active: capture_target.is_active,
                });
            }

            let panes = match capture_error {
                Some(error) => Err(error),
                None => Ok(preview_panes),
            };

            if app_event_tx
                .send(AppEvent::CaptureDone {
                    generation,
                    node_id,
                    panes,
                })
                .is_err()
            {
                break;
            }
        }
    })
}

fn dispatch_capture_request(
    app: &mut app::App,
    capture_request_tx: &mpsc::Sender<CaptureRequest>,
) {
    let capture_request = match app.take_pending_capture_request() {
        Some(capture_request) => capture_request,
        None => return,
    };
    let _ = capture_request_tx.send(capture_request);
}

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

    let (app_event_tx, app_event_rx) = mpsc::channel();
    let (capture_request_tx, capture_request_rx) = mpsc::channel();
    let stop_requested = Arc::new(AtomicBool::new(false));
    let input_handle = spawn_input_thread(Arc::clone(&stop_requested), app_event_tx.clone());
    let capture_handle = spawn_capture_worker(capture_request_rx, app_event_tx);

    let mut terminal = ratatui::init();
    dispatch_capture_request(&mut app, &capture_request_tx);

    loop {
        terminal
            .draw(|frame| ui::render(frame, &mut app))
            .expect("failed to draw");

        let next_event = if app.mode == Mode::Monitor {
            app_event_rx.recv_timeout(MONITOR_TICK_RATE)
        } else {
            match app_event_rx.recv() {
                Ok(next_event) => Ok(next_event),
                Err(_) => Err(mpsc::RecvTimeoutError::Disconnected),
            }
        };

        match next_event {
            Ok(AppEvent::Input(input_event)) => {
                if let Event::Key(key) = input_event {
                    let action = event::map_key(key, &app.mode);
                    app.handle_action(action);
                    dispatch_capture_request(&mut app, &capture_request_tx);
                }
            }
            Ok(AppEvent::CaptureDone {
                generation,
                node_id,
                panes,
            }) => {
                app.apply_capture_result(generation, node_id, panes);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if app.mode == Mode::Monitor {
                    app.handle_action(event::Action::Tick);
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }

        if app.should_quit {
            break;
        }
    }

    stop_requested.store(true, Ordering::Relaxed);
    drop(capture_request_tx);
    let _ = input_handle.join();
    let _ = capture_handle.join();

    ratatui::restore();
}
