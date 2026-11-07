//! Working-changes rows in the history graph: their labels, and the read-only status of
//! worktrees that have no tab.
//!
//! A WIP row reads "WIP · branch · folder" so it never passes for a branch pill. Folder
//! names that collide gain parent folders until they differ.

use core_domain::RepoStatus;

use crate::selection::{short_hash, Selection};
use crate::session::{same_path, SessionId};

/// First word of every WIP label.
pub(crate) const WIP_WORD: &str = "WIP";
/// Between the parts of a WIP label.
pub(crate) const WIP_SEPARATOR: &str = " · ";
/// Stands in for the branch when HEAD is detached.
pub(crate) const DETACHED_WORD: &str = "detached";

/// "WIP · branch · folder". `branch` is `None` or empty when HEAD is detached.
pub(crate) fn wip_label(branch: Option<&str>, folder: &str) -> String {
    let branch = branch.filter(|name| !name.is_empty()).unwrap_or(DETACHED_WORD);
    [WIP_WORD, branch, folder].join(WIP_SEPARATOR)
}

/// `text` in at most `max` characters, the middle replaced by `…` when it is longer. Both
/// ends stay: branch and folder names here share long prefixes and differ at the end.
pub(crate) fn middle_ellipsis(text: &str, max: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max || max < 3 {
        return text.to_string();
    }
    let keep = max - 1;
    let head = keep.div_ceil(2);
    let tail = keep - head;
    let mut out: String = chars[..head].iter().collect();
    out.push('…');
    out.extend(&chars[chars.len() - tail..]);
    out
}

/// `label` cut to at most `max` characters by shortening the middle of its longest part
/// (parts are split on [`WIP_SEPARATOR`]). The start and end of each name stay readable:
/// worktree and branch names often share a prefix and differ at the end.
pub(crate) fn fit_label(label: &str, max: usize) -> String {
    const KEEP_EACH_SIDE: usize = 4;
    let mut parts: Vec<Vec<char>> = label.split(WIP_SEPARATOR).map(|part| part.chars().collect()).collect();
    let separators = WIP_SEPARATOR.chars().count() * parts.len().saturating_sub(1);
    let total = |parts: &[Vec<char>]| parts.iter().map(Vec::len).sum::<usize>() + separators;
    while total(&parts) > max {
        let Some(longest) = (0..parts.len()).max_by_key(|&index| parts[index].len()) else {
            break;
        };
        let part = &parts[longest];
        if part.len() <= KEEP_EACH_SIDE * 2 + 1 {
            break;
        }
        let excess = total(&parts) - max;
        // Drop enough from the middle to fit, but never more than the part can spare.
        let removable = part.len() - (KEEP_EACH_SIDE * 2 + 1);
        let drop = (excess + 1).min(removable).max(1);
        let keep = part.len() - drop - 1;
        let head = keep.div_ceil(2);
        let tail = keep - head;
        let mut shorter: Vec<char> = part[..head].to_vec();
        shorter.push('…');
        shorter.extend_from_slice(&part[part.len() - tail..]);
        if shorter.len() >= part.len() {
            break;
        }
        parts[longest] = shorter;
    }
    parts
        .into_iter()
        .map(|part| part.into_iter().collect::<String>())
        .collect::<Vec<_>>()
        .join(WIP_SEPARATOR)
}

/// Path segments, root first, with either slash and no empty parts.
fn segments(path: &str) -> Vec<&str> {
    path.split(['/', '\\']).filter(|part| !part.is_empty()).collect()
}

/// Comparison form of a label: Windows folders ignore case.
fn fold(label: &str) -> String {
    if cfg!(windows) {
        label.to_lowercase()
    } else {
        label.to_string()
    }
}

