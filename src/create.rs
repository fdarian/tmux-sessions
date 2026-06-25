use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CreateTab {
    History,
    Worktree,
    Zoxide,
}

impl CreateTab {
    pub fn label(&self) -> &'static str {
        match self {
            CreateTab::History => "History",
            CreateTab::Worktree => "Worktree",
            CreateTab::Zoxide => "Zoxide",
        }
    }
}

#[derive(Clone)]
pub enum CreateTarget {
    ResumeDead { name: String, cwd: String },
    NewNamed { name: String, cwd: String },
    PathDir { path: String },
    NewWorktree { branch: String },
}

#[derive(Clone)]
pub struct CreateCandidate {
    pub primary: String,
    pub secondary: Option<String>,
    pub match_indices: Vec<usize>,
    pub frecency: Option<f64>,
    pub target: CreateTarget,
}

#[derive(Clone)]
pub struct WorktreeEntry {
    pub path: String,
    pub branch: String,
}

#[derive(Clone)]
pub struct ZoxideEntry {
    pub path: String,
    pub frecency: f64,
}

fn command_stdout(output: Output, command_name: &str) -> io::Result<String> {
    if !output.status.success() {
        return Err(io::Error::other(
            format!("{command_name} exited with status {}", output.status),
        ));
    }

    String::from_utf8(output.stdout).map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{command_name} returned invalid UTF-8: {err}"),
        )
    })
}

fn is_git_worktree_dir(dir: &Path) -> io::Result<bool> {
    let output = Command::new("git")
        .current_dir(dir)
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()?;
    if !output.status.success() {
        return Ok(false);
    }

    let stdout = String::from_utf8(output.stdout).map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("git rev-parse returned invalid UTF-8: {err}"),
        )
    })?;
    Ok(stdout.trim() == "true")
}

fn parse_worktree_entries(stdout: &str) -> io::Result<Vec<WorktreeEntry>> {
    let mut entries = Vec::new();
    let mut current_path: Option<String> = None;
    let mut current_branch: Option<String> = None;
    let mut is_detached = false;

    for line in stdout.lines().chain(std::iter::once("")) {
        if line.trim().is_empty() {
            if let Some(path) = current_path.take() {
                let branch = if let Some(branch) = current_branch.take() {
                    branch
                } else if is_detached {
                    "(detached)".to_string()
                } else {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("git worktree list returned no branch metadata for {path:?}"),
                    ));
                };
                entries.push(WorktreeEntry { path, branch });
            }
            current_branch = None;
            is_detached = false;
            continue;
        }

        if let Some(path) = line.strip_prefix("worktree ") {
            current_path = Some(path.to_string());
            continue;
        }
        if let Some(branch) = line.strip_prefix("branch ") {
            let branch = if let Some(branch) = branch.strip_prefix("refs/heads/") {
                branch.to_string()
            } else {
                branch.to_string()
            };
            current_branch = Some(branch);
            continue;
        }
        if line == "detached" {
            is_detached = true;
        }
    }

    if entries.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "git worktree list returned no worktree entries",
        ));
    }

    Ok(entries)
}

pub fn list_linked_worktree_paths(dir: &Path) -> io::Result<Option<Vec<WorktreeEntry>>> {
    if !is_git_worktree_dir(dir)? {
        return Ok(None);
    }

    let output = Command::new("git")
        .current_dir(dir)
        .args(["worktree", "list", "--porcelain"])
        .output()?;
    let stdout = command_stdout(output, "git worktree list --porcelain")?;
    let entries = parse_worktree_entries(&stdout)?;

    if entries.len() > 1 {
        Ok(Some(entries))
    } else {
        Ok(None)
    }
}

/// Returns worktrees when the cwd is inside a git work tree, regardless of linked count.
/// Used when `worktree_create_command` is set so the tab appears even with 0 linked worktrees.
pub fn list_worktrees_if_git(dir: &Path) -> io::Result<Option<Vec<WorktreeEntry>>> {
    if !is_git_worktree_dir(dir)? {
        return Ok(None);
    }

    let output = Command::new("git")
        .current_dir(dir)
        .args(["worktree", "list", "--porcelain"])
        .output()?;
    let stdout = command_stdout(output, "git worktree list --porcelain")?;
    let entries = parse_worktree_entries(&stdout)?;
    Ok(Some(entries))
}

/// Runs the configured worktree create command for `branch` from `cwd`, then returns the
/// path to the newly created worktree by re-querying `git worktree list --porcelain`.
pub fn run_worktree_create(command: &str, branch: &str, cwd: &Path) -> io::Result<PathBuf> {
    let tokens: Vec<&str> = command.split_whitespace().collect();
    if tokens.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "worktree_create_command is empty",
        ));
    }

    let args: Vec<String> = tokens[1..]
        .iter()
        .map(|token| token.replace("{branch}", branch))
        .collect();

    let status = Command::new(tokens[0].replace("{branch}", branch))
        .args(&args)
        .current_dir(cwd)
        .status()?;

    if !status.success() {
        return Err(io::Error::other(
            format!("worktree create command exited with status {status}"),
        ));
    }

    let output = Command::new("git")
        .current_dir(cwd)
        .args(["worktree", "list", "--porcelain"])
        .output()?;
    let stdout = command_stdout(output, "git worktree list --porcelain")?;
    let entries = parse_worktree_entries(&stdout)?;

    let matched = entries
        .into_iter()
        .find(|entry| entry.branch == branch);

    match matched {
        Some(entry) => Ok(PathBuf::from(entry.path)),
        None => Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("worktree for branch {branch:?} not found after create command ran"),
        )),
    }
}

fn parse_zoxide_entries(stdout: &str) -> io::Result<Vec<ZoxideEntry>> {
    let mut entries = Vec::new();

    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let split_index = match trimmed.find(char::is_whitespace) {
            Some(split_index) => split_index,
            None => continue,
        };
        let score_text = &trimmed[..split_index];
        let path = trimmed[split_index..].trim_start();
        if path.is_empty() {
            continue;
        }

        let frecency = match score_text.parse::<f64>() {
            Ok(frecency) => frecency,
            Err(_) => continue,
        };

        entries.push(ZoxideEntry {
            path: path.to_string(),
            frecency,
        });
    }

    Ok(entries)
}

pub fn list_zoxide_dirs() -> io::Result<Option<Vec<ZoxideEntry>>> {
    let output = match Command::new("zoxide").args(["query", "-l", "-s"]).output() {
        Ok(output) => output,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err),
    };

    let stdout = command_stdout(output, "zoxide query -l -s")?;
    Ok(Some(parse_zoxide_entries(&stdout)?))
}
