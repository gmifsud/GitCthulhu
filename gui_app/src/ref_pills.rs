//! Branch and tag labels drawn on the commit that is their tip.

use core_domain::{Branch, Tag};

/// How a ref pill is coloured in the graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RefPillKind {
    /// Current checkout.
    Head,
    Local,
    Remote,
    Tag,
}

/// One branch or tag that tips at a commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RefPill {
    pub label: String,
    pub kind: RefPillKind,
}

const PILL_LABEL_MAX: usize = 22;

/// Refs whose tip is `commit_id`, ordered head → local → remote → tag.
pub(crate) fn refs_at(commit_id: &str, branches: &[Branch], tags: &[Tag]) -> Vec<RefPill> {
    let mut pills = Vec::new();
    for branch in branches {
        if branch.tip != commit_id {
            continue;
        }
        let kind = if branch.is_head {
            RefPillKind::Head
        } else if branch.is_remote {
            RefPillKind::Remote
        } else {
            RefPillKind::Local
        };
        pills.push(RefPill {
            label: shorten_ref(&branch.name, PILL_LABEL_MAX),
            kind,
        });
    }
    for tag in tags {
        if tag.commit_id == commit_id {
            pills.push(RefPill {
                label: shorten_ref(&tag.name, PILL_LABEL_MAX),
                kind: RefPillKind::Tag,
            });
        }
    }
    pills.sort_by_key(|pill| match pill.kind {
        RefPillKind::Head => 0,
        RefPillKind::Local => 1,
        RefPillKind::Remote => 2,
        RefPillKind::Tag => 3,
    });
    pills
}

fn shorten_ref(name: &str, max: usize) -> String {
    let count = name.chars().count();
    if count <= max {
        return name.to_string();
    }
    let keep = max.saturating_sub(1);
    format!("{}…", name.chars().take(keep).collect::<String>())
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
        let pills = refs_at("abc", &branches, &tags);
        let labels: Vec<_> = pills.iter().map(|p| (p.label.as_str(), p.kind)).collect();
        assert_eq!(
            labels,
            vec![
                ("main", RefPillKind::Head),
                ("feature/x", RefPillKind::Local),
                ("origin/main", RefPillKind::Remote),
                ("v1", RefPillKind::Tag),
            ]
        );
    }

    #[test]
    fn long_ref_names_are_ellipsised() {
        let long = "feature/very-long-branch-name-that-overflows";
        let pills = refs_at(
            "id",
            &[branch(long, "id", false, false)],
            &[],
        );
        assert_eq!(pills[0].label.chars().count(), PILL_LABEL_MAX);
        assert!(pills[0].label.ends_with('…'));
    }
}