/// One label per path: the folder name, with parent folders added (`a/wt`, `b/wt`) only
/// where two paths would otherwise read the same. Paths that name the same folder share
/// a label and do not count as a collision.
pub(crate) fn folder_labels(paths: &[&str]) -> Vec<String> {
    let parts: Vec<Vec<&str>> = paths.iter().map(|path| segments(path)).collect();
    let mut depth = vec![1usize; paths.len()];
    let label_at = |index: usize, depth: usize| -> String {
        let segs = &parts[index];
        if segs.is_empty() {
            return paths[index].to_string();
        }
        let from = segs.len().saturating_sub(depth);
        segs[from..].join("/")
    };
    // Each pass widens every label that still collides; it ends once none can grow.
    loop {
        let labels: Vec<String> = (0..paths.len()).map(|index| label_at(index, depth[index])).collect();
        let mut grew = false;
        for index in 0..paths.len() {
            let clash = (0..paths.len()).any(|other| {
                other != index
                    && !same_path(paths[index], paths[other])
                    && fold(&labels[other]) == fold(&labels[index])
            });
            if clash && depth[index] < parts[index].len() {
                depth[index] += 1;
                grew = true;
            }
        }
        if !grew {
            return labels;
        }
    }
}

/// The label `folder_labels` gives `path` among `paths`, or its folder name when absent.
pub(crate) fn folder_label(paths: &[&str], path: &str) -> String {
    let labels = folder_labels(paths);
    paths
        .iter()
        .position(|candidate| same_path(candidate, path))
        .map(|index| labels[index].clone())
        .unwrap_or_else(|| crate::session::basename(path).to_string())
}

/// Hover text of a WIP row: the full path, branch and HEAD, and what a click does.
pub(crate) fn wip_tooltip(
    path: &str,
    branch: Option<&str>,
    head: Option<(&str, &str)>,
    active: bool,
) -> String {
    let branch = branch.filter(|name| !name.is_empty()).unwrap_or("(detached HEAD)");
    let head = match head {
        Some((id, subject)) if !subject.is_empty() => format!("{} {subject}", short_hash(id)),
        Some((id, _)) => short_hash(id).to_string(),
        None => "unknown".to_string(),
    };
    let role = if active {
        "Active worktree: commands act here"
    } else {
        "Click to inspect · double-click to open in a tab"
    };
    format!("{path}\nBranch: {branch}\nHEAD: {head}\n{role}")
}

/// True when the worktree has staged, unstaged, untracked or conflicted paths.
pub(crate) fn is_dirty(status: &RepoStatus) -> bool {
    change_count(status) > 0
}

/// Every changed path, counted once per list it is in.
pub(crate) fn change_count(status: &RepoStatus) -> usize {
    status.staged_files.len()
        + status.unstaged_files.len()
        + status.untracked_files.len()
        + status.conflicted_files.len()
}

/// The last read of a worktree that has no tab. Built by a transient adapter that only
/// reads status; `generation` names the read so an older one landing late is dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorktreeProbe {
    pub(crate) path: String,
    pub(crate) generation: u64,
    /// `None` while the read is running, or when it failed.
    pub(crate) status: Option<RepoStatus>,
    pub(crate) error: Option<String>,
}

/// A worktree shown in the Inspector without being activated. `session` is its tab when it
/// has one; `selection_at` is the active tab's selection when inspection began, so picking
/// anything else ends it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InspectTarget {
    pub(crate) worktree_path: String,
    pub(crate) session: Option<SessionId>,
    pub(crate) selection_at: Option<Selection>,
}

