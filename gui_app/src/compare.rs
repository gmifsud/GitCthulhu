//! The Compare window: one file, two versions, side by side.
//!
//! The same window is the conflict view. For a conflicted file it opens on yours (`:2`) against
//! theirs (`:3`) and offers to take a side; for any other file it compares the working tree
//! with a branch, tag or commit.

use core_domain::{Branch, DiffLine, DiffLineKind, Hunk, Patch, RepoStatus, Tag};

pub(crate) const STAGE_BASE: &str = ":1";
pub(crate) const STAGE_YOURS: &str = ":2";
pub(crate) const STAGE_THEIRS: &str = ":3";
pub(crate) const HEAD: &str = "HEAD";

/// One side of a compare.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum CompareSide {
    /// The file on disk, uncommitted edits included.
    WorkingTree,
    /// A branch, tag, commit, or index stage of a conflict.
    Revision(String),
}

impl std::fmt::Display for CompareSide {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompareSide::WorkingTree => f.write_str("Working tree"),
            CompareSide::Revision(name) => match name.as_str() {
                STAGE_BASE => f.write_str("Base (common ancestor)"),
                STAGE_YOURS => f.write_str("Yours (current branch)"),
                STAGE_THEIRS => f.write_str("Theirs (incoming)"),
                other => f.write_str(other),
            },
        }
    }
}

/// The open Compare window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompareState {
    /// The tab it was opened from; results for another tab are dropped.
    pub session_path: String,
    pub path: String,
    pub left: CompareSide,
    pub right: CompareSide,
    /// Typed revision under each side, applied with Enter.
    pub left_draft: String,
    pub right_draft: String,
    pub generation: u64,
    pub loading: bool,
    pub result: Option<Result<Patch, String>>,
}

impl CompareState {
    /// A conflicted file opens on yours against theirs; anything else on HEAD against the disk.
    pub(crate) fn open(session_path: String, path: String, conflicted: bool, against: Option<String>) -> Self {
        let (left, right) = match (against, conflicted) {
            (Some(revision), _) => (CompareSide::Revision(revision), CompareSide::WorkingTree),
            (None, true) => (
                CompareSide::Revision(STAGE_YOURS.to_string()),
                CompareSide::Revision(STAGE_THEIRS.to_string()),
            ),
            (None, false) => (CompareSide::Revision(HEAD.to_string()), CompareSide::WorkingTree),
        };
        Self {
            session_path,
            path,
            left,
            right,
            left_draft: String::new(),
            right_draft: String::new(),
            generation: 0,
            loading: false,
            result: None,
        }
    }

    pub(crate) fn swap(&mut self) {
        std::mem::swap(&mut self.left, &mut self.right);
        std::mem::swap(&mut self.left_draft, &mut self.right_draft);
    }
}

/// What to ask git for: `(left, right, reversed)`. Git always needs a revision on the left,
/// so working tree on the left asks the other way round and the patch is reversed afterwards.
/// `None` when both sides are the working tree.
pub(crate) fn query(left: &CompareSide, right: &CompareSide) -> Option<(String, Option<String>, bool)> {
    match (left, right) {
        (CompareSide::Revision(l), CompareSide::WorkingTree) => Some((l.clone(), None, false)),
        (CompareSide::Revision(l), CompareSide::Revision(r)) => Some((l.clone(), Some(r.clone()), false)),
        (CompareSide::WorkingTree, CompareSide::Revision(r)) => Some((r.clone(), None, true)),
        (CompareSide::WorkingTree, CompareSide::WorkingTree) => None,
    }
}

/// Every side the pickers offer: the working tree, HEAD, the conflict stages when the file
/// is conflicted, then local branches, remote branches and tags.
pub(crate) fn side_choices(branches: &[Branch], tags: &[Tag], conflicted: bool) -> Vec<CompareSide> {
    let mut choices = vec![CompareSide::WorkingTree, CompareSide::Revision(HEAD.to_string())];
    if conflicted {
        for stage in [STAGE_YOURS, STAGE_THEIRS, STAGE_BASE] {
            choices.push(CompareSide::Revision(stage.to_string()));
        }
    }
    let locals = branches.iter().filter(|branch| !branch.is_remote);
    let remotes = branches.iter().filter(|branch| branch.is_remote);
    for branch in locals.chain(remotes) {
        let side = CompareSide::Revision(branch.name.clone());
        if !choices.contains(&side) {
            choices.push(side);
        }
    }
    choices.extend(tags.iter().map(|tag| CompareSide::Revision(tag.name.clone())));
    choices
}

/// Changed files, conflicts first, without repeats: what the file picker lists.
pub(crate) fn changed_paths(status: Option<&RepoStatus>) -> Vec<String> {
    let Some(status) = status else {
        return Vec::new();
    };
    let mut paths: Vec<String> = Vec::new();
    for path in status
        .conflicted_files
        .iter()
        .chain(&status.unstaged_files)
        .chain(&status.staged_files)
        .chain(&status.untracked_files)
    {
        if !paths.contains(path) {
            paths.push(path.clone());
        }
    }
    paths
}

pub(crate) fn is_conflicted(status: Option<&RepoStatus>, path: &str) -> bool {
    status.is_some_and(|status| status.conflicted_files.iter().any(|file| file == path))
}

/// The same change seen from the other side: added and removed swap, as do the hunk ranges.
/// Within each run of changes the removed lines go first again, as git prints them.
pub(crate) fn reverse(patch: Patch) -> Patch {
    let hunks = patch
        .hunks
        .into_iter()
        .map(|hunk| Hunk {
            header: reverse_header(&hunk.header),
            lines: reverse_lines(hunk.lines),
        })
        .collect();
    Patch { hunks, ..patch }
}

