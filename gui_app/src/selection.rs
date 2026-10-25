//! Which row is selected, and which ribbon commands that selection allows.
//!
//! The view renders this. It does not decide it.

use core_domain::{Branch, Commit, FileDisposition, RepoStatus, Stash, Tag};

/// The row the ribbon is describing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Selection {
    Branch {
        name: String,
        current: bool,
    },
    Remote {
        name: String,
    },
    Commit {
        id: String,
    },
    File {
        path: String,
        place: FileDisposition,
    },
    Stash {
        index: usize,
        message: String,
    },
    Tag {
        name: String,
        commit_id: String,
    },
}

/// A command the ribbon can show. The view turns it into a [`crate::app::Message`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RibbonCommand {
    Checkout,
    Fetch,
    Pull,
    Push,
    Refresh,
    Stage,
    Unstage,
    Discard,
    Commit,
    Stash,
    Apply,
    Pop,
    Drop,
    CreateBranch,
    RenameBranch,
    DeleteBranch,
    Merge,
    Rebase,
    CherryPick,
    Reset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RibbonItem {
    pub label: &'static str,
    pub command: RibbonCommand,
    pub enabled: bool,
    pub primary: bool,
}

/// The selection still on screen. A missing branch, commit, file, or stash falls back to
/// the current branch, which is "Detached HEAD" when no local branch is checked out.
/// A stash matches only when both its index and its message are still in the list.
pub(crate) fn visible_selection(
    stored: Option<&Selection>,
    branches: &[Branch],
    commits: &[Commit],
    stashes: &[Stash],
    tags: &[Tag],
    status: Option<&RepoStatus>,
) -> Selection {
    let fallback = Selection::Branch {
        name: current_branch_name(branches),
        current: true,
    };
    let Some(stored) = stored else {
        return fallback;
    };
    match stored {
        Selection::Branch { name, .. } => match branches
            .iter()
            .find(|branch| !branch.is_remote && branch.name == *name)
        {
            Some(branch) => Selection::Branch {
                name: branch.name.clone(),
                current: branch.is_head,
            },
            None => fallback,
        },
        Selection::Remote { name } => {
            if branches
                .iter()
                .any(|branch| branch.is_remote && branch.name == *name)
            {
                Selection::Remote { name: name.clone() }
            } else {
                fallback
            }
        }
        Selection::Commit { id } => {
            if commits.iter().any(|commit| commit.id == *id) {
                Selection::Commit { id: id.clone() }
            } else {
                fallback
            }
        }
        Selection::File { path, place } => {
            let Some(status) = status else {
                return fallback;
            };
            let place = match place {
                FileDisposition::Staged if contains_path(&status.staged_files, path) => {
                    FileDisposition::Staged
                }
                FileDisposition::Unstaged if contains_path(&status.unstaged_files, path) => {
                    FileDisposition::Unstaged
                }
                FileDisposition::Untracked if contains_path(&status.untracked_files, path) => {
                    FileDisposition::Untracked
                }
                _ if contains_path(&status.staged_files, path) => FileDisposition::Staged,
                _ if contains_path(&status.unstaged_files, path) => FileDisposition::Unstaged,
                _ if contains_path(&status.untracked_files, path) => FileDisposition::Untracked,
                _ => return fallback,
            };
            Selection::File {
                path: path.clone(),
                place,
            }
        }
        Selection::Stash { index, message } => {
            if stashes
                .iter()
                .any(|stash| stash.index == *index && stash.message == *message)
            {
                Selection::Stash {
                    index: *index,
                    message: message.clone(),
                }
            } else {
                fallback
            }
        }
        Selection::Tag { name, .. } => match tags.iter().find(|tag| tag.name == *name) {
            Some(tag) => Selection::Tag {
                name: tag.name.clone(),
                commit_id: tag.commit_id.clone(),
            },
            None => fallback,
        },
    }
}

