use std::collections::BTreeMap;
use std::convert::Infallible;
use std::sync::atomic::AtomicBool;

use core_domain::{ChangeKind, DomainError, RepoStatus};
use gix::bstr::{BStr, BString};
use gix::status::index_worktree::iter::Summary;
use gix::status::plumbing::index_as_worktree::traits::{FastEq, SubmoduleStatus};
use gix::status::plumbing::index_as_worktree_with_renames::{Entry as StatusEntry, VisitEntry};

use super::{unknown, unknown_with_causes};

/// Stage-0 index entries keyed by path, for comparing two indices.
fn entries_by_path(
    index: &gix::index::State,
) -> BTreeMap<BString, (gix::ObjectId, gix::index::entry::Mode)> {
    index
        .entries()
        .iter()
        .filter(|entry| entry.stage() == gix::index::entry::Stage::Unconflicted)
        .map(|entry| (entry.path(index).to_owned(), (entry.id, entry.mode)))
        .collect()
}

/// Paths whose index entry differs from `HEAD` — what `git status` calls "Changes to be committed" —
/// each with how it differs. gix 0.66 has no tree-to-index status, so the two are compared directly.
///
/// Also returns the conflicted paths (index entries at stages 1–3). They are left out of the
/// staged map: their stage-0 entry is missing, which would otherwise read as a deletion.
pub(crate) fn staged_changes(
    repo: &gix::Repository,
    head_is_unborn: bool,
) -> Result<(BTreeMap<String, ChangeKind>, Vec<String>), DomainError> {
    let index = repo.index_or_empty().map_err(unknown)?;
    let head_entries = if head_is_unborn {
        BTreeMap::new()
    } else {
        let head_tree = repo.head_tree_id().map_err(unknown_with_causes)?;
        let head_index = repo.index_from_tree(&head_tree).map_err(unknown_with_causes)?;
        entries_by_path(&head_index)
    };
    let index_entries = entries_by_path(&index);
    let mut conflicted: Vec<String> = index
        .entries()
        .iter()
        .filter(|entry| entry.stage() != gix::index::entry::Stage::Unconflicted)
        .map(|entry| entry.path(&index).to_string())
        .collect();
    conflicted.dedup();

    let staged = index_entries
        .keys()
        .chain(head_entries.keys())
        .filter_map(|path| {
            let kind = match (head_entries.get(path), index_entries.get(path)) {
                (None, Some(_)) => ChangeKind::Added,
                (Some(_), None) => ChangeKind::Deleted,
                (head, index) if head != index => ChangeKind::Modified,
                _ => return None,
            };
            Some((path.to_string(), kind))
        })
        .filter(|(path, _)| !conflicted.contains(path))
        .collect();
    Ok((staged, conflicted))
}

/// File one index ↔ worktree change under the matching `RepoStatus` list.
pub(crate) fn record_change(status: &mut RepoStatus, summary: Summary, path: String) {
    let kind = match summary {
        Summary::Added => {
            status.untracked_files.push(path);
            return;
        }
        Summary::Removed => ChangeKind::Deleted,
        Summary::Renamed | Summary::Copied => ChangeKind::Renamed,
        Summary::IntentToAdd => ChangeKind::Added,
        // Listed from the index in `staged_changes`, not as an unstaged change.
        Summary::Conflict => return,
        Summary::Modified | Summary::TypeChange => ChangeKind::Modified,
    };
    status.kinds.unstaged.insert(path.clone(), kind);
    status.unstaged_files.push(path);
}

/// Submodule checker that never reports a change.
///
/// Only used while `HEAD` is unborn: gix 0.66's built-in checker looks up `.gitmodules`
/// in the `HEAD` commit and fails when there is none. With no commits there are no
/// committed submodules to report anyway.
#[derive(Clone)]
struct NoSubmodules;

impl SubmoduleStatus for NoSubmodules {
    type Output = ();
    type Error = Infallible;

    fn status(
        &mut self,
        _entry: &gix::index::Entry,
        _rela_path: &BStr,
    ) -> Result<Option<()>, Infallible> {
        Ok(None)
    }
}

/// Collects the summary of every change reported by `index_worktree_status`.
struct CollectChanges(Vec<(Summary, String)>);

impl<'index> VisitEntry<'index> for CollectChanges {
    type ContentChange = ();
    type SubmoduleStatus = ();

    fn visit_entry(&mut self, entry: StatusEntry<'index, (), ()>) {
        if let Some(summary) = entry.summary() {
            self.0
                .push((summary, entry.destination_rela_path().to_string()));
        }
    }
}

