use std::io;
use std::process::Command;

use ratatui::style::{Color, Modifier, Style};

#[derive(Clone)]
pub struct Session {
    pub id: String,
    pub name: String,
    pub display_name: String,
    pub window_count: usize,
    pub attached: bool,
    pub activity: u64,
}

#[derive(Clone)]
pub struct Window {
    pub session_id: String,
    pub id: String,
    pub index: usize,
    pub name: String,
    pub active: bool,
    pub pane_title: String,
    pub flags: String,
}

#[derive(Clone)]
pub struct Pane {
    pub session_id: String,
    pub window_id: String,
    pub id: String,
    pub index: usize,
    pub title: String,
    pub current_command: String,
    pub active: bool,
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

pub fn get_current_session_id() -> io::Result<String> {
    run_tmux_output(&["display-message", "-p", "#{session_id}"])
        .map(|s| s.trim().to_string())
}

pub fn list_sessions(current_session_id: &str) -> io::Result<Vec<Session>> {
    let format = "#{session_id}\x1f#{session_name}\x1f#{session_windows}\x1f#{session_activity}";
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
        let activity = parts[3].parse::<u64>().map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("failed to parse activity {:?}: {}", parts[3], e),
            )
        })?;
        sessions.push(Session {
            id: parts[0].to_string(),
            name: parts[1].to_string(),
            display_name: parts[1].to_string(),
            window_count,
            attached: parts[0] == current_session_id,
            activity,
        });
    }
    Ok(sessions)
}

pub fn list_windows() -> io::Result<Vec<Window>> {
    let format = "#{session_id}\x1f#{window_id}\x1f#{window_index}\x1f#{window_name}\x1f#{window_active}\x1f#{pane_title}\x1f#{window_flags}";
    let output = run_tmux_output(&["list-windows", "-a", "-F", format])?;
    let mut windows = Vec::new();
    for line in output.lines() {
        let parts: Vec<&str> = line.splitn(7, '\x1f').collect();
        if parts.len() != 7 {
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
            flags: parts[6].to_string(),
        });
    }
    Ok(windows)
}

pub fn list_panes() -> io::Result<Vec<Pane>> {
    let format = "#{session_id}\x1f#{window_id}\x1f#{pane_id}\x1f#{pane_index}\x1f#{pane_title}\x1f#{pane_current_command}\x1f#{pane_active}";
    let output = run_tmux_output(&["list-panes", "-a", "-F", format])?;
    let mut panes = Vec::new();
    for line in output.lines() {
        let parts: Vec<&str> = line.splitn(7, '\x1f').collect();
        if parts.len() != 7 {
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
            active: parts[6] != "0",
        });
    }
    Ok(panes)
}


pub fn capture_pane_raw(pane_id: &str) -> io::Result<Vec<u8>> {
    let output = Command::new("tmux")
        .args(&["capture-pane", "-ep", "-t", pane_id])
        .output()?;
    if !output.status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("tmux capture-pane failed for {}", pane_id),
        ));
    }
    Ok(output.stdout)
}

pub fn get_mode_style() -> io::Result<String> {
    run_tmux_output(&["show-options", "-gv", "mode-style"])
        .map(|s| s.trim().to_string())
}

pub fn parse_style(style_str: &str) -> Style {
    let mut style = Style::default();
    for term in style_str.split(',') {
        let term = term.trim();
        if let Some(color_str) = term.strip_prefix("fg=") {
            if let Some(color) = parse_color(color_str) {
                style = style.fg(color);
            }
        } else if let Some(color_str) = term.strip_prefix("bg=") {
            if let Some(color) = parse_color(color_str) {
                style = style.bg(color);
            }
        } else {
            match term {
                "bold" => style = style.add_modifier(Modifier::BOLD),
                "dim" => style = style.add_modifier(Modifier::DIM),
                "reverse" => style = style.add_modifier(Modifier::REVERSED),
                "italics" => style = style.add_modifier(Modifier::ITALIC),
                _ => {}
            }
        }
    }
    style
}

fn parse_color(s: &str) -> Option<Color> {
    if s == "default" {
        return None;
    }
    if let Some(hex) = s.strip_prefix('#') {
        if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            return Some(Color::Rgb(r, g, b));
        }
        return None;
    }
    if let Some(num) = s.strip_prefix("colour") {
        return num.parse::<u8>().ok().map(Color::Indexed);
    }
    match s {
        "black" => Some(Color::Black),
        "red" => Some(Color::Red),
        "green" => Some(Color::Green),
        "yellow" => Some(Color::Yellow),
        "blue" => Some(Color::Blue),
        "magenta" => Some(Color::Magenta),
        "cyan" => Some(Color::Cyan),
        "white" => Some(Color::White),
        _ => None,
    }
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
