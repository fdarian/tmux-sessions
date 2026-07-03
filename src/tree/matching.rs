use fuzzy_matcher::FuzzyMatcher;

use crate::tmux;
use crate::tree::session_text;
use crate::tree::DeadSessionRef;
use crate::tree::FlatEntry;
use crate::tree::NodeId;

pub fn fuzzy_match_multi(
    matcher: &fuzzy_matcher::skim::SkimMatcherV2,
    query: &str,
    text: &str,
) -> Option<(i64, Vec<usize>)> {
    let terms: Vec<&str> = query.split_whitespace().collect();
    if terms.is_empty() {
        return Some((0, Vec::new()));
    }

    let mut total_score = 0i64;
    let mut match_indices: Vec<usize> = Vec::new();

    for term in terms {
        let (score, indices) = matcher.fuzzy_indices(text, term)?;
        total_score += score;
        match_indices.extend(indices);
    }

    match_indices.sort_unstable();
    match_indices.dedup();

    Some((total_score, match_indices))
}

pub fn match_live_sessions<'a>(
    sessions: &'a [tmux::Session],
    query: &str,
) -> Vec<&'a tmux::Session> {
    let matcher = fuzzy_matcher::skim::SkimMatcherV2::default();
    let mut matched_sessions = Vec::new();

    for session in sessions.iter() {
        let text = session_text(session);
        if fuzzy_match_multi(&matcher, query, &text).is_some() {
            matched_sessions.push(session);
        }
    }

    matched_sessions
}

pub fn match_dead_sessions(dead_sessions: &[DeadSessionRef<'_>], query: &str) -> Vec<FlatEntry> {
    let matcher = fuzzy_matcher::skim::SkimMatcherV2::default();
    let mut dead_scored: Vec<(i64, u64, FlatEntry)> = Vec::new();
    for dead in dead_sessions.iter() {
        let text = format!("{}: (dead)", dead.display_name);
        if let Some((score, _)) = fuzzy_match_multi(&matcher, query, &text) {
            dead_scored.push((
                score,
                dead.last_seen,
                FlatEntry {
                    node_id: NodeId::DeadSession(dead.name.to_string()),
                    depth: 0,
                    has_children: false,
                    is_last_sibling: false,
                    ancestor_is_last: vec![],
                    text,
                },
            ));
        }
    }
    dead_scored.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));

    dead_scored.into_iter().map(|(_, _, entry)| entry).collect()
}