/// Index ↔ worktree changes for a repository whose `HEAD` is unborn.
/// Same comparison as `Repository::status()`, minus the submodule lookup that fails there.
pub(crate) fn unborn_worktree_changes(
    repo: &gix::Repository,
) -> Result<Vec<(Summary, String)>, DomainError> {
    let index = repo.index_or_empty().map_err(unknown)?;
    let options = gix::status::index_worktree::Options {
        dirwalk_options: Some(
            repo.dirwalk_options()
                .map_err(unknown)?
                .emit_untracked(gix::dir::walk::EmissionMode::Matching),
        ),
        ..Default::default()
    };
    let mut changes = CollectChanges(Vec::new());
    repo.index_worktree_status(
        &index,
        Vec::<BString>::new(),
        &mut changes,
        FastEq,
        NoSubmodules,
        &mut gix::progress::Discard,
        &AtomicBool::new(false),
        options,
    )
    .map_err(unknown)?;
    Ok(changes.0)
}

/// Untracked files read for a line count, at most; the rest show no count.
const UNTRACKED_COUNT_LIMIT: usize = 500;
/// Untracked files above this size show no count.
const UNTRACKED_COUNT_MAX_BYTES: u64 = 1024 * 1024;
/// A NUL in this many leading bytes marks a file as binary, as git decides it.
const BINARY_SNIFF_BYTES: usize = 8_000;

/// Parse `git diff --numstat -z`: `added<TAB>removed<TAB>path<NUL>`, binary files as `-<TAB>-`.
/// A rename or copy has an empty path field, then the old and the new path as two more records;
/// the count belongs to the new path.
pub(crate) fn parse_numstat_z(text: &str) -> BTreeMap<String, Option<(u32, u32)>> {
    let mut counts = BTreeMap::new();
    let mut records = text.split('\0');
    while let Some(record) = records.next() {
        let mut fields = record.splitn(3, '\t');
        let (Some(added), Some(removed), Some(path)) = (fields.next(), fields.next(), fields.next()) else {
            continue;
        };
        let count = added.parse().ok().zip(removed.parse().ok());
        let path = if path.is_empty() {
            let _old = records.next();
            match records.next() {
                Some(new) if !new.is_empty() => new,
                _ => continue,
            }
        } else {
            path
        };
        counts.insert(path.to_string(), count);
    }
    counts
}

/// The empty tree, which the index is compared with while `HEAD` is unborn.
const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

/// Added and removed lines for the staged changes: the index against `HEAD` (the empty tree
/// while `HEAD` is unborn). Plumbing, so a background reload never refreshes `.git/index` or
/// takes `index.lock` the way porcelain `git diff` does when stat data is stale. A failed
/// read counts nothing rather than failing every count.
pub(crate) fn staged_line_counts(repo_path: &str, head_is_born: bool) -> BTreeMap<String, Option<(u32, u32)>> {
    let base = if head_is_born { "HEAD" } else { EMPTY_TREE };
    numstat_or_empty(
        repo_path,
        &["diff-index", "--cached", "--numstat", "-z", "-M", "--no-textconv", "--no-ext-diff", base],
    )
}

/// Added and removed lines for the unstaged changes: the worktree against the index, by
/// plumbing for the same reason as [`staged_line_counts`].
pub(crate) fn unstaged_line_counts(repo_path: &str) -> BTreeMap<String, Option<(u32, u32)>> {
    numstat_or_empty(repo_path, &["diff-files", "--numstat", "-z", "--no-textconv", "--no-ext-diff"])
}

fn numstat_or_empty(repo_path: &str, args: &[&str]) -> BTreeMap<String, Option<(u32, u32)>> {
    super::command::run_git_stdout(repo_path, args)
        .map(|text| parse_numstat_z(&text))
        .unwrap_or_default()
}

/// Line count of an untracked file: its lines (a last line without a newline counts), or `None`
/// when it is binary, over [`UNTRACKED_COUNT_MAX_BYTES`] or cannot be read.
fn untracked_line_count(path: &std::path::Path) -> Option<(u32, u32)> {
    let metadata = std::fs::symlink_metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > UNTRACKED_COUNT_MAX_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    if bytes[..bytes.len().min(BINARY_SNIFF_BYTES)].contains(&0) {
        return None;
    }
    let newlines = bytes.iter().filter(|&&byte| byte == b'\n').count();
    let unterminated = usize::from(bytes.last().is_some_and(|&byte| byte != b'\n'));
    Some((u32::try_from(newlines + unterminated).ok()?, 0))
}

/// Line counts for `untracked` (as listed by status), counting the first
/// [`UNTRACKED_COUNT_LIMIT`] files; later ones have no count.
pub(crate) fn untracked_line_counts(
    workdir: &std::path::Path,
    untracked: &[String],
) -> BTreeMap<String, Option<(u32, u32)>> {
    untracked
        .iter()
        .enumerate()
        .map(|(position, path)| {
            let count = (position < UNTRACKED_COUNT_LIMIT)
                .then(|| untracked_line_count(&workdir.join(path)))
                .flatten();
            (path.clone(), count)
        })
        .collect()
}