fn reverse_lines(lines: Vec<DiffLine>) -> Vec<DiffLine> {
    let flip = |line: DiffLine| DiffLine {
        kind: match line.kind {
            DiffLineKind::Added => DiffLineKind::Removed,
            DiffLineKind::Removed => DiffLineKind::Added,
            DiffLineKind::Context => DiffLineKind::Context,
        },
        text: line.text,
    };
    let mut out = Vec::with_capacity(lines.len());
    let mut run: Vec<DiffLine> = Vec::new();
    let flush = |run: &mut Vec<DiffLine>, out: &mut Vec<DiffLine>| {
        let (removed, added): (Vec<_>, Vec<_>) =
            run.drain(..).partition(|line| line.kind == DiffLineKind::Removed);
        out.extend(removed);
        out.extend(added);
    };
    for line in lines.into_iter().map(flip) {
        if line.kind == DiffLineKind::Context {
            flush(&mut run, &mut out);
            out.push(line);
        } else {
            run.push(line);
        }
    }
    flush(&mut run, &mut out);
    out
}

/// `@@ -a,b +c,d @@ tail` → `@@ -c,d +a,b @@ tail`. Anything else is left alone.
fn reverse_header(header: &str) -> String {
    let Some(rest) = header.strip_prefix("@@ -") else {
        return header.to_string();
    };
    let Some((old, rest)) = rest.split_once(" +") else {
        return header.to_string();
    };
    let Some((new, tail)) = rest.split_once(" @@") else {
        return header.to_string();
    };
    format!("@@ -{new} +{old} @@{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(kind: DiffLineKind, text: &str) -> DiffLine {
        DiffLine { kind, text: text.into() }
    }

    #[test]
    fn a_conflict_opens_on_yours_against_theirs() {
        let state = CompareState::open("r".into(), "a.txt".into(), true, None);
        assert_eq!(state.left, CompareSide::Revision(STAGE_YOURS.into()));
        assert_eq!(state.right, CompareSide::Revision(STAGE_THEIRS.into()));
        let plain = CompareState::open("r".into(), "a.txt".into(), false, None);
        assert_eq!(plain.left, CompareSide::Revision(HEAD.into()));
        assert_eq!(plain.right, CompareSide::WorkingTree);
        let against = CompareState::open("r".into(), "a.txt".into(), true, Some("main".into()));
        assert_eq!(against.left, CompareSide::Revision("main".into()));
    }

    #[test]
    fn the_working_tree_on_the_left_asks_git_the_other_way_round() {
        let tree = CompareSide::WorkingTree;
        let main = CompareSide::Revision("main".into());
        assert_eq!(query(&main, &tree), Some(("main".into(), None, false)));
        assert_eq!(query(&tree, &main), Some(("main".into(), None, true)));
        assert_eq!(query(&main, &main), Some(("main".into(), Some("main".into()), false)));
        assert_eq!(query(&tree, &tree), None);
    }

    #[test]
    fn reversing_swaps_kinds_and_ranges_and_keeps_removed_first() {
        let patch = Patch {
            path: "a".into(),
            binary: false,
            hunks: vec![Hunk {
                header: "@@ -1,2 +1,3 @@ fn x".into(),
                lines: vec![
                    line(DiffLineKind::Context, "keep"),
                    line(DiffLineKind::Removed, "old"),
                    line(DiffLineKind::Added, "new"),
                    line(DiffLineKind::Added, "more"),
                ],
            }],
        };
        let reversed = reverse(patch);
        assert_eq!(reversed.hunks[0].header, "@@ -1,3 +1,2 @@ fn x");
        let kinds: Vec<_> = reversed.hunks[0].lines.iter().map(|l| (l.kind, l.text.as_str())).collect();
        assert_eq!(
            kinds,
            vec![
                (DiffLineKind::Context, "keep"),
                (DiffLineKind::Removed, "new"),
                (DiffLineKind::Removed, "more"),
                (DiffLineKind::Added, "old"),
            ]
        );
    }

    #[test]
    fn choices_list_stages_only_for_a_conflict_and_never_repeat() {
        let branch = |name: &str, remote: bool| Branch {
            name: name.into(),
            is_remote: remote,
            is_head: false,
            tip: String::new(),
            ahead: 0,
            behind: 0,
        };
        let branches = vec![branch("main", false), branch("origin/main", true), branch("HEAD", false)];
        let tags = vec![Tag { name: "v1".into(), commit_id: String::new() }];
        let plain = side_choices(&branches, &tags, false);
        assert_eq!(
            plain.iter().map(ToString::to_string).collect::<Vec<_>>(),
            vec!["Working tree", "HEAD", "main", "origin/main", "v1"]
        );
        let conflicted = side_choices(&branches, &tags, true);
        assert!(conflicted.contains(&CompareSide::Revision(STAGE_THEIRS.into())));
    }

    #[test]
    fn changed_paths_put_conflicts_first_once() {
        let status = RepoStatus {
            staged_files: vec!["b".into()],
            unstaged_files: vec!["b".into(), "c".into()],
            untracked_files: vec!["d".into()],
            tracking: None,
            head_commit_id: None,
            kinds: Default::default(),
            operation: None,
            conflicted_files: vec!["a".into()],
        };
        assert_eq!(changed_paths(Some(&status)), vec!["a", "b", "c", "d"]);
        assert!(is_conflicted(Some(&status), "a"));
        assert!(!is_conflicted(Some(&status), "b"));
    }
}