/// One Working changes row, already labelled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WipRow {
    pub(crate) path: String,
    /// Index of its tab, or `None` for a worktree read through a probe.
    pub(crate) session: Option<usize>,
    pub(crate) label: String,
    pub(crate) tooltip: String,
    pub(crate) changes: usize,
    pub(crate) active: bool,
    pub(crate) inspected: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn middle_ellipsis_keeps_both_ends() {
        assert_eq!(middle_ellipsis("short", 10), "short");
        assert_eq!(middle_ellipsis("4527-order-receipt-processes", 15), "4527-or…ocesses");
        assert_eq!(middle_ellipsis("4527-order-receipt-processes", 15).chars().count(), 15);
    }

    #[test]
    fn a_label_that_fits_is_left_alone() {
        let label = wip_label(Some("main"), "primary");
        assert_eq!(fit_label(&label, 80), label);
    }

    #[test]
    fn a_long_label_loses_the_middle_of_its_longest_part() {
        let label = wip_label(
            Some("7015-signature-modal-ensure-that-same-logic-implemented-not-allowing-success-closure-of-screen"),
            "7015-signature-modal",
        );
        let fitted = fit_label(&label, 60);
        assert!(fitted.chars().count() <= 60, "{fitted}");
        assert!(fitted.starts_with("WIP · 7015-"), "{fitted}");
        assert!(fitted.contains('…'), "{fitted}");
        assert!(fitted.ends_with("of-screen · 7015-signature-modal"), "end of the branch and the folder stay: {fitted}");
    }

    #[test]
    fn a_tiny_budget_still_ends_cleanly() {
        let fitted = fit_label(&wip_label(Some("a-very-long-branch-name"), "a-very-long-folder"), 5);
        assert!(fitted.starts_with("WIP · "), "{fitted}");
    }

    #[test]
    fn the_label_names_wip_then_branch_then_folder() {
        assert_eq!(wip_label(Some("feature/analyse"), "wt"), "WIP · feature/analyse · wt");
    }

    #[test]
    fn a_detached_worktree_says_so_instead_of_a_branch() {
        assert_eq!(wip_label(None, "wt"), "WIP · detached · wt");
        assert_eq!(wip_label(Some(""), "wt"), "WIP · detached · wt");
    }

    #[test]
    fn distinct_folders_keep_their_bare_names() {
        assert_eq!(
            folder_labels(&[r"D:\repo", r"D:\wt\Knowl-analy-3bd5e"]),
            vec!["repo".to_string(), "Knowl-analy-3bd5e".to_string()]
        );
    }

    #[test]
    fn shared_folder_names_gain_parent_folders() {
        assert_eq!(
            folder_labels(&[r"D:\a\wt", r"D:\b\wt", r"D:\repo"]),
            vec!["a/wt".to_string(), "b/wt".to_string(), "repo".to_string()]
        );
    }

    #[test]
    fn parents_are_added_until_the_labels_differ() {
        assert_eq!(
            folder_labels(&["/x/one/wt", "/y/one/wt"]),
            vec!["x/one/wt".to_string(), "y/one/wt".to_string()]
        );
    }

    #[test]
    fn the_same_folder_twice_is_not_a_collision() {
        assert_eq!(
            folder_labels(&["D:/repo/wt", r"D:\repo\wt\"]),
            vec!["wt".to_string(), "wt".to_string()]
        );
    }

    #[test]
    fn a_path_that_is_one_long_suffix_of_another_stops_growing() {
        // "/wt" cannot grow past its only segment; "/a/wt" grows once and differs.
        assert_eq!(folder_labels(&["/wt", "/a/wt"]), vec!["wt".to_string(), "a/wt".to_string()]);
    }

    #[test]
    fn folder_label_looks_up_one_path() {
        let paths = [r"D:\a\wt", r"D:\b\wt"];
        assert_eq!(folder_label(&paths, "D:/b/wt"), "b/wt");
        assert_eq!(folder_label(&paths, r"D:\elsewhere\x"), "x");
    }

    #[test]
    fn the_tooltip_holds_path_branch_head_and_role() {
        let tip = wip_tooltip(r"D:\wt\x", Some("feature/y"), Some(("abcdef0123", "Fix it")), false);
        assert_eq!(
            tip,
            "D:\\wt\\x\nBranch: feature/y\nHEAD: abcdef0 Fix it\nClick to inspect · double-click to open in a tab"
        );
        let active = wip_tooltip("/r", None, None, true);
        assert!(active.contains("Branch: (detached HEAD)"));
        assert!(active.contains("HEAD: unknown"));
        assert!(active.ends_with("Active worktree: commands act here"));
    }

    #[test]
    fn dirty_counts_every_list() {
        let mut status = RepoStatus {
            staged_files: vec![],
            unstaged_files: vec![],
            untracked_files: vec![],
            tracking: None,
            head_commit_id: None,
            kinds: Default::default(),
            operation: None,
            conflicted_files: vec![],
        };
        assert!(!is_dirty(&status));
        status.conflicted_files.push("c".into());
        status.untracked_files.push("u".into());
        assert!(is_dirty(&status));
        assert_eq!(change_count(&status), 2);
    }
}
