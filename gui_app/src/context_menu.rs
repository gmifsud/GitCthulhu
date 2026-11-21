//! Right-click actions for every object the window lists: commits, local and remote branches,
//! tags, stashes, worktrees, and working-tree change rows. Pure: `context_actions` decides the
//! rows and their enabled state; the window maps each kind onto an existing message.

use core_domain::{Branch, FileDisposition, Worktree};

use crate::selection::short_hash;
use crate::session::same_path;

/// What the user right-clicked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ContextTarget {
    /// A commit row in the graph, with its subject line for copying.
    Commit { id: String, subject: String },
    LocalBranch { name: String, tip: String },
    /// A remote-tracking branch such as `origin/feature/x`.
    RemoteBranch { name: String, tip: String },
    Tag { name: String, commit_id: String },
    Stash { index: usize, message: String },
    Worktree { path: String, branch: String, is_main: bool },
    /// A file or folder row in the working-tree changes list.
    ChangePath {
        path: String,
        place: FileDisposition,
        is_dir: bool,
    },
    /// Several selected files in the working tree. The menu acts on all of them.
    Changes { files: Vec<crate::changes::MarkedFile> },
    /// A file with unresolved conflicts.
    Conflict { path: String },
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
    StageFiles(Vec<String>),
    UnstageFiles(Vec<String>),
    DiscardFiles(Vec<crate::changes::MarkedFile>),
    StashFiles(Vec<String>),
    IgnoreFiles(Vec<String>),
    /// `git rm --cached` these paths (they stay on disk), then add `ignore` to `.gitignore`.
    Untrack { paths: Vec<String>, ignore: Vec<String> },
    ResolveConflict { path: String, side: core_domain::ConflictSide },
    MarkResolved(String),
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
    CheckoutBranch { name: String },
    /// Check out the local branch for a remote one, creating it (tracking) when missing.
    CheckoutRemote { remote: String, local: String },
    /// Open the New Branch dialog starting from any ref or commit.
    NewBranchFrom { start: String },
    MergeIntoCurrent { name: String },
    /// Merge this commit, whatever branch it is on, into the current branch.
    MergeCommit { id: String },
    RebaseCurrentOnto { name: String },
    RenameBranch { name: String },
    DeleteBranch { name: String },
    CherryPick { id: String },
    ResetTo { id: String },
    /// Undo this commit, the HEAD one, keeping its changes staged.
    UndoCommit { id: String },
    CopyText { text: String },
    OpenWorktreeTab { path: String },
    NewWorktree,
    RemoveWorktree { path: String },
    /// Reveal an absolute folder (a worktree) in the OS file manager.
    OpenFolder { path: String },
    StashApply { index: usize, message: String },
    StashPop { index: usize, message: String },
    StashDrop { index: usize, message: String },
}

/// The local name a remote branch checks out as: `origin/feature/x` → `feature/x`.
pub(crate) fn local_name_for_remote(remote: &str) -> &str {
    remote.split_once('/').map_or(remote, |(_, rest)| rest)
}

