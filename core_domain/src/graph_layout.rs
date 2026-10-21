//! Commit graph layout: assigns each commit a lane (column) for drawing the history graph.
//!
//! Works like `git log --graph`: walking commits newest first, every lane "expects" the
//! next commit it leads to. A commit takes the lane that expects it (or the lowest free
//! lane if none does), its first parent continues in that lane, and any further parents
//! of a merge are given lanes of their own. When several lanes expect the same commit
//! they converge on it and the extra lanes are freed for reuse.

use crate::{Commit, DagNode};

/// Lay out `commits`, which must be ordered newest first (children before parents).
///
/// Each node's `connections` lists one `(parent_id, lane)` pair per parent, in parent
/// order, where `lane` is the lane the edge to that parent runs down. Parents missing
/// from `commits` (for example beyond a truncated log) keep their lane open.
/// The result is deterministic for a given input order.
pub fn build_dag(commits: &[Commit]) -> Vec<DagNode> {
    // `lanes[i]` is the id of the commit lane `i` leads to next; `None` marks a free lane.
    let mut lanes: Vec<Option<&str>> = Vec::new();
    let mut nodes = Vec::with_capacity(commits.len());

    for commit in commits {
        let id = commit.id.as_str();

        let lane = match lanes.iter().position(|slot| *slot == Some(id)) {
            Some(lane) => lane,
            None => claim_free_lane(&mut lanes),
        };
        // Any other lanes that were heading to this commit end here.
        for slot in lanes.iter_mut().skip(lane + 1) {
            if *slot == Some(id) {
                *slot = None;
            }
        }

        let mut connections = Vec::with_capacity(commit.parent_ids.len());
        for (index, parent) in commit.parent_ids.iter().enumerate() {
            let parent_lane = if index == 0 {
                lanes[lane] = Some(parent.as_str());
                lane
            } else {
                match lanes.iter().position(|slot| *slot == Some(parent.as_str())) {
                    Some(existing) => existing,
                    None => {
                        let new_lane = claim_free_lane(&mut lanes);
                        lanes[new_lane] = Some(parent.as_str());
                        new_lane
                    }
                }
            };
            connections.push((parent.clone(), parent_lane));
        }
        if commit.parent_ids.is_empty() {
            lanes[lane] = None; // root commit: its lane ends here
        }

        while lanes.last() == Some(&None) {
            lanes.pop();
        }

        nodes.push(DagNode {
            commit_id: commit.id.clone(),
            lane,
            connections,
        });
    }

    nodes
}

/// Index of the lowest free lane, opening a new one if every lane is in use.
/// The returned lane is still marked free; the caller assigns it.
fn claim_free_lane(lanes: &mut Vec<Option<&str>>) -> usize {
    match lanes.iter().position(Option::is_none) {
        Some(lane) => lane,
        None => {
            lanes.push(None);
            lanes.len() - 1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit(id: &str, parents: &[&str]) -> Commit {
        Commit {
            id: id.to_string(),
            parent_ids: parents.iter().map(|p| p.to_string()).collect(),
            author: "Test Author".to_string(),
            message: format!("commit {id}"),
            timestamp: 0,
        }
    }

    /// `(commit_id, lane)` for every node, for compact assertions.
    fn lanes(nodes: &[DagNode]) -> Vec<(&str, usize)> {
        nodes
            .iter()
            .map(|n| (n.commit_id.as_str(), n.lane))
            .collect()
    }

    fn connections<'a>(nodes: &'a [DagNode], id: &str) -> Vec<(&'a str, usize)> {
        nodes
            .iter()
            .find(|n| n.commit_id == id)
            .expect("node exists")
            .connections
            .iter()
            .map(|(parent, lane)| (parent.as_str(), *lane))
            .collect()
    }

    #[test]
    fn empty_history_has_no_nodes() {
        assert!(build_dag(&[]).is_empty());
    }

    #[test]
    fn linear_history_stays_in_one_lane() {
        let nodes = build_dag(&[
            commit("c3", &["c2"]),
            commit("c2", &["c1"]),
            commit("c1", &[]),
        ]);

        assert_eq!(lanes(&nodes), vec![("c3", 0), ("c2", 0), ("c1", 0)]);
        assert_eq!(connections(&nodes, "c3"), vec![("c2", 0)]);
        assert!(connections(&nodes, "c1").is_empty());
    }

    #[test]
    fn merge_opens_a_lane_that_converges_on_the_base() {
        // M merges feature (B) into main (A); both started from base.
        let nodes = build_dag(&[
            commit("M", &["A", "B"]),
            commit("A", &["base"]),
            commit("B", &["base"]),
            commit("base", &[]),
        ]);

        assert_eq!(
            lanes(&nodes),
            vec![("M", 0), ("A", 0), ("B", 1), ("base", 0)]
        );
        assert_eq!(connections(&nodes, "M"), vec![("A", 0), ("B", 1)]);
        assert_eq!(connections(&nodes, "B"), vec![("base", 1)]);
    }

    #[test]
    fn unmerged_branch_tips_get_separate_lanes() {
        let nodes = build_dag(&[
            commit("T1", &["base"]),
            commit("T2", &["base"]),
            commit("base", &[]),
        ]);

        assert_eq!(lanes(&nodes), vec![("T1", 0), ("T2", 1), ("base", 0)]);
    }

    #[test]
    fn freed_lanes_are_reused_before_new_ones_open() {
        // T1 and T2 converge on X, freeing lane 1 while lane 2 (heading to Z) is still open.
        // T4 must take the freed lane 1 rather than open lane 3.
        let nodes = build_dag(&[
            commit("T1", &["X"]),
            commit("T2", &["X"]),
            commit("T3", &["Z"]),
            commit("X", &["Y"]),
            commit("T4", &["Y"]),
            commit("Y", &[]),
            commit("Z", &[]),
        ]);

        assert_eq!(
            lanes(&nodes),
            vec![
                ("T1", 0),
                ("T2", 1),
                ("T3", 2),
                ("X", 0),
                ("T4", 1),
                ("Y", 0),
                ("Z", 2)
            ]
        );
    }

    #[test]
    fn octopus_merge_gives_each_extra_parent_a_lane() {
        let nodes = build_dag(&[
            commit("M", &["A", "B", "C"]),
            commit("A", &[]),
            commit("B", &[]),
            commit("C", &[]),
        ]);

        assert_eq!(connections(&nodes, "M"), vec![("A", 0), ("B", 1), ("C", 2)]);
        assert_eq!(lanes(&nodes), vec![("M", 0), ("A", 0), ("B", 1), ("C", 2)]);
    }

    #[test]
    fn merge_reuses_the_lane_already_heading_to_its_second_parent() {
        // T2 is already heading to B when M merges it, so M's edge joins that lane.
        let nodes = build_dag(&[
            commit("T2", &["B"]),
            commit("M", &["A", "B"]),
            commit("A", &[]),
            commit("B", &[]),
        ]);

        assert_eq!(connections(&nodes, "M"), vec![("A", 1), ("B", 0)]);
        assert_eq!(lanes(&nodes), vec![("T2", 0), ("M", 1), ("A", 1), ("B", 0)]);
    }

    #[test]
    fn parent_outside_the_history_keeps_its_lane_open() {
        // `older` is beyond a truncated log, so lane 0 stays reserved for it.
        let nodes = build_dag(&[commit("C", &["older"]), commit("D", &[])]);

        assert_eq!(lanes(&nodes), vec![("C", 0), ("D", 1)]);
    }
}