pub(crate) fn context_label(selection: &Selection) -> String {
    match selection {
        Selection::Branch { .. } => "Branch".to_string(),
        Selection::Remote { .. } => "Remote".to_string(),
        Selection::Commit { id } => short_hash(id).to_string(),
        Selection::File { .. } => "Changes".to_string(),
        Selection::Stash { index, .. } => format!("stash@{{{index}}}"),
        Selection::Tag { name, .. } => name.clone(),
    }
}

pub(crate) fn short_hash(id: &str) -> &str {
    let end = id.len().min(7);
    &id[..end]
}

/// Commands for `selection`. `can_commit` is decided by the window from the message
/// and the index. `can_stash` is true when a tracked change exists; untracked files
/// alone do not count, because `git stash push` leaves them in the worktree.
/// Every command is disabled while a repository job is running; that gate stays in the view.
pub(crate) fn ribbon_commands(
    selection: &Selection,
    can_commit: bool,
    can_stash: bool,
) -> Vec<RibbonItem> {
    match selection {
        Selection::Branch { name, current } => vec![
            item("Checkout", RibbonCommand::Checkout, !current, false),
            item("Fetch", RibbonCommand::Fetch, true, false),
            item("Pull", RibbonCommand::Pull, true, false),
            item("Push", RibbonCommand::Push, true, false),
            item("Refresh", RibbonCommand::Refresh, true, false),
            item("Stash", RibbonCommand::Stash, can_stash, false),
            item("Create", RibbonCommand::CreateBranch, true, false),
            item(
                "Rename",
                RibbonCommand::RenameBranch,
                name.as_str() != DETACHED_HEAD,
                false,
            ),
            item(
                "Delete",
                RibbonCommand::DeleteBranch,
                name.as_str() != DETACHED_HEAD && !current,
                false,
            ),
            item(
                "Merge",
                RibbonCommand::Merge,
                name.as_str() != DETACHED_HEAD && !current,
                false,
            ),
            item(
                "Rebase",
                RibbonCommand::Rebase,
                name.as_str() != DETACHED_HEAD && !current,
                false,
            ),
        ],
        Selection::Remote { .. } => vec![
            item("Checkout", RibbonCommand::Checkout, false, false),
            item("Create", RibbonCommand::CreateBranch, true, false),
        ],
        Selection::Commit { .. } => vec![
            item("Cherry-pick", RibbonCommand::CherryPick, true, false),
            item("Reset", RibbonCommand::Reset, true, false),
        ],
        Selection::File { place, .. } => {
            let (label, command) = match place {
                FileDisposition::Staged => ("Unstage", RibbonCommand::Unstage),
                FileDisposition::Unstaged | FileDisposition::Untracked => {
                    ("Stage", RibbonCommand::Stage)
                }
            };
            vec![
                item(label, command, true, false),
                item("Discard", RibbonCommand::Discard, true, false),
                item("Commit", RibbonCommand::Commit, can_commit, true),
            ]
        }
        Selection::Stash { .. } => vec![
            item("Apply", RibbonCommand::Apply, true, false),
            item("Pop", RibbonCommand::Pop, true, false),
            item("Drop", RibbonCommand::Drop, true, false),
        ],
        Selection::Tag { .. } => vec![],
    }
}

fn item(label: &'static str, command: RibbonCommand, enabled: bool, primary: bool) -> RibbonItem {
    RibbonItem {
        label,
        command,
        enabled,
        primary,
    }
}

const DETACHED_HEAD: &str = "Detached HEAD";

fn current_branch_name(branches: &[Branch]) -> String {
    branches
        .iter()
        .find(|branch| branch.is_head && !branch.is_remote)
        .map(|branch| branch.name.clone())
        .unwrap_or_else(|| DETACHED_HEAD.to_string())
}

