//! What the window is doing and has done, per repository: every push, pull, stash, checkout,
//! commit and terminal command, with its result. Finished entries are saved through the
//! `ActivityStore` port; running ones live only here.

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use core_domain::{repository_key, ActivityOutcome, ActivityRecord};

/// Entries kept in memory per repository, and loaded back at start.
pub(crate) const KEEP_PER_REPOSITORY: usize = 500;
/// How long a finished action stays in the status bar.
pub(crate) const RESULT_SHOWN_FOR: std::time::Duration = std::time::Duration::from_secs(6);

/// Where an action runs. A tab runs one git action at a time, and its terminal one command, so
/// a lane has at most one running entry.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum Lane {
    /// Git work in the worktree at this folder.
    Worktree(String),
    /// The terminal.
    Terminal,
    /// Adding or removing a worktree, one at a time for the whole window.
    Worktrees,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Entry {
    pub id: u64,
    /// [`repository_key`] of the repository it belongs to.
    pub repository: String,
    pub record: ActivityRecord,
    /// `None` once finished (and for entries loaded from disk).
    pub running: Option<(Lane, Instant)>,
    /// When it finished in this run, for the status bar's short-lived result.
    pub finished_at: Option<Instant>,
}

impl Entry {
    pub(crate) fn is_running(&self) -> bool {
        self.running.is_some()
    }
}

/// What to start recording: a title, what it acts on, and the git command it amounts to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Action {
    pub title: String,
    pub detail: String,
    pub command: String,
}