fn action(label: impl Into<String>, enabled: bool, kind: ContextActionKind) -> ContextAction {
    ContextAction {
        label: label.into(),
        enabled,
        kind,
    }
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

/// Build menu rows for `target`. Pure; unit-tested. `head_is_merge`: the HEAD commit has more
/// than one parent.
pub(crate) fn context_actions(
    target: &ContextTarget,
    catalogue: &[Worktree],
    branches: &[Branch],
    active_head: Option<&str>,
    head_is_merge: bool,
    loading: bool,
) -> Vec<ContextAction> {
    let idle = !loading;
    match target {
        ContextTarget::Commit { id, subject } => {
            let short = short_hash(id);
            let on_head = active_head == Some(id.as_str());
            // A worktree already checked out here opens in a tab; only otherwise is a new one
            // offered. Offering to create one over an existing worktree made no sense.
            let mut actions: Vec<ContextAction> = catalogue
                .iter()
                .filter(|worktree| {
                    !worktree.branch.is_empty()
                        && branches.iter().any(|branch| !branch.is_remote && branch.name == worktree.branch && branch.tip == *id)
                })
                .map(|worktree| {
                    action(
                        format!("Open worktree '{}' in a tab", worktree.branch),
                        true,
                        ContextActionKind::OpenWorktreeTab { path: worktree.path.clone() },
                    )
                })
                .collect();
            actions.extend([
                action("New branch from here…", idle, ContextActionKind::NewBranchFrom { start: id.clone() }),
                action(
                    "New worktree from here…",
                    idle,
                    ContextActionKind::OpenAsWorktree {
                        branch: format!("wip/{short}"),
                        create_branch: true,
                        start_point: Some(id.clone()),
                    },
                ),
                action("Detach HEAD here", idle && !on_head, ContextActionKind::DetachHead { commit_id: id.clone() }),
                action("Merge into current…", idle && !on_head, ContextActionKind::MergeCommit { id: id.clone() }),
                action("Cherry-pick onto current…", idle && !on_head, ContextActionKind::CherryPick { id: id.clone() }),
                action("Reset current branch to here…", idle && !on_head, ContextActionKind::ResetTo { id: id.clone() }),
            ]);
            if on_head {
                // A merge is not undone here (see `actions::MERGE_HEAD_REASON`).
                actions.push(action(
                    "Undo commit (keep changes)",
                    idle && !head_is_merge,
                    ContextActionKind::UndoCommit { id: id.clone() },
                ));
            }
            actions.extend([
                action("Copy hash", true, ContextActionKind::CopyText { text: id.clone() }),
                action(format!("Copy short hash ({short})"), true, ContextActionKind::CopyText { text: short.to_string() }),
                action("Copy subject", !subject.is_empty(), ContextActionKind::CopyText { text: subject.clone() }),
            ]);
            // Every branch at this commit, local first, each copyable by name: the pills on the row,
            // without having to aim at one.
            let mut here: Vec<&Branch> = branches.iter().filter(|branch| branch.tip == *id).collect();
            here.sort_by_key(|branch| (branch.is_remote, branch.name.clone()));
            actions.extend(
                here.into_iter()
                    .map(|branch| action(format!("Copy '{}'", branch.name), true, ContextActionKind::CopyText { text: branch.name.clone() })),
            );
            actions
        }
        ContextTarget::LocalBranch { name, tip } => {
            let current = branches
                .iter()
                .any(|branch| !branch.is_remote && branch.is_head && branch.name == *name);
            let elsewhere = branch_checked_out_elsewhere(catalogue, name);
            vec![
                action(
                    format!("Checkout '{name}'"),
                    idle && !current && !elsewhere,
                    ContextActionKind::CheckoutBranch { name: name.clone() },
                ),
                action(
                    format!("New branch from '{name}'…"),
                    idle,
                    ContextActionKind::NewBranchFrom { start: name.clone() },
                ),
                action(
                    "Open as worktree…",
                    idle && !current && !elsewhere,
                    ContextActionKind::OpenAsWorktree {
                        branch: name.clone(),
                        create_branch: false,
                        start_point: None,
                    },
                ),
                action(
                    format!("Merge '{name}' into current…"),
                    idle && !current,
                    ContextActionKind::MergeIntoCurrent { name: name.clone() },
                ),
                action(
                    format!("Rebase current onto '{name}'…"),
                    idle && !current,
                    ContextActionKind::RebaseCurrentOnto { name: name.clone() },
                ),
                action(
                    "Detach at tip",
                    idle && active_head != Some(tip.as_str()),
                    ContextActionKind::DetachHead { commit_id: tip.clone() },
                ),
                action("Rename…", idle, ContextActionKind::RenameBranch { name: name.clone() }),
                action("Delete…", idle && !current, ContextActionKind::DeleteBranch { name: name.clone() }),
                action("Copy name", true, ContextActionKind::CopyText { text: name.clone() }),
            ]
        }
        ContextTarget::RemoteBranch { name, tip } => {
            let local = local_name_for_remote(name).to_string();
            let local_exists = branches.iter().any(|branch| !branch.is_remote && branch.name == local);
            let local_current = branches
                .iter()
                .any(|branch| !branch.is_remote && branch.is_head && branch.name == local);
            let checkout_label = if local_exists {
                format!("Checkout '{local}'")
            } else {
                format!("Checkout as new local branch '{local}'")
            };
            vec![
                action(
                    checkout_label,
                    idle && !local_current && !branch_checked_out_elsewhere(catalogue, &local),
                    ContextActionKind::CheckoutRemote {
                        remote: name.clone(),
                        local: local.clone(),
                    },
                ),
                action(
                    format!("New branch from '{name}'…"),
                    idle,
                    ContextActionKind::NewBranchFrom { start: name.clone() },
                ),
                action(
                    "Open as worktree…",
                    idle && !local_exists,
                    ContextActionKind::OpenAsWorktree {
                        branch: local,
                        create_branch: true,
                        start_point: Some(name.clone()),
                    },
                ),
                action(
                    format!("Merge '{name}' into current…"),
                    idle,
                    ContextActionKind::MergeIntoCurrent { name: name.clone() },
                ),
                action(
                    format!("Rebase current onto '{name}'…"),
                    idle,
                    ContextActionKind::RebaseCurrentOnto { name: name.clone() },
                ),
                action(
                    "Detach at tip",
                    idle && active_head != Some(tip.as_str()),
                    ContextActionKind::DetachHead { commit_id: tip.clone() },
                ),
                action("Copy name", true, ContextActionKind::CopyText { text: name.clone() }),
            ]
        }
        ContextTarget::Tag { name, commit_id } => vec![
            action(format!("New branch from '{name}'…"), idle, ContextActionKind::NewBranchFrom { start: name.clone() }),
            action(
                "Detach HEAD here",
                idle && active_head != Some(commit_id.as_str()),
                ContextActionKind::DetachHead { commit_id: commit_id.clone() },
            ),
            action("Copy name", true, ContextActionKind::CopyText { text: name.clone() }),
        ],
        ContextTarget::Stash { index, message } => vec![
            action("Apply", idle, ContextActionKind::StashApply { index: *index, message: message.clone() }),
            action("Pop", idle, ContextActionKind::StashPop { index: *index, message: message.clone() }),
            action("Drop…", idle, ContextActionKind::StashDrop { index: *index, message: message.clone() }),
            action("Copy message", !message.is_empty(), ContextActionKind::CopyText { text: message.clone() }),
            // The name git commands take: `git stash show stash@{2}`.
            action(format!("Copy name (stash@{{{index}}})"), true, ContextActionKind::CopyText { text: format!("stash@{{{index}}}") }),
        ],
        ContextTarget::Worktree { path, branch, is_main } => vec![
            action("Open in tab", idle, ContextActionKind::OpenWorktreeTab { path: path.clone() }),
            action("New worktree…", idle, ContextActionKind::NewWorktree),
            action("Open folder", true, ContextActionKind::OpenFolder { path: path.clone() }),
            action("Copy path", true, ContextActionKind::CopyText { text: path.clone() }),
            action(
                if branch.is_empty() { "Copy branch".to_string() } else { format!("Copy branch '{branch}'") },
                !branch.is_empty(),
                ContextActionKind::CopyText { text: branch.clone() },
            ),
            action("Remove worktree…", idle && !is_main, ContextActionKind::RemoveWorktree { path: path.clone() }),
        ],
        ContextTarget::ChangePath {
            path,
            place,
            is_dir,
        } => change_path_actions(path, *place, *is_dir, loading),
        ContextTarget::Changes { files } => selection_actions(files, loading),
        ContextTarget::Conflict { path } => conflict_actions(path, loading),
    }
}

fn files_label(verb: &str, count: usize) -> String {
    if count == 1 {
        format!("{verb} 1 file")
    } else {
        format!("{verb} {count} files")
    }
}

/// Stage, unstage, discard, stash and ignore for a multi-file selection. Each counts only the
/// files it applies to: unstage the staged ones, ignore the unversioned ones.
fn selection_actions(files: &[crate::changes::MarkedFile], loading: bool) -> Vec<ContextAction> {
    let idle = !loading;
    let paths_where = |keep: &dyn Fn(FileDisposition) -> bool| -> Vec<String> {
        files.iter().filter(|file| keep(file.place)).map(|file| file.path.clone()).collect()
    };
    let stage = paths_where(&|place| place != FileDisposition::Staged);
    let unstage = paths_where(&|place| place == FileDisposition::Staged);
    let tracked = paths_where(&|place| place != FileDisposition::Untracked);
    let untracked = paths_where(&|place| place == FileDisposition::Untracked);
    let all: Vec<String> = files.iter().map(|file| file.path.clone()).collect();
    vec![
        action(files_label("Stage", stage.len()), idle && !stage.is_empty(), ContextActionKind::StageFiles(stage)),
        action(files_label("Unstage", unstage.len()), idle && !unstage.is_empty(), ContextActionKind::UnstageFiles(unstage)),
        action(
            format!("{}…", files_label("Discard", files.len())),
            idle,
            ContextActionKind::DiscardFiles(files.to_vec()),
        ),
        action(format!("{}…", files_label("Stash", tracked.len())), idle && !tracked.is_empty(), ContextActionKind::StashFiles(tracked.clone())),
        action(files_label("Ignore", untracked.len()), idle && !untracked.is_empty(), ContextActionKind::IgnoreFiles(untracked)),
        action(
            format!("{} (keep on disk)", files_label("Untrack", tracked.len())),
            idle && !tracked.is_empty(),
            ContextActionKind::Untrack { paths: tracked.clone(), ignore: Vec::new() },
        ),
        action(
            format!("{} and ignore", files_label("Untrack", tracked.len())),
            idle && !tracked.is_empty(),
            ContextActionKind::Untrack {
                ignore: tracked.iter().map(|path| gitignore_pattern(path, false)).collect(),
                paths: tracked,
            },
        ),
        action("Copy paths", true, ContextActionKind::CopyText { text: all.join("\n") }),
    ]
}

fn conflict_actions(path: &str, loading: bool) -> Vec<ContextAction> {
    let idle = !loading;
    let side = |side| ContextActionKind::ResolveConflict { path: path.to_string(), side };
    vec![
        action("Accept yours (keep this branch)", idle, side(core_domain::ConflictSide::Yours)),
        action("Accept theirs (take the incoming change)", idle, side(core_domain::ConflictSide::Theirs)),
        action("Mark resolved (I edited the file)", idle, ContextActionKind::MarkResolved(path.to_string())),
        action("Copy path", true, ContextActionKind::CopyText { text: path.to_string() }),
    ]
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
        // Untracking needs something tracked: unversioned rows show it greyed out.
        ContextAction {
            label: if is_dir { "Untrack folder (keep on disk)" } else { "Untrack file (keep on disk)" }.into(),
            enabled: idle && place != FileDisposition::Untracked,
            kind: ContextActionKind::Untrack {
                paths: vec![display.to_string()],
                ignore: Vec::new(),
            },
        },
        ContextAction {
            label: if is_dir { "Untrack folder and ignore" } else { "Untrack file and ignore" }.into(),
            enabled: idle && place != FileDisposition::Untracked,
            kind: ContextActionKind::Untrack {
                paths: vec![display.to_string()],
                ignore: vec![gitignore_pattern(path, is_dir)],
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

    fn branch(name: &str, remote: bool, head: bool) -> Branch {
        Branch {
            name: name.into(),
            is_remote: remote,
            is_head: head,
            tip: "tip1".into(),
            ahead: 0,
            behind: 0,
        }
    }

    fn find<'a>(actions: &'a [ContextAction], label: &str) -> &'a ContextAction {
        actions
            .iter()
            .find(|action| action.label.starts_with(label))
            .unwrap_or_else(|| panic!("no '{label}' in {actions:?}"))
    }

    #[test]
    fn a_commit_row_copies_its_hash_subject_and_every_branch_on_it() {
        let target = ContextTarget::Commit { id: "abcdef0123456789".into(), subject: "Fix the parser".into() };
        let mut here = branch("feature/x", false, false);
        here.tip = "abcdef0123456789".into();
        let mut remote = branch("origin/feature/x", true, false);
        remote.tip = "abcdef0123456789".into();
        let elsewhere = branch("main", false, false);
        let actions = context_actions(&target, &[], &[remote, elsewhere, here], Some("other"), false, false);
        let copied = |label: &str| match &find(&actions, label).kind {
            ContextActionKind::CopyText { text } => text.clone(),
            other => panic!("{label} does not copy: {other:?}"),
        };
        assert_eq!(copied("Copy short hash"), "abcdef0");
        assert_eq!(copied("Copy subject"), "Fix the parser");
        assert_eq!(copied("Copy 'feature/x'"), "feature/x");
        assert_eq!(copied("Copy 'origin/feature/x'"), "origin/feature/x");
        let names: Vec<&str> = actions.iter().filter(|a| a.label.starts_with("Copy '")).map(|a| a.label.as_str()).collect();
        assert_eq!(names, vec!["Copy 'feature/x'", "Copy 'origin/feature/x'"], "local first, and only branches at this commit");
    }

    #[test]
    fn a_stash_copies_its_message_and_the_name_git_takes() {
        let target = ContextTarget::Stash { index: 2, message: "WIP on main: tidy".into() };
        let actions = context_actions(&target, &[], &[], None, false, false);
        assert!(matches!(&find(&actions, "Copy message").kind, ContextActionKind::CopyText { text } if text == "WIP on main: tidy"));
        assert!(matches!(&find(&actions, "Copy name (stash@{2})").kind, ContextActionKind::CopyText { text } if text == "stash@{2}"));
    }

    #[test]
    fn commit_menu_branches_opens_and_detaches() {
        let target = ContextTarget::Commit {
            id: "abcdef0123456789".into(),
            subject: "Fix the parser".into(),
        };
        let actions = context_actions(&target, &[], &[], Some("other"), false, false);
        assert!(matches!(
            &find(&actions, "New branch from here").kind,
            ContextActionKind::NewBranchFrom { start } if start == "abcdef0123456789"
        ));
        assert!(matches!(
            &find(&actions, "New worktree from here").kind,
            ContextActionKind::OpenAsWorktree {
                create_branch: true,
                start_point: Some(id),
                ..
            } if id == "abcdef0123456789"
        ));
        assert!(find(&actions, "Detach HEAD here").enabled);
        assert!(!actions.iter().any(|action| action.label.starts_with("Open worktree")), "no worktree is here");
    }

    #[test]
    fn a_commit_with_a_worktree_offers_to_open_it_in_a_tab() {
        let id = "abcdef0123456789";
        let target = ContextTarget::Commit { id: id.into(), subject: String::new() };
        let branches = vec![Branch {
            name: "feature/x".into(),
            is_remote: false,
            is_head: false,
            tip: id.into(),
            ahead: 0,
            behind: 0,
        }];
        let catalogue = vec![Worktree { path: "D:/wt/x".into(), branch: "feature/x".into(), is_main: false, locked: false }];
        let actions = context_actions(&target, &catalogue, &branches, Some("other"), false, false);
        assert!(
            matches!(&actions[0].kind, ContextActionKind::OpenWorktreeTab { path } if path == "D:/wt/x"),
            "the existing worktree comes first: {:?}",
            actions[0]
        );
        assert_eq!(actions[0].label, "Open worktree 'feature/x' in a tab");
    }

    #[test]
    fn only_the_head_row_offers_to_undo_its_commit() {
        let id = "abcdef0123456789";
        let target = ContextTarget::Commit { id: id.into(), subject: String::new() };
        let on_head = context_actions(&target, &[], &[], Some(id), false, false);
        let undo = find(&on_head, "Undo commit (keep changes)");
        assert!(undo.enabled);
        assert_eq!(undo.kind, ContextActionKind::UndoCommit { id: id.into() });
        let busy = context_actions(&target, &[], &[], Some(id), false, true);
        assert!(!find(&busy, "Undo commit").enabled);
        let elsewhere = context_actions(&target, &[], &[], Some("other"), false, false);
        assert!(!elsewhere.iter().any(|action| action.label.starts_with("Undo commit")));
        let merge = context_actions(&target, &[], &[], Some(id), true, false);
        assert!(!find(&merge, "Undo commit").enabled, "a merge commit is not undone here");
    }

    #[test]
    fn detach_disabled_when_already_on_commit() {
        let id = "abcdef0123456789";
        let target = ContextTarget::Commit { id: id.into(), subject: String::new() };
        let actions = context_actions(&target, &[], &[], Some(id), false, false);
        assert!(!find(&actions, "Detach HEAD here").enabled);
        assert!(!find(&actions, "Cherry-pick").enabled);
    }

    #[test]
    fn branch_open_disabled_when_checked_out_elsewhere() {
        let target = ContextTarget::LocalBranch {
            name: "feature".into(),
            tip: "tip1".into(),
        };
        let catalogue = vec![wt(r"D:\repo-feature", "feature")];
        let actions = context_actions(&target, &catalogue, &[], Some("main-tip"), false, false);
        assert!(!find(&actions, "Open as worktree").enabled);
        assert!(!find(&actions, "Checkout").enabled);
        assert!(find(&actions, "New branch from").enabled);
    }

    #[test]
    fn the_current_branch_cannot_be_checked_out_merged_or_deleted() {
        let target = ContextTarget::LocalBranch {
            name: "main".into(),
            tip: "tip1".into(),
        };
        let branches = [branch("main", false, true)];
        let actions = context_actions(&target, &[], &branches, Some("tip1"), false, false);
        assert!(!find(&actions, "Checkout").enabled);
        assert!(!find(&actions, "Merge").enabled);
        assert!(!find(&actions, "Delete").enabled);
        assert!(find(&actions, "Rename").enabled);
    }

    #[test]
    fn a_remote_branch_checks_out_as_a_new_tracking_branch() {
        let target = ContextTarget::RemoteBranch {
            name: "origin/feature/x".into(),
            tip: "tip9".into(),
        };
        let actions = context_actions(&target, &[], &[branch("main", false, true)], Some("tip1"), false, false);
        let checkout = find(&actions, "Checkout as new local branch 'feature/x'");
        assert!(checkout.enabled);
        assert!(matches!(
            &checkout.kind,
            ContextActionKind::CheckoutRemote { remote, local } if remote == "origin/feature/x" && local == "feature/x"
        ));
        assert!(matches!(
            &find(&actions, "New branch from 'origin/feature/x'").kind,
            ContextActionKind::NewBranchFrom { start } if start == "origin/feature/x"
        ));
        assert!(matches!(
            &find(&actions, "Open as worktree").kind,
            ContextActionKind::OpenAsWorktree { branch, create_branch: true, start_point: Some(start) }
                if branch == "feature/x" && start == "origin/feature/x"
        ));
    }

    #[test]
    fn a_remote_with_a_local_branch_checks_out_the_local_one() {
        let target = ContextTarget::RemoteBranch {
            name: "origin/dev".into(),
            tip: "tip9".into(),
        };
        let branches = [branch("main", false, true), branch("dev", false, false)];
        let actions = context_actions(&target, &[], &branches, Some("tip1"), false, false);
        assert!(find(&actions, "Checkout 'dev'").enabled);
        assert!(!find(&actions, "Open as worktree").enabled, "the local branch already exists");
    }

    #[test]
    fn the_main_worktree_cannot_be_removed() {
        let main = ContextTarget::Worktree {
            path: "/r/main".into(),
            branch: "main".into(),
            is_main: true,
        };
        let linked = ContextTarget::Worktree {
            path: "/r/wt".into(),
            branch: "feature".into(),
            is_main: false,
        };
        assert!(!find(&context_actions(&main, &[], &[], None, false, false), "Remove worktree").enabled);
        assert!(find(&context_actions(&linked, &[], &[], None, false, false), "Remove worktree").enabled);
        assert!(find(&context_actions(&linked, &[], &[], None, false, false), "Open in tab").enabled);
    }

    #[test]
    fn remote_names_drop_only_the_remote() {
        assert_eq!(local_name_for_remote("origin/feature/x"), "feature/x");
        assert_eq!(local_name_for_remote("upstream/main"), "main");
    }

    #[test]
    fn loading_disables_everything_but_copying() {
        let target = ContextTarget::LocalBranch {
            name: "feature".into(),
            tip: "tip1".into(),
        };
        let actions = context_actions(&target, &[], &[], Some("other"), false, true);
        assert!(actions
            .iter()
            .all(|a| !a.enabled || matches!(a.kind, ContextActionKind::CopyText { .. })));
    }

    #[test]
    fn folder_menu_offers_stage_discard_ignore_stash_untrack_open() {
        let target = ContextTarget::ChangePath {
            path: "scripts".into(),
            place: FileDisposition::Unstaged,
            is_dir: true,
        };
        let actions = context_actions(&target, &[], &[], None, false, false);
        assert_eq!(actions.len(), 7);
        assert_eq!(actions[6].label, "Open folder", "open stays last");
        assert_eq!(actions[0].label, "Stage folder");
        assert!(actions[2].label.contains("scripts/"));
        assert!(matches!(
            &actions[2].kind,
            ContextActionKind::IgnoreChange { is_dir: true, .. }
        ));
    }

    #[test]
    fn untrack_is_offered_for_tracked_rows_only() {
        let row = |place| ContextTarget::ChangePath { path: "bin/".into(), place, is_dir: true };
        let untrack = |actions: Vec<ContextAction>| {
            actions
                .into_iter()
                .filter(|action| matches!(action.kind, ContextActionKind::Untrack { .. }))
                .collect::<Vec<_>>()
        };
        let tracked = untrack(context_actions(&row(FileDisposition::Unstaged), &[], &[], None, false, false));
        assert_eq!(tracked.len(), 2);
        assert!(tracked.iter().all(|action| action.enabled));
        assert!(matches!(
            &tracked[1].kind,
            ContextActionKind::Untrack { paths, ignore } if paths == &vec!["bin".to_string()] && ignore == &vec!["bin/".to_string()]
        ));
        let unversioned = untrack(context_actions(&row(FileDisposition::Untracked), &[], &[], None, false, false));
        assert!(unversioned.iter().all(|action| !action.enabled));
    }

    #[test]
    fn staged_file_offers_unstage() {
        let target = ContextTarget::ChangePath {
            path: "a.rs".into(),
            place: FileDisposition::Staged,
            is_dir: false,
        };
        let actions = context_actions(&target, &[], &[], None, false, false);
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
