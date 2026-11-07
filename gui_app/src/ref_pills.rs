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
    /// What the pill shows: the name, ellipsised past [`PILL_LABEL_MAX`].
    pub label: String,
    /// The full ref name, for the hover text.
    pub name: String,
    pub kind: RefPillKind,
    pub ahead: usize,
    pub behind: usize,
}

/// Long enough for typical `feature/…` names; still caps pathological refs.
const PILL_LABEL_MAX: usize = 44;

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
/// (unless it is HEAD, which stays Head).
pub(crate) fn refs_at(
    commit_id: &str,
    branches: &[Branch],
    tags: &[Tag],
    worktrees: &[Worktree],
) -> Vec<RefPill> {
    let worktree_branches: std::collections::HashSet<&str> = worktrees
        .iter()
        .filter(|wt| !wt.branch.is_empty())
        .map(|wt| wt.branch.as_str())
        .collect();

    let mut pills = Vec::new();
    for branch in branches {
        if branch.tip != commit_id {
            continue;
        }
        let kind = if branch.is_head {
            RefPillKind::Head
        } else if branch.is_remote {
            RefPillKind::Remote
        } else if worktree_branches.contains(branch.name.as_str()) {
            RefPillKind::Worktree
        } else {
            RefPillKind::Local
        };
        pills.push(RefPill {
            label: shorten_ref(&branch.name, PILL_LABEL_MAX),
            name: branch.name.clone(),
            kind,
            ahead: branch.ahead,
            behind: branch.behind,
        });
    }
    for tag in tags {
        if tag.commit_id == commit_id {
            pills.push(RefPill {
                label: shorten_ref(&tag.name, PILL_LABEL_MAX),
                name: tag.name.clone(),
                kind: RefPillKind::Tag,
                ahead: 0,
                behind: 0,
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

fn shorten_ref(name: &str, max: usize) -> String {
    crate::wip::middle_ellipsis(name, max)
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
        let pills = refs_at("abc", &branches, &tags, &[]);
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
        );
        assert_eq!(pills[0].kind, RefPillKind::Worktree);
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
        let pills = refs_at("abc", &branches, &[], &worktrees);
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
    fn long_ref_names_are_ellipsised() {
        let long = "feature/very-long-branch-name-that-overflows-the-pill-gutter-limit";
        let pills = refs_at("id", &[branch(long, "id", false, false)], &[], &[]);
        assert_eq!(pills[0].label.chars().count(), PILL_LABEL_MAX);
        assert!(pills[0].label.contains('…'), "shortened in the middle");
        assert!(pills[0].label.starts_with("feature/"), "the start stays");
        assert!(pills[0].label.ends_with("gutter-limit"), "and so does the end");
        assert_eq!(pills[0].name, long, "the full name stays for the hover text");
    }
}