impl Action {
    pub(crate) fn new(title: impl Into<String>, detail: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            detail: detail.into(),
            command: command.into(),
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct ActivityLog {
    /// Oldest first, all repositories.
    entries: Vec<Entry>,
    next_id: u64,
    /// Repository keys whose saved log has been read in.
    loaded: Vec<String>,
}

impl ActivityLog {
    /// Start an entry. A lane that still has a running entry has it closed as stopped first,
    /// so a lost finish can never leave a spinner behind.
    pub(crate) fn begin(&mut self, repository: &str, worktree: &str, lane: Lane, action: Action) -> u64 {
        if self.entries.iter().any(|entry| entry.running.as_ref().is_some_and(|(l, _)| *l == lane)) {
            self.finish(&lane, ActivityOutcome::Stopped);
        }
        self.next_id += 1;
        let worktree = if repository_key(worktree) == repository_key(repository) {
            String::new()
        } else {
            worktree.to_string()
        };
        self.entries.push(Entry {
            id: self.next_id,
            repository: repository_key(repository),
            record: ActivityRecord {
                started: unix_now(),
                duration_ms: 0,
                title: action.title,
                detail: action.detail,
                command: action.command,
                worktree,
                outcome: ActivityOutcome::Succeeded,
            },
            running: Some((lane, Instant::now())),
            finished_at: None,
        });
        self.trim();
        self.next_id
    }

    /// Close the running entry in `lane`. Returns the repository and the record to save.
    pub(crate) fn finish(&mut self, lane: &Lane, outcome: ActivityOutcome) -> Option<(String, ActivityRecord)> {
        let entry = self
            .entries
            .iter_mut()
            .rev()
            .find(|entry| entry.running.as_ref().is_some_and(|(l, _)| l == lane))?;
        let (_, began) = entry.running.take()?;
        entry.record.duration_ms = u64::try_from(began.elapsed().as_millis()).unwrap_or(u64::MAX);
        entry.record.outcome = outcome;
        entry.finished_at = Some(Instant::now());
        Some((entry.repository.clone(), entry.record.clone()))
    }

    /// Put a repository's saved log in front of anything recorded for it this run.
    /// Reading the same repository twice changes nothing.
    pub(crate) fn restore(&mut self, repository: &str, records: Vec<ActivityRecord>) {
        let key = repository_key(repository);
        if self.loaded.contains(&key) {
            return;
        }
        self.loaded.push(key.clone());
        let saved: Vec<Entry> = records
            .into_iter()
            .map(|record| {
                self.next_id += 1;
                Entry {
                    id: self.next_id,
                    repository: key.clone(),
                    record,
                    running: None,
                    finished_at: None,
                }
            })
            .collect();
        let at = self.entries.iter().position(|entry| entry.repository == key).unwrap_or(self.entries.len());
        self.entries.splice(at..at, saved);
        self.trim();
    }

    #[cfg(test)]
    pub(crate) fn is_loaded(&self, repository: &str) -> bool {
        self.loaded.contains(&repository_key(repository))
    }

    /// One repository's entries, newest first.
    pub(crate) fn for_repository(&self, repository: &str) -> Vec<&Entry> {
        let key = repository_key(repository);
        self.entries.iter().rev().filter(|entry| entry.repository == key).collect()
    }

    /// The newest entry still running anywhere, for the status bar.
    pub(crate) fn running(&self) -> Option<&Entry> {
        self.entries.iter().rev().find(|entry| entry.is_running())
    }

    /// The newest entry that finished within [`RESULT_SHOWN_FOR`], for the status bar.
    pub(crate) fn recent_result(&self) -> Option<&Entry> {
        self.entries
            .iter()
            .rev()
            .find(|entry| entry.finished_at.is_some_and(|at| at.elapsed() < RESULT_SHOWN_FOR))
    }

    /// Failures in a repository since it was opened this run (saved ones are history).
    pub(crate) fn fresh_failures(&self, repository: &str) -> usize {
        let key = repository_key(repository);
        self.entries
            .iter()
            .filter(|entry| {
                entry.repository == key
                    && entry.finished_at.is_some()
                    && matches!(entry.record.outcome, ActivityOutcome::Failed(_))
            })
            .count()
    }

    /// Forget a repository's finished entries. Running ones stay.
    pub(crate) fn clear(&mut self, repository: &str) {
        let key = repository_key(repository);
        self.entries.retain(|entry| entry.repository != key || entry.is_running());
    }

    /// Keep each repository to its newest [`KEEP_PER_REPOSITORY`] entries.
    fn trim(&mut self) {
        let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        let mut keep = vec![true; self.entries.len()];
        for (index, entry) in self.entries.iter().enumerate().rev() {
            let count = seen.entry(entry.repository.clone()).or_default();
            *count += 1;
            if *count > KEEP_PER_REPOSITORY && !entry.is_running() {
                keep[index] = false;
            }
        }
        let mut flags = keep.into_iter();
        self.entries.retain(|_| flags.next().unwrap_or(true));
    }
}

/// "a.rs", or "3 files" past one.
fn files(paths: &[String]) -> String {
    match paths {
        [one] => one.clone(),
        many => format!("{} files", many.len()),
    }
}

fn short(id: &str) -> &str {
    id.get(..8).unwrap_or(id)
}

fn operation(operation: core_domain::RepoOperation) -> (&'static str, &'static str) {
    use core_domain::RepoOperation::*;
    match operation {
        Merge => ("merge", "merge"),
        Rebase => ("rebase", "rebase"),
        CherryPick => ("cherry-pick", "cherry-pick"),
        Revert => ("revert", "revert"),
    }
}

pub(crate) fn edit_action(edit: &crate::jobs::WorktreeEdit) -> Action {
    use crate::jobs::WorktreeEdit;
    match edit {
        WorktreeEdit::Stage(paths) => Action::new("Stage", files(paths), "git add"),
        WorktreeEdit::Unstage(paths) => Action::new("Unstage", files(paths), "git restore --staged"),
        WorktreeEdit::Untrack { paths, ignore } if ignore.is_empty() => {
            Action::new("Untrack", files(paths), "git rm --cached -r")
        }
        WorktreeEdit::Untrack { paths, .. } => {
            Action::new("Untrack and ignore", files(paths), "git rm --cached -r + .gitignore")
        }
        WorktreeEdit::Commit(message) => Action::new(
            "Commit",
            message.lines().next().unwrap_or_default().trim(),
            "git commit",
        ),
    }
}

pub(crate) fn stash_action(action: &crate::jobs::StashAction) -> Action {
    use crate::jobs::StashAction;
    match action {
        StashAction::Push { message, include_untracked } => Action::new(
            "Stash",
            if message.trim().is_empty() { "all changes" } else { message.trim() },
            if *include_untracked { "git stash push --include-untracked" } else { "git stash push" },
        ),
        StashAction::PushPaths { paths, .. } => Action::new("Stash", files(paths), "git stash push --"),
        StashAction::Apply(index) => Action::new("Stash apply", format!("stash@{{{index}}}"), "git stash apply"),
        StashAction::Pop(index) => Action::new("Stash pop", format!("stash@{{{index}}}"), "git stash pop"),
        StashAction::Drop(index) => Action::new("Stash drop", format!("stash@{{{index}}}"), "git stash drop"),
    }
}

pub(crate) fn branch_action(action: &crate::jobs::BranchAction) -> Action {
    use crate::jobs::BranchAction;
    match action {
        BranchAction::Create { name, start } => {
            Action::new("Create branch", format!("{name} from {start}"), format!("git branch {name} {start}"))
        }
        BranchAction::Rename { from, to } => {
            Action::new("Rename branch", format!("{from} → {to}"), format!("git branch -m {from} {to}"))
        }
        BranchAction::Delete { name } => Action::new("Delete branch", name.clone(), format!("git branch -d {name}")),
    }
}

pub(crate) fn history_action(action: &crate::jobs::HistoryAction) -> Action {
    use crate::jobs::HistoryAction;
    use core_domain::{ConflictSide, ResetMode};
    match action {
        HistoryAction::CherryPick { id } => {
            Action::new("Cherry-pick", short(id), format!("git cherry-pick {}", short(id)))
        }
        HistoryAction::Reset { id, mode } => {
            let flag = match mode {
                ResetMode::Soft => "--soft",
                ResetMode::Mixed => "--mixed",
                ResetMode::Hard => "--hard",
            };
            Action::new("Reset", format!("{} {}", flag.trim_start_matches('-'), short(id)), format!("git reset {flag} {}", short(id)))
        }
        HistoryAction::Merge { name, no_ff } => Action::new(
            "Merge",
            format!("{name} into current"),
            if *no_ff { format!("git merge --no-ff {name}") } else { format!("git merge {name}") },
        ),
        HistoryAction::Rebase { name } => Action::new("Rebase", format!("onto {name}"), format!("git rebase {name}")),
        HistoryAction::Abort(op) => {
            let (name, command) = operation(*op);
            Action::new("Abort", name, format!("git {command} --abort"))
        }
        HistoryAction::Continue(op) => {
            let (name, command) = operation(*op);
            Action::new("Continue", name, format!("git {command} --continue"))
        }
        HistoryAction::Resolve { path, side } => {
            let (label, flag) = match side {
                ConflictSide::Yours => ("yours", "--ours"),
                ConflictSide::Theirs => ("theirs", "--theirs"),
            };
            Action::new("Resolve conflict", format!("{path}: {label}"), format!("git checkout {flag} + git add"))
        }
    }
}

pub(crate) fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// "850 ms", "4.2 s", "2 min 5 s".
pub(crate) fn duration_label(ms: u64) -> String {
    match ms {
        0..=999 => format!("{ms} ms"),
        1_000..=59_999 => format!("{:.1} s", ms as f64 / 1_000.0),
        _ => format!("{} min {} s", ms / 60_000, (ms % 60_000) / 1_000),
    }
}

/// The status bar's present-tense line for a running action: "Push: development…".
pub(crate) fn running_label(record: &ActivityRecord) -> String {
    if record.detail.is_empty() {
        format!("{}…", record.title)
    } else {
        format!("{}: {}…", record.title, record.detail)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPO: &str = r"D:\Repos\ISL";

    fn push() -> Action {
        Action::new("Push", "development", "git push")
    }

    #[test]
    fn an_action_runs_then_finishes_with_its_outcome() {
        let mut log = ActivityLog::default();
        let lane = Lane::Worktree(REPO.into());
        log.begin(REPO, REPO, lane.clone(), push());
        assert_eq!(log.running().map(|entry| entry.record.title.as_str()), Some("Push"));
        let (repository, record) = log.finish(&lane, ActivityOutcome::Failed("rejected".into())).expect("closed");
        assert_eq!(repository, repository_key(REPO));
        assert_eq!(record.outcome, ActivityOutcome::Failed("rejected".into()));
        assert_eq!(record.worktree, "", "the repository's own folder is not repeated");
        assert!(log.running().is_none());
        assert!(log.recent_result().is_some());
        assert_eq!(log.fresh_failures(REPO), 1);
        assert!(log.finish(&lane, ActivityOutcome::Succeeded).is_none(), "nothing left to close");
    }

    #[test]
    fn a_new_action_in_a_busy_lane_stops_the_old_one() {
        let mut log = ActivityLog::default();
        let lane = Lane::Terminal;
        log.begin(REPO, REPO, lane.clone(), Action::new("Terminal", "", "git log"));
        log.begin(REPO, REPO, lane.clone(), Action::new("Terminal", "", "git status"));
        let entries = log.for_repository(REPO);
        assert_eq!(entries[1].record.outcome, ActivityOutcome::Stopped);
        assert!(entries[0].is_running());
    }

    #[test]
    fn repositories_keep_separate_logs_and_saved_entries_go_first() {
        let mut log = ActivityLog::default();
        log.begin(REPO, r"D:\Repos\ISL-wt", Lane::Worktree(r"D:\Repos\ISL-wt".into()), push());
        log.begin(r"D:\Repos\Other", r"D:\Repos\Other", Lane::Worktree(r"D:\Repos\Other".into()), push());
        let saved = ActivityRecord {
            started: 1,
            duration_ms: 5,
            title: "Fetch".into(),
            detail: String::new(),
            command: "git fetch".into(),
            worktree: String::new(),
            outcome: ActivityOutcome::Succeeded,
        };
        log.restore("d:/repos/isl/", vec![saved.clone()]);
        log.restore(REPO, vec![saved]);
        let titles: Vec<_> = log.for_repository(REPO).iter().map(|entry| entry.record.title.clone()).collect();
        assert_eq!(titles, vec!["Push", "Fetch"], "newest first, the saved one once");
        assert_eq!(log.for_repository(REPO)[0].record.worktree, r"D:\Repos\ISL-wt");
        assert!(log.is_loaded(REPO));
        log.clear(REPO);
        assert_eq!(log.for_repository(REPO).len(), 1, "the running push stays");
        assert_eq!(log.for_repository(r"D:\Repos\Other").len(), 1);
    }

    #[test]
    fn each_repository_keeps_its_newest_entries() {
        let mut log = ActivityLog::default();
        let lane = Lane::Worktree(REPO.into());
        for _ in 0..KEEP_PER_REPOSITORY + 5 {
            log.begin(REPO, REPO, lane.clone(), push());
            log.finish(&lane, ActivityOutcome::Succeeded);
        }
        assert_eq!(log.for_repository(REPO).len(), KEEP_PER_REPOSITORY);
    }

    #[test]
    fn actions_name_what_they_did_and_the_git_command() {
        use crate::jobs::{HistoryAction, StashAction, WorktreeEdit};
        let stage = edit_action(&WorktreeEdit::Stage(vec!["a.rs".into(), "b.rs".into()]));
        assert_eq!((stage.title.as_str(), stage.detail.as_str()), ("Stage", "2 files"));
        let commit = edit_action(&WorktreeEdit::Commit("feat: x\n\nbody".into()));
        assert_eq!(commit.detail, "feat: x");
        let pop = stash_action(&StashAction::Pop(2));
        assert_eq!((pop.detail.as_str(), pop.command.as_str()), ("stash@{2}", "git stash pop"));
        let reset = history_action(&HistoryAction::Reset { id: "abcdef0123".into(), mode: core_domain::ResetMode::Hard });
        assert_eq!(reset.command, "git reset --hard abcdef01");
        let abort = history_action(&HistoryAction::Abort(core_domain::RepoOperation::CherryPick));
        assert_eq!(abort.command, "git cherry-pick --abort");
    }

    #[test]
    fn durations_read_naturally() {
        assert_eq!(duration_label(850), "850 ms");
        assert_eq!(duration_label(4_200), "4.2 s");
        assert_eq!(duration_label(125_000), "2 min 5 s");
        assert_eq!(running_label(&ActivityRecord {
            started: 0,
            duration_ms: 0,
            title: "Push".into(),
            detail: "development".into(),
            command: String::new(),
            worktree: String::new(),
            outcome: ActivityOutcome::Succeeded,
        }), "Push: development…");
    }
}
