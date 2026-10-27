//! Right-click actions for graph commits, local branches, and WIP change rows.

use core_domain::{FileDisposition, Worktree};

use crate::selection::short_hash;
use crate::session::same_path;

/// What the user right-clicked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ContextTarget {
    Commit { id: String },
    LocalBranch { name: String, tip: String },
    /// A file or folder row in the working-tree changes list.
    ChangePath {
        path: String,
        place: FileDisposition,
        is_dir: bool,
    },
}

/// One row in the context menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ContextAction {
    pub label: String,
    pub enabled: bool,
    pub kind: ContextActionKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ContextActionKind {
    OpenAsWorktree {
        branch: String,
        create_branch: bool,
        start_point: Option<String>,
    },
    DetachHead { commit_id: String },
    /// Stage (or unstage when `unstage`) this path — a file or directory prefix.
    StageChange {
        path: String,
        unstage: bool,
    },
    /// Discard every matching change under `path` (file or folder prefix).
    DiscardChange {
        path: String,
        place: FileDisposition,
        is_dir: bool,
    },
    /// Append a `.gitignore` pattern for this path.
    IgnoreChange {
        path: String,
        is_dir: bool,
    },
    /// `git stash push -- <path>` (file or folder).
    StashChange { path: String },
    /// Reveal the path (or its parent for a file) in the OS file manager.
    OpenInOs { path: String, is_dir: bool },
}

/// Open-menu state: target plus approximate pointer position for the overlay.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ContextMenuState {
    pub target: ContextTarget,
    pub x: f32,
    pub y: f32,
}

/// Whether `branch` is already checked out in some catalogue worktree.
pub(crate) fn branch_checked_out_elsewhere(catalogue: &[Worktree], branch: &str) -> bool {
    catalogue.iter().any(|row| row.branch == branch)
}

/// `.gitignore` pattern for a change path (folders keep a trailing slash).
pub(crate) fn gitignore_pattern(path: &str, is_dir: bool) -> String {
    let trimmed = path.trim().trim_end_matches('/');
    if is_dir {
        format!("{trimmed}/")
    } else {
        trimmed.to_string()
    }
}

/// Build menu rows for `target`. Pure; unit-tested.
pub(crate) fn context_actions(
    target: &ContextTarget,
    catalogue: &[Worktree],
    active_head: Option<&str>,
    loading: bool,
) -> Vec<ContextAction> {
    match target {
        ContextTarget::Commit { id } => {
            let short = short_hash(id);
            let open_enabled = !loading;
            let detach_enabled = !loading && active_head != Some(id.as_str());
            vec![
                ContextAction {
                    label: "Open as worktree…".into(),
                    enabled: open_enabled,
                    kind: ContextActionKind::OpenAsWorktree {
                        branch: format!("wip/{short}"),
                        create_branch: true,
                        start_point: Some(id.clone()),
                    },
                },
                ContextAction {
                    label: "Detach HEAD here".into(),
                    enabled: detach_enabled,
                    kind: ContextActionKind::DetachHead {
                        commit_id: id.clone(),
                    },
                },
            ]
        }
        ContextTarget::LocalBranch { name, tip } => {
            let open_enabled = !loading && !branch_checked_out_elsewhere(catalogue, name);
            let detach_enabled = !loading && active_head != Some(tip.as_str());
            vec![
                ContextAction {
                    label: "Open as worktree…".into(),
                    enabled: open_enabled,
                    kind: ContextActionKind::OpenAsWorktree {
                        branch: name.clone(),
                        create_branch: false,
                        start_point: None,
                    },
                },
                ContextAction {
                    label: "Detach at tip".into(),
                    enabled: detach_enabled,
                    kind: ContextActionKind::DetachHead {
                        commit_id: tip.clone(),
                    },
                },
            ]
        }
        ContextTarget::ChangePath {
            path,
            place,
            is_dir,
        } => change_path_actions(path, *place, *is_dir, loading),
    }
}

