//! Branch and tag labels drawn beside the commit that is their tip.

use crate::icons::Icon;
use core_domain::{Branch, Tag, Worktree};

/// How a ref pill is coloured and marked in the graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RefPillKind {
    /// Current checkout.
    Head,
    /// Local branch checked out in a linked worktree.
    Worktree,
    Local,
    Remote,
    Tag,
}

/// One branch or tag that tips at a commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RefPill {
    /// What the pill shows: the full name, never shortened.
    pub label: String,
    /// The full ref name, for the hover text.
    pub name: String,
    pub kind: RefPillKind,
    pub ahead: usize,
    pub behind: usize,
    /// The worktree that has this branch checked out: the current one for the HEAD pill,
    /// another for a worktree pill. Its colour is the pill's.
    pub worktree: Option<String>,
}


/// Icon shown inside the pill for the ref kind.
pub(crate) fn pill_mark(kind: RefPillKind) -> Icon {
    match kind {
        RefPillKind::Head | RefPillKind::Local => Icon::Branch,
        RefPillKind::Worktree => Icon::Worktree,
        RefPillKind::Remote => Icon::Cloud,
        RefPillKind::Tag => Icon::Tag,
    }
}

/// Refs whose tip is `commit_id`, ordered head → worktree → local → remote → tag.
///
/// A local branch whose name appears in `worktrees` is classified as [`RefPillKind::Worktree`]
/// (unless it is HEAD, which stays Head). `current` is the folder whose history this is: the
/// HEAD pill belongs to it.
pub(crate) fn refs_at(
    commit_id: &str,
    branches: &[Branch],
    tags: &[Tag],
    worktrees: &[Worktree],
    current: &str,
) -> Vec<RefPill> {
    let checked_out_in = |name: &str| {
        worktrees
            .iter()
            .find(|wt| !wt.branch.is_empty() && wt.branch == name)
            .map(|wt| wt.path.clone())
    };

    let mut pills = Vec::new();
    for branch in branches {
        if branch.tip != commit_id {
            continue;
        }
        let elsewhere = (!branch.is_head && !branch.is_remote)
            .then(|| checked_out_in(&branch.name))
            .flatten();
        let (kind, worktree) = if branch.is_head {
            (RefPillKind::Head, Some(current.to_string()))
        } else if branch.is_remote {
            (RefPillKind::Remote, None)
        } else if elsewhere.is_some() {
            (RefPillKind::Worktree, elsewhere)
        } else {
            (RefPillKind::Local, None)
        };
        pills.push(RefPill {
            label: branch.name.clone(),
            name: branch.name.clone(),
            kind,
            ahead: branch.ahead,
            behind: branch.behind,
            worktree,
        });
    }
    for tag in tags {
        if tag.commit_id == commit_id {
            pills.push(RefPill {
                label: tag.name.clone(),
                name: tag.name.clone(),
                kind: RefPillKind::Tag,
                ahead: 0,
                behind: 0,
                worktree: None,
            });
        }
    }
    pills.sort_by_key(|pill| match pill.kind {
        RefPillKind::Head => 0,
        RefPillKind::Worktree => 1,
        RefPillKind::Local => 2,
        RefPillKind::Remote => 3,
        RefPillKind::Tag => 4,
    });
    pills
}