/// Ahead/behind for `HEAD`, using the same `@{upstream}` git uses for push and pull.
///
/// A missing upstream, a detached `HEAD`, and an unborn branch are `Ok(None)`.
pub(crate) fn tracking_status(
    repo_path: &str,
) -> Result<Option<core_domain::TrackingStatus>, DomainError> {
    let upstream = match super::command::run_git_stdout(
        repo_path,
        &["rev-parse", "--abbrev-ref", "@{upstream}"],
    ) {
        Ok(name) if !name.is_empty() => name,
        Ok(_) => return Ok(None),
        Err(DomainError::Unknown(message)) if upstream_is_absent(&message) => return Ok(None),
        Err(error) => return Err(error),
    };
    let counts = super::command::run_git_stdout(
        repo_path,
        &["rev-list", "--left-right", "--count", "HEAD...@{upstream}"],
    )?;
    let (ahead, behind) = parse_ahead_behind(&counts)?;
    Ok(Some(core_domain::TrackingStatus {
        upstream,
        ahead,
        behind,
    }))
}

fn upstream_is_absent(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    message.contains("no upstream")
        || message.contains("no such branch")
        || message.contains("does not point to a branch")
        || message.contains("unknown revision")
        || message.contains("needed a single revision")
        || message.contains("ambiguous argument")
}

fn parse_ahead_behind(text: &str) -> Result<(usize, usize), DomainError> {
    let mut parts = text.split_whitespace();
    match (
        parts.next().and_then(|part| part.parse().ok()),
        parts.next().and_then(|part| part.parse().ok()),
    ) {
        (Some(ahead), Some(behind)) => Ok((ahead, behind)),
        _ => Err(DomainError::Unknown(format!(
            "unexpected ahead/behind counts: {text}"
        ))),
    }
}

/// Ahead/behind for every local branch that has an upstream, via `for-each-ref`.
///
/// Keys are short branch names (`main`, not `refs/heads/main`). Remotes are omitted.
/// Uses `|` as separator (more reliable than a literal tab on Windows shells).
pub(crate) fn local_branch_tracking(
    repo_path: &str,
) -> Result<BTreeMap<String, (usize, usize)>, DomainError> {
    let text = match super::command::run_git_stdout(
        repo_path,
        &[
            "for-each-ref",
            "--format=%(refname:short)|%(upstream:track)",
            "refs/heads",
        ],
    ) {
        Ok(text) => text,
        Err(DomainError::Unknown(_)) => return Ok(BTreeMap::new()),
        Err(error) => return Err(error),
    };
    let mut map = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim().trim_end_matches('\r');
        let Some((name, track)) = line.split_once('|') else {
            continue;
        };
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        if let Some((ahead, behind)) = parse_upstream_track(track) {
            map.insert(name.to_string(), (ahead, behind));
        }
    }
    Ok(map)
}

/// Parse `%(upstream:track)` forms like `[ahead 1, behind 2]`, `[ahead 3]`, `[behind 1]`.
fn parse_upstream_track(track: &str) -> Option<(usize, usize)> {
    let track = track.trim();
    if track.is_empty() || track.eq_ignore_ascii_case("[gone]") {
        return None;
    }
    let inner = track
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .unwrap_or(track);
    let mut ahead = 0usize;
    let mut behind = 0usize;
    let mut saw = false;
    for part in inner.split(',') {
        let part = part.trim();
        if let Some(n) = part
            .strip_prefix("ahead ")
            .and_then(|n| n.trim().parse().ok())
        {
            ahead = n;
            saw = true;
        } else if let Some(n) = part
            .strip_prefix("behind ")
            .and_then(|n| n.trim().parse().ok())
        {
            behind = n;
            saw = true;
        }
    }
    saw.then_some((ahead, behind))
}

#[cfg(test)]
mod track_tests {
    use super::parse_upstream_track;

    #[test]
    fn parse_upstream_track_forms() {
        assert_eq!(parse_upstream_track("[ahead 1, behind 2]"), Some((1, 2)));
        assert_eq!(parse_upstream_track("[ahead 3]"), Some((3, 0)));
        assert_eq!(parse_upstream_track("[behind 4]"), Some((0, 4)));
        assert_eq!(parse_upstream_track(""), None);
        assert_eq!(parse_upstream_track("[gone]"), None);
    }
}

#[cfg(test)]
mod numstat_tests {
    use super::parse_numstat_z;

    #[test]
    fn numstat_z_reads_counts_binary_and_renames() {
        let text = "3\t1\ta.txt\0-\t-\timg.png\x002\t0\t\0old.rs\0new.rs\0";
        let counts = parse_numstat_z(text);
        assert_eq!(counts["a.txt"], Some((3, 1)));
        assert_eq!(counts["img.png"], None);
        assert_eq!(counts["new.rs"], Some((2, 0)));
        assert!(!counts.contains_key("old.rs"));
    }
}
