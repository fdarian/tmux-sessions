use std::io;
use std::process::Command;

use serde::Deserialize;

use crate::tmux;

#[derive(Deserialize)]
pub struct Config {
    pub formatter: Option<String>,
}

pub fn load_config() -> io::Result<Option<Config>> {
    let home = std::env::var("HOME").map_err(|_| {
        io::Error::new(io::ErrorKind::NotFound, "$HOME not set")
    })?;
    let path = format!("{}/.config/tmux-sessions/config.json", home);

    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };

    let config: Config = serde_json::from_str(&content).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid config {}: {}", path, e),
        )
    })?;

    Ok(Some(config))
}

fn format_session_name(formatter: &str, raw_name: &str) -> io::Result<String> {
    let output = Command::new(formatter).arg(raw_name).output()?;
    if !output.status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("formatter exited with status {}", output.status),
        ));
    }
    let formatted = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if formatted.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "formatter returned empty output",
        ));
    }
    Ok(formatted)
}

pub fn apply_formatter_to_sessions(sessions: &mut [tmux::Session], config: &Option<Config>) {
    let formatter = match config.as_ref().and_then(|c| c.formatter.as_deref()) {
        Some(f) => f,
        None => return,
    };

    for session in sessions.iter_mut() {
        if let Ok(formatted) = format_session_name(formatter, &session.name) {
            session.display_name = formatted;
        }
    }
}