/// Hover text for a commit's pills: every ref at the commit, one per line, by full name,
/// with the worktree folder where one has the branch checked out. A clipped pill or a
/// "+N" never hides a name.
pub(crate) fn pill_tooltip(
    pills: &[RefPill],
    worktrees: &[Worktree],
    folder_of: &dyn Fn(&str) -> String,
) -> String {
    pills
        .iter()
        .map(|pill| {
            let kind = match pill.kind {
                RefPillKind::Head => "current branch",
                RefPillKind::Worktree => "branch in a worktree",
                RefPillKind::Local => "branch",
                RefPillKind::Remote => "remote branch",
                RefPillKind::Tag => "tag",
            };
            let checked_out = matches!(pill.kind, RefPillKind::Head | RefPillKind::Worktree)
                .then(|| worktrees.iter().find(|entry| entry.branch == pill.name))
                .flatten()
                .map(|entry| format!(" · worktree {}", folder_of(&entry.path)))
                .unwrap_or_default();
            format!("{} ({kind}){checked_out}", pill.name)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The worktree whose tip this commit is, if any: the current one's before another's. Its
/// colour is the commit's dot and the line below it.
pub(crate) fn tip_worktree(pills: &[RefPill]) -> Option<&str> {
    pills
        .iter()
        .filter(|pill| matches!(pill.kind, RefPillKind::Head | RefPillKind::Worktree))
        .find_map(|pill| pill.worktree.as_deref())
}

/// Dominant tip kind for canvas node styling (local/worktree beat remote).
pub(crate) fn primary_node_kind(pills: &[RefPill]) -> Option<RefPillKind> {
    pills
        .iter()
        .map(|pill| pill.kind)
        .min_by_key(|kind| match kind {
            RefPillKind::Head => 0,
            RefPillKind::Worktree => 1,
            RefPillKind::Local => 2,
            RefPillKind::Remote => 3,
            RefPillKind::Tag => 4,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn branch(name: &str, tip: &str, is_remote: bool, is_head: bool) -> Branch {
        Branch {
            name: name.to_string(),
            is_remote,
            is_head,
            tip: tip.to_string(),
            ahead: 0,
            behind: 0,
        }
    }

    fn wt(path: &str, branch: &str) -> Worktree {
        Worktree {
            path: path.into(),
            branch: branch.into(),
            locked: false,
            is_main: false,
        }
    }

    #[test]
    fn refs_at_collects_tips_in_kind_order() {
        let branches = vec![
            branch("origin/main", "abc", true, false),
            branch("main", "abc", false, true),
            branch("feature/x", "abc", false, false),
            branch("other", "zzz", false, false),
        ];
        let tags = vec![
            Tag {
                name: "v1".into(),
                commit_id: "abc".into(),
            },
            Tag {
                name: "old".into(),
                commit_id: "nope".into(),
            },
        ];
        let mut branches = branches;
        branches[1].ahead = 2;
        branches[1].behind = 1;
        let pills = refs_at("abc", &branches, &tags, &[], "/r");
        let labels: Vec<_> = pills
            .iter()
            .map(|p| (p.label.as_str(), p.kind, p.ahead, p.behind))
            .collect();
        assert_eq!(
            labels,
            vec![
                ("main", RefPillKind::Head, 2, 1),
                ("feature/x", RefPillKind::Local, 0, 0),
                ("origin/main", RefPillKind::Remote, 0, 0),
                ("v1", RefPillKind::Tag, 0, 0),
            ]
        );
    }

    #[test]
    fn local_checked_out_elsewhere_is_worktree() {
        let branches = vec![branch("feature/x", "abc", false, false)];
        let pills = refs_at(
            "abc",
            &branches,
            &[],
            &[wt(r"D:\repo-feature", "feature/x")],
            r"D:\repo",
        );
        assert_eq!(pills[0].kind, RefPillKind::Worktree);
        assert_eq!(pills[0].worktree.as_deref(), Some(r"D:\repo-feature"));
    }

    #[test]
    fn the_head_pill_belongs_to_the_current_folder_and_names_the_tip_worktree() {
        let branches = vec![
            branch("feature/x", "abc", false, false),
            branch("main", "abc", false, true),
            branch("origin/main", "abc", true, false),
        ];
        let pills = refs_at("abc", &branches, &[], &[wt("/wt/x", "feature/x")], "/repo");
        assert_eq!(pills[0].worktree.as_deref(), Some("/repo"), "HEAD sorts first and is this folder's");
        assert_eq!(tip_worktree(&pills), Some("/repo"));
        assert_eq!(tip_worktree(&pills[1..]), Some("/wt/x"));
        assert_eq!(tip_worktree(&pills[2..]), None, "a remote is no worktree's");
    }

    #[test]
    fn the_tooltip_names_every_ref_in_full_with_its_worktree_folder() {
        let long = "feature/analyse-changes-done-pr";
        let branches = vec![
            branch("main", "abc", false, true),
            branch(long, "abc", false, false),
            branch("origin/main", "abc", true, false),
        ];
        let worktrees = vec![wt("D:/repo", "main"), wt("D:/wt/Knowl-analy-3bd5e", long)];
        let pills = refs_at("abc", &branches, &[], &worktrees, "D:/repo");
        let folder = |path: &str| crate::session::basename(path).to_string();

        let tip = pill_tooltip(&pills, &worktrees, &folder);

        assert_eq!(
            tip,
            "main (current branch) · worktree repo\n\
             feature/analyse-changes-done-pr (branch in a worktree) · worktree Knowl-analy-3bd5e\n\
             origin/main (remote branch)"
        );
    }

    #[test]
    fn long_ref_names_are_shown_whole() {
        let long = "feature/very-long-branch-name-that-overflows-the-pill-gutter-limit";
        let pills = refs_at("id", &[branch(long, "id", false, false)], &[], &[], "/r");
        assert_eq!(pills[0].label, long, "a cut name could read as another branch");
        assert_eq!(pills[0].name, long);
    }
}
