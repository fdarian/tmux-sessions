use std::collections::{HashMap, HashSet};

use crate::app::MonitorSort;
use crate::procs::ProcessRow;

/// A flattened, ordered row in the process tree view. Mirrors the shape of
/// `crate::tree::FlatEntry` (depth, has_children, ancestor_is_last, ...) so
/// the process monitor renders with the same tree conventions as the
/// session tree. Carries no display text — the caller resolves `pid` back
/// to a `ProcessRow` for rendering.
pub struct MonitorEntry {
    pub pid: u32,
    pub depth: u8,
    pub has_children: bool,
    pub is_last_sibling: bool,
    pub ancestor_is_last: Vec<bool>,
}

#[derive(Clone, Copy, Default)]
struct SubtreeTotals {
    rss_kb: u64,
    pcpu: f64,
}

fn sort_key(totals: &SubtreeTotals, sort: MonitorSort) -> f64 {
    match sort {
        MonitorSort::Mem => totals.rss_kb as f64,
        MonitorSort::Cpu => totals.pcpu,
    }
}

/// Computes the "own + all descendants" totals for every pid, guarding
/// against cycles from bad `ps` data with a visited set. Totals are used
/// for ordering only — displayed values stay literal per-row.
fn compute_subtree_totals(
    row_by_pid: &HashMap<u32, &ProcessRow>,
    children: &HashMap<u32, Vec<u32>>,
    roots: &[u32],
) -> HashMap<u32, SubtreeTotals> {
    let mut totals: HashMap<u32, SubtreeTotals> = HashMap::new();
    let mut visited: HashSet<u32> = HashSet::new();
    for &root in roots {
        compute_subtree(root, row_by_pid, children, &mut totals, &mut visited);
    }
    totals
}

fn compute_subtree(
    pid: u32,
    row_by_pid: &HashMap<u32, &ProcessRow>,
    children: &HashMap<u32, Vec<u32>>,
    totals: &mut HashMap<u32, SubtreeTotals>,
    visited: &mut HashSet<u32>,
) -> SubtreeTotals {
    if !visited.insert(pid) {
        // Cycle guard: a pid we've already visited on this walk contributes
        // nothing further, so a bad ppid chain can't loop forever.
        return SubtreeTotals::default();
    }

    let mut own = SubtreeTotals::default();
    if let Some(row) = row_by_pid.get(&pid) {
        own.rss_kb = row.rss_kb;
        own.pcpu = row.pcpu;
    }

    if let Some(kids) = children.get(&pid) {
        for &child_pid in kids {
            let child_totals = compute_subtree(child_pid, row_by_pid, children, totals, visited);
            own.rss_kb += child_totals.rss_kb;
            own.pcpu += child_totals.pcpu;
        }
    }

    totals.insert(pid, own);
    own
}

fn sort_siblings(pids: &mut [u32], totals: &HashMap<u32, SubtreeTotals>, sort: MonitorSort) {
    pids.sort_by(|a, b| {
        let key_a = totals.get(a).map(|t| sort_key(t, sort)).unwrap_or(0.0);
        let key_b = totals.get(b).map(|t| sort_key(t, sort)).unwrap_or(0.0);
        key_b
            .partial_cmp(&key_a)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.cmp(b))
    });
}

fn push_subtree(
    pid: u32,
    depth: u8,
    is_last_sibling: bool,
    ancestor_is_last: &[bool],
    children: &HashMap<u32, Vec<u32>>,
    collapsed: &HashSet<u32>,
    entries: &mut Vec<MonitorEntry>,
) {
    let kids = children.get(&pid);
    let has_children = kids.is_some_and(|kids| !kids.is_empty());

    entries.push(MonitorEntry {
        pid,
        depth,
        has_children,
        is_last_sibling,
        ancestor_is_last: ancestor_is_last.to_vec(),
    });

    if !has_children || collapsed.contains(&pid) {
        return;
    }

    let kids = kids.expect("has_children implies kids is Some");
    let mut next_ancestor_is_last = ancestor_is_last.to_vec();
    next_ancestor_is_last.push(is_last_sibling);

    let last_index = kids.len() - 1;
    for (i, &child_pid) in kids.iter().enumerate() {
        push_subtree(
            child_pid,
            depth + 1,
            i == last_index,
            &next_ancestor_is_last,
            children,
            collapsed,
            entries,
        );
    }
}

/// Shapes flat `ProcessRow`s into a collapsible tree: children nest under
/// their parent (by `ppid`), siblings and roots are ordered descending by
/// each subtree's total (own + all descendants) of the active sort key, and
/// entries under a collapsed pid are omitted. Pure — no App state, no
/// rendering.
pub fn flatten_process_tree(
    rows: &[ProcessRow],
    collapsed: &HashSet<u32>,
    sort: MonitorSort,
) -> Vec<MonitorEntry> {
    let pid_set: HashSet<u32> = rows.iter().map(|row| row.pid).collect();
    let row_by_pid: HashMap<u32, &ProcessRow> = rows.iter().map(|row| (row.pid, row)).collect();

    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    let mut roots: Vec<u32> = Vec::new();
    for row in rows {
        if row.ppid != row.pid && pid_set.contains(&row.ppid) {
            children.entry(row.ppid).or_default().push(row.pid);
        } else {
            roots.push(row.pid);
        }
    }

    let totals = compute_subtree_totals(&row_by_pid, &children, &roots);

    sort_siblings(&mut roots, &totals, sort);
    for kids in children.values_mut() {
        sort_siblings(kids, &totals, sort);
    }

    let mut entries = Vec::new();
    if roots.is_empty() {
        return entries;
    }
    let last_root_index = roots.len() - 1;
    for (i, &pid) in roots.iter().enumerate() {
        push_subtree(
            pid,
            0,
            i == last_root_index,
            &[],
            &children,
            collapsed,
            &mut entries,
        );
    }
    entries
}