fn change_path_actions(
    path: &str,
    place: FileDisposition,
    is_dir: bool,
    loading: bool,
) -> Vec<ContextAction> {
    let idle = !loading;
    let display = path.trim_end_matches('/');
    let (stage_label, unstage) = match place {
        FileDisposition::Staged => {
            if is_dir {
                ("Unstage folder", true)
            } else {
                ("Unstage file", true)
            }
        }
        FileDisposition::Unstaged | FileDisposition::Untracked => {
            if is_dir {
                ("Stage folder", false)
            } else {
                ("Stage file", false)
            }
        }
    };
    let discard_label = if is_dir {
        "Discard all changes in folder"
    } else {
        "Discard changes"
    };
    let ignore_label = if is_dir {
        format!("Ignore all files in '{display}/'")
    } else {
        format!("Ignore '{display}'")
    };
    let stash_label = if is_dir { "Stash folder" } else { "Stash file" };
    let open_label = if is_dir { "Open folder" } else { "Open containing folder" };

    vec![
        ContextAction {
            label: stage_label.into(),
            enabled: idle,
            kind: ContextActionKind::StageChange {
                path: path.to_string(),
                unstage,
            },
        },
        ContextAction {
            label: discard_label.into(),
            enabled: idle,
            kind: ContextActionKind::DiscardChange {
                path: path.to_string(),
                place,
                is_dir,
            },
        },
        ContextAction {
            label: ignore_label,
            enabled: idle,
            kind: ContextActionKind::IgnoreChange {
                path: path.to_string(),
                is_dir,
            },
        },
        ContextAction {
            label: stash_label.into(),
            // Stash pathspecs need tracked changes; untracked-only folders still get the row
            // but git may refuse — keep enabled when idle and let git report.
            enabled: idle && place != FileDisposition::Untracked,
            kind: ContextActionKind::StashChange {
                path: path.to_string(),
            },
        },
        ContextAction {
            label: open_label.into(),
            enabled: true,
            kind: ContextActionKind::OpenInOs {
                path: path.to_string(),
                is_dir,
            },
        },
    ]
}

/// True when the catalogue row is the session's own path (same folder).
#[allow(dead_code)]
pub(crate) fn catalogue_row_is_self(catalogue_path: &str, session_path: &str) -> bool {
    same_path(catalogue_path, session_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_domain::Worktree;

    fn wt(path: &str, branch: &str) -> Worktree {
        Worktree {
            path: path.into(),
            branch: branch.into(),
            locked: false,
            is_main: false,
        }
    }

    #[test]
    fn commit_open_always_enabled_when_idle() {
        let target = ContextTarget::Commit {
            id: "abcdef0123456789".into(),
        };
        let actions = context_actions(&target, &[], Some("other"), false);
        assert!(actions[0].enabled);
        assert!(matches!(
            &actions[0].kind,
            ContextActionKind::OpenAsWorktree {
                create_branch: true,
                start_point: Some(id),
                ..
            } if id == "abcdef0123456789"
        ));
        assert!(actions[1].enabled);
    }

    #[test]
    fn detach_disabled_when_already_on_commit() {
        let id = "abcdef0123456789";
        let target = ContextTarget::Commit { id: id.into() };
        let actions = context_actions(&target, &[], Some(id), false);
        assert!(!actions[1].enabled);
    }

    #[test]
    fn branch_open_disabled_when_checked_out_elsewhere() {
        let target = ContextTarget::LocalBranch {
            name: "feature".into(),
            tip: "tip1".into(),
        };
        let catalogue = vec![wt(r"D:\repo-feature", "feature")];
        let actions = context_actions(&target, &catalogue, Some("main-tip"), false);
        assert!(!actions[0].enabled);
        assert!(actions[1].enabled);
    }

    #[test]
    fn loading_disables_everything() {
        let target = ContextTarget::LocalBranch {
            name: "feature".into(),
            tip: "tip1".into(),
        };
        let actions = context_actions(&target, &[], Some("other"), true);
        assert!(actions.iter().all(|a| !a.enabled));
    }

    #[test]
    fn folder_menu_offers_stage_discard_ignore_stash_open() {
        let target = ContextTarget::ChangePath {
            path: "scripts".into(),
            place: FileDisposition::Unstaged,
            is_dir: true,
        };
        let actions = context_actions(&target, &[], None, false);
        assert_eq!(actions.len(), 5);
        assert_eq!(actions[0].label, "Stage folder");
        assert!(actions[2].label.contains("scripts/"));
        assert!(matches!(
            &actions[2].kind,
            ContextActionKind::IgnoreChange { is_dir: true, .. }
        ));
    }

    #[test]
    fn staged_file_offers_unstage() {
        let target = ContextTarget::ChangePath {
            path: "a.rs".into(),
            place: FileDisposition::Staged,
            is_dir: false,
        };
        let actions = context_actions(&target, &[], None, false);
        assert_eq!(actions[0].label, "Unstage file");
        assert!(matches!(
            &actions[0].kind,
            ContextActionKind::StageChange { unstage: true, .. }
        ));
    }

    #[test]
    fn gitignore_pattern_adds_slash_for_dirs() {
        assert_eq!(gitignore_pattern("scripts", true), "scripts/");
        assert_eq!(gitignore_pattern("a.rs", false), "a.rs");
    }
}
