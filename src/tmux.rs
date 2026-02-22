use std::io;
use std::process::Command;

#[derive(Clone)]
pub struct Session {
    pub id: String,
    pub name: String,
    pub window_count: usize,
    pub attached: bool,
}

#[derive(Clone)]
pub struct Window {
    pub session_id: String,
    pub id: String,
    pub index: usize,
    pub name: String,
    pub active: bool,
    pub pane_title: String,
}

#[derive(Clone)]
pub struct Pane {
    pub session_id: String,
    pub window_id: String,
    pub id: String,
    pub index: usize,
    pub title: String,
    pub current_command: String,
}

fn run_tmux_output(args: &[&str]) -> io::Result<String> {
    let output = Command::new("tmux").args(args).output()?;
    if !output.status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!(
                "tmux {:?} exited with status {}",
                args, output.status
            ),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn run_tmux(args: &[&str]) -> io::Result<()> {
    let status = Command::new("tmux").args(args).status()?;
    if !status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("tmux {:?} exited with status {}", args, status),
        ));
    }
    Ok(())
}

pub fn list_sessions() -> io::Result<Vec<Session>> {
    let format = "#{session_id}\x1f#{session_name}\x1f#{session_windows}\x1f#{session_attached}";
    let output = run_tmux_output(&["list-sessions", "-F", format])?;
    let mut sessions = Vec::new();
    for line in output.lines() {
        let parts: Vec<&str> = line.split('\x1f').collect();
        if parts.len() != 4 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unexpected field count in session line: {:?}", line),
            ));
        }
        let window_count = parts[2].parse::<usize>().map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("failed to parse window_count {:?}: {}", parts[2], e),
            )
        })?;
        sessions.push(Session {
            id: parts[0].to_string(),
            name: parts[1].to_string(),
            window_count,
            attached: parts[3] != "0",
        });
    }
    Ok(sessions)
}

pub fn list_windows() -> io::Result<Vec<Window>> {
    let format = "#{session_id}\x1f#{window_id}\x1f#{window_index}\x1f#{window_name}\x1f#{window_active}\x1f#{pane_title}";
    let output = run_tmux_output(&["list-windows", "-a", "-F", format])?;
    let mut windows = Vec::new();
    for line in output.lines() {
        let parts: Vec<&str> = line.splitn(6, '\x1f').collect();
        if parts.len() != 6 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unexpected field count in window line: {:?}", line),
            ));
        }
        let index = parts[2].parse::<usize>().map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("failed to parse window_index {:?}: {}", parts[2], e),
            )
        })?;
        windows.push(Window {
            session_id: parts[0].to_string(),
            id: parts[1].to_string(),
            index,
            name: parts[3].to_string(),
            active: parts[4] != "0",
            pane_title: parts[5].to_string(),
        });
    }
    Ok(windows)
}

pub fn list_panes() -> io::Result<Vec<Pane>> {
    let format = "#{session_id}\x1f#{window_id}\x1f#{pane_id}\x1f#{pane_index}\x1f#{pane_title}\x1f#{pane_current_command}";
    let output = run_tmux_output(&["list-panes", "-a", "-F", format])?;
    let mut panes = Vec::new();
    for line in output.lines() {
        let parts: Vec<&str> = line.splitn(6, '\x1f').collect();
        if parts.len() != 6 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unexpected field count in pane line: {:?}", line),
            ));
        }
        let index = parts[3].parse::<usize>().map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("failed to parse pane_index {:?}: {}", parts[3], e),
            )
        })?;
        panes.push(Pane {
            session_id: parts[0].to_string(),
            window_id: parts[1].to_string(),
            id: parts[2].to_string(),
            index,
            title: parts[4].to_string(),
            current_command: parts[5].to_string(),
        });
    }
    Ok(panes)
}

pub fn capture_pane(pane_id: &str) -> io::Result<String> {
    let raw = run_tmux_output(&["capture-pane", "-ep", "-t", pane_id])?;
    let mut cleaned = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
                // skip until we hit a letter
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            // other escape sequences (non-CSI) are dropped
        } else {
            cleaned.push(ch);
        }
    }
    Ok(cleaned)
}

pub fn switch_client(target: &str) -> io::Result<()> {
    run_tmux(&["switch-client", "-t", target])
}

pub fn select_window(target: &str) -> io::Result<()> {
    run_tmux(&["select-window", "-t", target])
}

pub fn select_pane(target: &str) -> io::Result<()> {
    run_tmux(&["select-pane", "-t", target])
}

pub fn kill_session(target: &str) -> io::Result<()> {
    run_tmux(&["kill-session", "-t", target])
}

pub fn kill_window(target: &str) -> io::Result<()> {
    run_tmux(&["kill-window", "-t", target])
}

pub fn kill_pane(target: &str) -> io::Result<()> {
    run_tmux(&["kill-pane", "-t", target])
}
