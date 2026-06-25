use std::io;
use std::process::Command;

use serde::Deserialize;

#[derive(Deserialize)]
pub struct Config {
    pub formatter: Option<String>,
    pub group_name_separator: Option<String>,
    pub zoxide: Option<bool>,
    pub worktree_create_command: Option<String>,
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

pub fn format_session_name(formatter: &str, raw_name: &str) -> io::Result<String> {
    let parts: Vec<&str> = formatter.split_whitespace().collect();
    let output = Command::new(parts[0]).args(&parts[1..]).arg(raw_name).output()?;
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

