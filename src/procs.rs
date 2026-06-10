use std::collections::HashMap;
use std::io;
use std::process::Command;

#[derive(Clone)]
pub struct PaneContext {
    pub session_name: String,
    pub window_index: usize,
    pub window_name: String,
    #[allow(dead_code)]
    pub pane_index: usize,
    pub pane_id: String,
}

#[derive(Clone)]
pub struct ProcessRow {
    pub pid: u32,
    pub command: String,
    pub rss_kb: u64,
    pub pcpu: f64,
    pub pane: PaneContext,
}

struct PsEntry {
    pid: u32,
    ppid: u32,
    rss_kb: u64,
    pcpu: f64,
    command: String,
}

struct MonitorPane {
    pane_pid: u32,
    context: PaneContext,
}

fn run_tmux_output(args: &[&str]) -> io::Result<String> {
    let output = Command::new("tmux").args(args).output()?;
    if !output.status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("tmux {:?} exited with status {}", args, output.status),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn list_monitor_panes() -> io::Result<Vec<MonitorPane>> {
    let format = "#{session_name}\x1f#{window_index}\x1f#{window_name}\x1f#{pane_index}\x1f#{pane_id}\x1f#{pane_pid}";
    let output = run_tmux_output(&["list-panes", "-a", "-F", format])?;
    let mut panes = Vec::new();
    for line in output.lines() {
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.splitn(6, '\x1f').collect();
        if parts.len() != 6 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unexpected field count in pane line: {:?}", line),
            ));
        }
        let window_index = parts[1].parse::<usize>().map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("failed to parse window_index {:?}: {}", parts[1], e),
            )
        })?;
        let pane_index = parts[3].parse::<usize>().map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("failed to parse pane_index {:?}: {}", parts[3], e),
            )
        })?;
        let pane_pid = parts[5].parse::<u32>().map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("failed to parse pane_pid {:?}: {}", parts[5], e),
            )
        })?;
        panes.push(MonitorPane {
            pane_pid,
            context: PaneContext {
                session_name: parts[0].to_string(),
                window_index,
                window_name: parts[2].to_string(),
                pane_index,
                pane_id: parts[4].to_string(),
            },
        });
    }
    Ok(panes)
}

fn list_ps_entries() -> io::Result<Vec<PsEntry>> {
    let output = Command::new("ps")
        .args(["-axo", "pid=,ppid=,rss=,pcpu=,comm="])
        .output()?;
    if !output.status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("ps exited with status {}", output.status),
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut entries = Vec::new();
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() < 5 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unexpected field count in ps line: {:?}", line),
            ));
        }
        let pid = parts[0].parse::<u32>().map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("failed to parse pid {:?}: {}", parts[0], e),
            )
        })?;
        let ppid = parts[1].parse::<u32>().map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("failed to parse ppid {:?}: {}", parts[1], e),
            )
        })?;
        let rss_kb = parts[2].parse::<u64>().map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("failed to parse rss {:?}: {}", parts[2], e),
            )
        })?;
        let pcpu = parts[3].parse::<f64>().map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("failed to parse pcpu {:?}: {}", parts[3], e),
            )
        })?;
        let command = parts[4..].join(" ");
        entries.push(PsEntry {
            pid,
            ppid,
            rss_kb,
            pcpu,
            command,
        });
    }
    Ok(entries)
}

fn find_pane_owner(
    pid: u32,
    pane_pids: &HashMap<u32, PaneContext>,
    ppid_by_pid: &HashMap<u32, u32>,
) -> Option<PaneContext> {
    let mut current = pid;
    loop {
        if let Some(ctx) = pane_pids.get(&current) {
            return Some(ctx.clone());
        }
        match ppid_by_pid.get(&current) {
            Some(ppid) if *ppid != 0 && *ppid != current => current = *ppid,
            _ => return None,
        }
    }
}

pub fn collect_process_rows() -> io::Result<Vec<ProcessRow>> {
    let panes = list_monitor_panes()?;
    let ps_entries = list_ps_entries()?;

    let mut pane_pids: HashMap<u32, PaneContext> = HashMap::new();
    for pane in panes.iter() {
        pane_pids.insert(pane.pane_pid, pane.context.clone());
    }

    let mut ppid_by_pid: HashMap<u32, u32> = HashMap::new();
    for entry in ps_entries.iter() {
        ppid_by_pid.insert(entry.pid, entry.ppid);
    }

    let mut rows = Vec::new();
    for entry in ps_entries.iter() {
        let pane = match find_pane_owner(entry.pid, &pane_pids, &ppid_by_pid) {
            Some(ctx) => ctx,
            None => continue,
        };
        rows.push(ProcessRow {
            pid: entry.pid,
            command: entry.command.clone(),
            rss_kb: entry.rss_kb,
            pcpu: entry.pcpu,
            pane,
        });
    }
    Ok(rows)
}

pub fn kill_process(pid: u32) -> io::Result<()> {
    let status = Command::new("kill")
        .arg(pid.to_string())
        .status()?;
    if !status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("kill {} exited with status {}", pid, status),
        ));
    }
    Ok(())
}

pub fn format_rss_kb(rss_kb: u64) -> String {
    if rss_kb >= 1_048_576 {
        format!("{:.1}G", rss_kb as f64 / 1_048_576.0)
    } else if rss_kb >= 1024 {
        format!("{:.0}M", rss_kb as f64 / 1024.0)
    } else {
        format!("{}K", rss_kb)
    }
}

pub fn format_pcpu(pcpu: f64) -> String {
    format!("{:.1}%", pcpu)
}

pub fn format_pane_label(pane: &PaneContext) -> String {
    format!(
        "{} · {}:{}",
        pane.session_name, pane.window_index, pane.window_name
    )
}