use std::collections::BTreeMap;
use std::convert::Infallible;
use std::sync::atomic::AtomicBool;

use core_domain::{ChangeKind, DomainError, RepoStatus};
use gix::bstr::{BStr, BString};
use gix::status::index_worktree::iter::Summary;
use gix::status::plumbing::index_as_worktree::traits::{FastEq, SubmoduleStatus};
use gix::status::plumbing::index_as_worktree_with_renames::{Entry as StatusEntry, VisitEntry};

use super::unknown;

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
pub(crate) fn staged_changes(
    repo: &gix::Repository,
    head_is_unborn: bool,
) -> Result<BTreeMap<String, ChangeKind>, DomainError> {
    let index = repo.index_or_empty().map_err(unknown)?;
    let head_entries = if head_is_unborn {
        BTreeMap::new()
    } else {
        let head_tree = repo.head_tree_id().map_err(unknown)?;
        let head_index = repo.index_from_tree(&head_tree).map_err(unknown)?;
        entries_by_path(&head_index)
    };
    let index_entries = entries_by_path(&index);

    Ok(index_entries
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
        .collect())
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
        Summary::Conflict => ChangeKind::Conflicted,
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
