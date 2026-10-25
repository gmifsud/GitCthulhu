//! Right-click actions for graph commits and local branches.

use core_domain::Worktree;

use crate::selection::short_hash;
use crate::session::same_path;

/// What the user right-clicked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ContextTarget {
    Commit { id: String },
    LocalBranch { name: String, tip: String },
}

/// One row in the context menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ContextAction {
    pub label: &'static str,
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
                    label: "Open as worktree…",
                    enabled: open_enabled,
                    kind: ContextActionKind::OpenAsWorktree {
                        branch: format!("wip/{short}"),
                        create_branch: true,
                        start_point: Some(id.clone()),
                    },
                },
                ContextAction {
                    label: "Detach HEAD here",
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
                    label: "Open as worktree…",
                    enabled: open_enabled,
                    kind: ContextActionKind::OpenAsWorktree {
                        branch: name.clone(),
                        create_branch: false,
                        start_point: None,
                    },
                },
                ContextAction {
                    label: "Detach at tip",
                    enabled: detach_enabled,
                    kind: ContextActionKind::DetachHead {
                        commit_id: tip.clone(),
                    },
                },
            ]
        }
    }
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
}
