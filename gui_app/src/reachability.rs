//! Filter loaded commits to those reachable from a tip (Focus current).

use core_domain::{Commit, DagNode};
use std::collections::{HashMap, HashSet, VecDeque};

/// Commit ids reachable from `tip` by walking `parent_ids` within `commits`.
///
/// Missing tip → empty. Parents not in the loaded set are ignored (page boundary).
pub(crate) fn reachable_commit_ids(tip: &str, commits: &[Commit]) -> HashSet<String> {
    let by_id: HashMap<&str, &Commit> = commits
        .iter()
        .map(|commit| (commit.id.as_str(), commit))
        .collect();
    if !by_id.contains_key(tip) {
        return HashSet::new();
    }
    let mut seen = HashSet::new();
    let mut queue = VecDeque::from([tip.to_string()]);
    while let Some(id) = queue.pop_front() {
        if !seen.insert(id.clone()) {
            continue;
        }
        let Some(commit) = by_id.get(id.as_str()) else {
            continue;
        };
        for parent in &commit.parent_ids {
            if by_id.contains_key(parent.as_str()) {
                queue.push_back(parent.clone());
            }
        }
    }
    seen
}

/// Keep DAG nodes whose commit is reachable from `tip`, preserving order.
pub(crate) fn filter_dag_nodes<'a>(
    tip: &str,
    commits: &[Commit],
    nodes: &'a [DagNode],
) -> Vec<&'a DagNode> {
    let keep = reachable_commit_ids(tip, commits);
    nodes
        .iter()
        .filter(|node| keep.contains(&node.commit_id))
        .collect()
}

/// Tip of the current checkout: local head branch tip, else `head_commit_id` from status.
pub(crate) fn session_head_tip(
    branches: &[core_domain::Branch],
    head_commit_id: Option<&str>,
) -> Option<String> {
    if let Some(branch) = branches.iter().find(|b| b.is_head && !b.is_remote) {
        return Some(branch.tip.clone());
    }
    head_commit_id.map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_domain::Branch;

    fn commit(id: &str, parents: &[&str]) -> Commit {
        Commit {
            id: id.to_string(),
            parent_ids: parents.iter().map(|p| (*p).to_string()).collect(),
            author: "a".into(),
            message: id.into(),
            timestamp: 0,
        }
    }

    #[test]
    fn linear_history_keeps_ancestors_only() {
        let commits = vec![
            commit("c", &["b"]),
            commit("b", &["a"]),
            commit("a", &[]),
            commit("other", &["a"]),
        ];
        let keep = reachable_commit_ids("b", &commits);
        assert!(keep.contains("b"));
        assert!(keep.contains("a"));
        assert!(!keep.contains("c"));
        assert!(!keep.contains("other"));
    }

    #[test]
    fn merge_commit_keeps_both_parents() {
        let commits = vec![
            commit("m", &["l", "r"]),
            commit("l", &["a"]),
            commit("r", &["a"]),
            commit("a", &[]),
            commit("x", &["a"]),
        ];
        let keep = reachable_commit_ids("m", &commits);
        assert_eq!(keep.len(), 4);
        assert!(!keep.contains("x"));
    }

    #[test]
    fn missing_tip_yields_empty() {
        let commits = vec![commit("a", &[])];
        assert!(reachable_commit_ids("missing", &commits).is_empty());
    }

    #[test]
    fn session_head_prefers_branch_tip() {
        let branches = vec![Branch {
            name: "main".into(),
            is_remote: false,
            is_head: true,
            tip: "abc".into(),
        }];
        assert_eq!(
            session_head_tip(&branches, Some("zzz")).as_deref(),
            Some("abc")
        );
    }

    #[test]
    fn session_head_falls_back_to_status_when_detached() {
        let branches = vec![Branch {
            name: "main".into(),
            is_remote: false,
            is_head: false,
            tip: "abc".into(),
        }];
        assert_eq!(
            session_head_tip(&branches, Some("det")).as_deref(),
            Some("det")
        );
    }
}