fn contains_path(paths: &[String], path: &str) -> bool {
    paths.iter().any(|candidate| candidate == path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn branch(name: &str, is_remote: bool, is_head: bool) -> Branch {
        Branch {
            name: name.to_string(),
            is_remote,
            is_head,
            tip: String::new(),
        }
    }

    fn commit(id: &str) -> Commit {
        Commit {
            id: id.to_string(),
            parent_ids: vec![],
            author: "Tester".to_string(),
            message: "subject".to_string(),
            timestamp: 0,
        }
    }

    fn status(staged: &[&str], unstaged: &[&str], untracked: &[&str]) -> RepoStatus {
        let owned = |files: &[&str]| files.iter().map(|file| (*file).to_string()).collect();
        RepoStatus {
            staged_files: owned(staged),
            unstaged_files: owned(unstaged),
            untracked_files: owned(untracked),
            tracking: None,
            head_commit_id: None,
        }
    }

    fn labels(items: &[RibbonItem]) -> Vec<&'static str> {
        items.iter().map(|item| item.label).collect()
    }

    #[test]
    fn the_current_branch_cannot_be_checked_out_again() {
        let selection = Selection::Branch {
            name: "dev".to_string(),
            current: true,
        };
        let items = ribbon_commands(&selection, false, false);
        assert_eq!(
            labels(&items),
            [
                "Checkout", "Fetch", "Pull", "Push", "Refresh", "Stash", "Create", "Rename",
                "Delete", "Merge", "Rebase",
            ]
        );
        assert!(!items[0].enabled);
        assert!(items[1].enabled);
        assert!(!items[5].enabled);
        assert!(items[6].enabled);
        assert!(items[7].enabled);
        assert!(!items[8].enabled);
        assert_eq!(items[8].command, RibbonCommand::DeleteBranch);
        assert!(!items[9].enabled);
        assert_eq!(items[9].command, RibbonCommand::Merge);
        assert!(!items[10].enabled);
        assert_eq!(items[10].command, RibbonCommand::Rebase);
        assert_eq!(context_label(&selection), "Branch");
    }

    #[test]
    fn another_local_branch_can_be_checked_out() {
        let selection = Selection::Branch {
            name: "main".to_string(),
            current: false,
        };
        let items = ribbon_commands(&selection, false, false);
        assert!(items[0].enabled);
        assert_eq!(items[0].command, RibbonCommand::Checkout);
        assert!(items[8].enabled);
        assert_eq!(items[8].command, RibbonCommand::DeleteBranch);
        assert!(items[9].enabled);
        assert_eq!(items[9].command, RibbonCommand::Merge);
        assert!(items[10].enabled);
        assert_eq!(items[10].command, RibbonCommand::Rebase);
    }

    #[test]
    fn a_detached_head_can_create_a_branch_but_not_rename_or_delete() {
        let selection = Selection::Branch {
            name: "Detached HEAD".to_string(),
            current: true,
        };
        let items = ribbon_commands(&selection, false, false);
        assert!(items[6].enabled);
        assert!(!items[7].enabled);
        assert!(!items[8].enabled);
        assert!(!items[9].enabled);
        assert!(!items[10].enabled);
    }

    #[test]
    fn a_remote_is_not_a_checkout_target() {
        let selection = Selection::Remote {
            name: "origin/dev".to_string(),
        };
        let items = ribbon_commands(&selection, true, false);
        assert_eq!(labels(&items), ["Checkout", "Create"]);
        assert!(!items[0].enabled);
        assert!(items[1].enabled);
        assert_eq!(context_label(&selection), "Remote");
    }

    #[test]
    fn a_commit_offers_cherry_pick_and_reset() {
        let selection = Selection::Commit {
            id: "1558f59abc".to_string(),
        };
        let items = ribbon_commands(&selection, true, false);
        assert_eq!(labels(&items), ["Cherry-pick", "Reset"]);
        assert!(items.iter().all(|item| item.enabled && !item.primary));
        assert_eq!(context_label(&selection), "1558f59");
        assert_eq!(short_hash("abc"), "abc");
    }

    #[test]
    fn a_file_offers_stage_or_unstage_and_commit_follows_the_index() {
        let staged = Selection::File {
            path: "a.txt".to_string(),
            place: FileDisposition::Staged,
        };
        let items = ribbon_commands(&staged, false, false);
        assert_eq!(labels(&items), ["Unstage", "Discard", "Commit"]);
        assert!(!items[2].enabled);
        assert!(items[2].primary);

        let ready = ribbon_commands(&staged, true, false);
        assert!(ready[2].enabled);

        let unstaged = Selection::File {
            path: "b.txt".to_string(),
            place: FileDisposition::Unstaged,
        };
        assert_eq!(ribbon_commands(&unstaged, true, false)[0].label, "Stage");
        assert_eq!(context_label(&unstaged), "Changes");
    }

    #[test]
    fn a_missing_file_falls_back_to_the_current_branch() {
        let stored = Selection::File {
            path: "gone.txt".to_string(),
            place: FileDisposition::Unstaged,
        };
        let branches = vec![
            branch("dev", false, true),
            branch("origin/dev", true, false),
        ];
        let visible = visible_selection(
            Some(&stored),
            &branches,
            &[commit("abc1234")],
            &[],
            &[],
            Some(&status(&[], &["keep.txt"], &[])),
        );
        assert_eq!(
            visible,
            Selection::Branch {
                name: "dev".to_string(),
                current: true,
            }
        );
    }

    #[test]
    fn a_branch_selection_learns_whether_it_is_current() {
        let stored = Selection::Branch {
            name: "main".to_string(),
            current: false,
        };
        let branches = vec![branch("main", false, true)];
        let visible = visible_selection(Some(&stored), &branches, &[], &[], &[], None);
        assert_eq!(
            visible,
            Selection::Branch {
                name: "main".to_string(),
                current: true,
            }
        );
    }

    #[test]
    fn tracked_changes_enable_stash_on_the_branch_ribbon() {
        let selection = Selection::Branch {
            name: "dev".to_string(),
            current: true,
        };
        let items = ribbon_commands(&selection, false, true);
        assert!(items[5].enabled);
        assert_eq!(items[5].command, RibbonCommand::Stash);
    }

    #[test]
    fn a_stash_offers_apply_pop_and_drop() {
        let selection = Selection::Stash {
            index: 2,
            message: "WIP on dev".to_string(),
        };
        let items = ribbon_commands(&selection, false, false);
        assert_eq!(labels(&items), ["Apply", "Pop", "Drop"]);
        assert!(items.iter().all(|item| item.enabled && !item.primary));
        assert_eq!(context_label(&selection), "stash@{2}");
    }

    #[test]
    fn a_stash_whose_slot_moved_falls_back_to_the_branch() {
        let stored = Selection::Stash {
            index: 0,
            message: "WIP on dev".to_string(),
        };
        let stashes = vec![Stash {
            index: 0,
            message: "On main: other".to_string(),
        }];
        let branches = vec![branch("dev", false, true)];
        let visible = visible_selection(Some(&stored), &branches, &[], &stashes, &[], None);
        assert_eq!(
            visible,
            Selection::Branch {
                name: "dev".to_string(),
                current: true,
            }
        );
    }

    #[test]
    fn a_tag_shows_its_name_and_no_commands() {
        let selection = Selection::Tag {
            name: "v1".to_string(),
            commit_id: "abc1234".to_string(),
        };
        assert!(ribbon_commands(&selection, false, false).is_empty());
        assert_eq!(context_label(&selection), "v1");
    }

    #[test]
    fn a_tag_learns_a_moved_commit_and_a_missing_tag_falls_back() {
        let stored = Selection::Tag {
            name: "v1".to_string(),
            commit_id: "old".to_string(),
        };
        let tags = vec![Tag {
            name: "v1".to_string(),
            commit_id: "abc1234".to_string(),
        }];
        let branches = vec![branch("dev", false, true)];
        assert_eq!(
            visible_selection(Some(&stored), &branches, &[], &[], &tags, None),
            Selection::Tag {
                name: "v1".to_string(),
                commit_id: "abc1234".to_string(),
            }
        );
        let gone = Selection::Tag {
            name: "v0".to_string(),
            commit_id: "abc1234".to_string(),
        };
        assert_eq!(
            visible_selection(Some(&gone), &branches, &[], &[], &tags, None),
            Selection::Branch {
                name: "dev".to_string(),
                current: true,
            }
        );
    }
}
