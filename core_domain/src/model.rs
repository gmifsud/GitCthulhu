use std::collections::BTreeMap;
use serde::{Deserialize, Serialize};

/// Represents a Git commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Commit {
    pub id: String,
    pub parent_ids: Vec<String>,
    pub author: String,
    pub message: String,
    pub timestamp: i64,
}

/// Represents a node in the visual commit DAG.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DagNode {
    pub commit_id: String,
    pub lane: usize,
    /// One `(parent_id, lane the edge runs down)` per parent.
    pub connections: Vec<(String, usize)>,
}

/// A tag that peels to a commit. Tags that point at a tree or a blob are left out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub name: String,
    pub commit_id: String,
}

/// One entry from `git stash list`, addressed as `stash@{index}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stash {
    pub index: usize,
    pub message: String,
    /// The stash commit. Its first parent is the HEAD it was taken on, so its first-parent
    /// diff is what `git stash show -p` prints for tracked files.
    pub commit_id: String,
}

/// Represents a Git branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Branch {
    pub name: String,
    pub is_remote: bool,
    pub is_head: bool,
    /// Peeled commit id this branch currently points at (full hex).
    pub tip: String,
    /// Commits on this branch not on its upstream. `0` when unknown or no upstream.
    pub ahead: usize,
    /// Commits on the upstream not on this branch. `0` when unknown or no upstream.
    pub behind: usize,
}

/// What merging a branch or commit into the current branch would do, worked out without
/// touching the index, the working tree or any ref.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MergePreview {
    /// Commits the merge would bring in. Zero: already up to date.
    pub commits: usize,
    /// The current branch has nothing of its own: it would simply move forward.
    pub fast_forward: bool,
    /// Files the merge would change.
    pub files: Vec<String>,
    /// Files that would conflict.
    pub conflicts: Vec<String>,
}

/// One entry from `git worktree list`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worktree {
    pub path: String,
    pub branch: String,
    pub is_main: bool,
    pub locked: bool,
}

/// How the current branch compares with its upstream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackingStatus {
    /// Short upstream name, such as `origin/main`.
    pub upstream: String,
    pub ahead: usize,
    pub behind: usize,
}

/// Where a changed path sits, which decides the diff and the discard command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileDisposition {
    Staged,
    Unstaged,
    Untracked,
}

/// One line of a unified diff, without the leading `+`, `-`, or space.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLineKind {
    Context,
    Added,
    Removed,
}

/// One `@@` block of a unified diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    pub header: String,
    pub lines: Vec<DiffLine>,
}

/// The text diff of one path. `binary` is set when git reports a binary file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Patch {
    pub path: String,
    pub hunks: Vec<Hunk>,
    pub binary: bool,
}

/// How far `git reset` moves the current branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetMode {
    /// Move `HEAD` and leave the index and the worktree.
    Soft,
    /// Move `HEAD`, clear the index, and leave the worktree.
    Mixed,
    /// Move `HEAD` and replace the index and the worktree.
    Hard,
}

/// How a changed path differs, as `git status` reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed,
    Conflicted,
    Untracked,
}

/// The kind of each staged and unstaged path in a [`RepoStatus`].
///
/// A staged or unstaged path with no entry reads as [`ChangeKind::Modified`], so an adapter
/// that does not classify changes still reports something true. Untracked paths need no entry.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChangeKinds {
    pub staged: BTreeMap<String, ChangeKind>,
    pub unstaged: BTreeMap<String, ChangeKind>,
}

impl ChangeKinds {
    pub fn of(&self, place: FileDisposition, path: &str) -> ChangeKind {
        let kinds = match place {
            FileDisposition::Staged => &self.staged,
            FileDisposition::Unstaged => &self.unstaged,
            FileDisposition::Untracked => return ChangeKind::Untracked,
        };
        kinds.get(path).copied().unwrap_or(ChangeKind::Modified)
    }
}

/// A git operation stopped half-way, waiting for the user (usually on conflicts).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepoOperation {
    Merge,
    Rebase,
    CherryPick,
    Revert,
}

impl RepoOperation {
    /// "merge", "rebase"… for messages.
    pub fn noun(self) -> &'static str {
        match self {
            RepoOperation::Merge => "merge",
            RepoOperation::Rebase => "rebase",
            RepoOperation::CherryPick => "cherry-pick",
            RepoOperation::Revert => "revert",
        }
    }
}

/// Which side of a conflict to keep for a whole file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictSide {
    /// The branch you are on (`--ours`).
    Yours,
    /// The branch being merged in, or the commit being replayed (`--theirs`).
    Theirs,
}

/// Represents the status of the working directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoStatus {
    pub staged_files: Vec<String>,
    pub unstaged_files: Vec<String>,
    pub untracked_files: Vec<String>,
    /// `None` when the current branch has no upstream, or `HEAD` is detached or unborn.
    pub tracking: Option<TrackingStatus>,
    /// Peeled `HEAD` commit id when born; `None` when unborn. Used for focus-current when detached.
    pub head_commit_id: Option<String>,
    /// Added, modified, deleted… for each staged and unstaged path.
    pub kinds: ChangeKinds,
    /// The merge, rebase, cherry-pick or revert in progress, if any.
    pub operation: Option<RepoOperation>,
    /// Paths with unresolved conflicts. They are in no other list.
    pub conflicted_files: Vec<String>,
}

/// Domain errors that can occur during Git operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainError {
    RepositoryNotFound,
    AccessDenied,
    AuthenticationFailed,
    Unknown(String),
}

/// What a worktree tab keeps when the user switches away from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorktreeTabRetention {
    /// Keep the commit message, file marks, and selection. The default: returning to a tab
    /// finds it as it was left.
    #[default]
    Full,
    /// Keep the commit message and file marks; clear the selection.
    Partial,
    /// Clear the commit message, file marks, and selection.
    PathOnly,
}

/// How the app name is drawn at the left of the header. Chosen under Appearance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeaderMark {
    /// "Git" muted, "Cthulhu" in the accent colour.
    #[default]
    TwoTone,
    /// An accent badge with a tentacle-ish "G", then the name.
    Badge,
    /// Spaced small capitals in a muted colour.
    SmallCaps,
    /// A shell prompt: `❯ gitcthulhu`.
    Prompt,
    /// The badge alone, no name.
    BadgeOnly,
}

impl HeaderMark {
    pub const ALL: [HeaderMark; 5] = [
        HeaderMark::TwoTone,
        HeaderMark::Badge,
        HeaderMark::SmallCaps,
        HeaderMark::Prompt,
        HeaderMark::BadgeOnly,
    ];
}

/// The worktree tabs a repository had open, so the next launch can put them back.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct WorkspaceTabs {
    /// The folder the repository was opened from.
    pub repository: String,
    /// Every tab's folder, in tab order.
    pub tabs: Vec<String>,
    /// The folder of the tab that was in front. Empty when unknown.
    #[serde(default)]
    pub active: String,
}

/// User-defined settings for the application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppSettings {
    pub theme: String,
    pub repository_shortcuts: Vec<String>,
    /// Last folder opened, when one has been remembered.
    pub last_repo_path: Option<String>,
    /// Face chosen under Appearance. Unknown names fall back to [`Self::DEFAULT_UI_FONT`].
    pub ui_font: String,
    /// Point size chosen under Appearance, clamped to [`Self::MIN_UI_SIZE`]..=[`Self::MAX_UI_SIZE`].
    pub ui_size: u8,
    /// Kilo gateway token. Empty until the user saves one under Agents and Models.
    pub agent_token: String,
    /// Model id sent to the Kilo gateway, such as `kilo-auto/free`.
    pub agent_model: String,
    /// What a worktree tab keeps when the user switches to another tab.
    pub worktree_tab_retention: WorktreeTabRetention,
    /// Repositories opened with Open… or from the command line, in the order the user keeps
    /// them: a new one joins at the end, opening one never moves it, and only a drag in the
    /// Repositories column reorders them. Linked worktree tabs are never added.
    pub recent_repositories: Vec<String>,
    /// How the header draws the app name.
    pub header_mark: HeaderMark,
    /// The Repositories column stays open beside the Explorer (true) or folds to a rail.
    pub repositories_pinned: bool,
    /// The tabs each repository had open when it was last on screen, one entry per repository.
    pub workspace_tabs: Vec<WorkspaceTabs>,
}

impl AppSettings {
    pub const DEFAULT_UI_FONT: &'static str = "Segoe UI";
    pub const DEFAULT_UI_SIZE: u8 = 14;
    pub const MIN_UI_SIZE: u8 = 12;
    pub const MAX_UI_SIZE: u8 = 18;
    pub const DEFAULT_AGENT_MODEL: &'static str = "kilo-auto/free";

    /// Add `path` to the end of [`Self::recent_repositories`] unless the folder is already
    /// there. A folder already listed keeps its place; only its spelling is refreshed.
    /// Blank paths are ignored. Returns true when the list changed.
    ///
    /// There is no cap and nothing is dropped: the list is the user's, kept in their order,
    /// and shortened only by Forget.
    pub fn remember_recent(&mut self, path: &str) -> bool {
        let path = path.trim();
        if path.is_empty() {
            return false;
        }
        match self
            .recent_repositories
            .iter_mut()
            .find(|known| same_repository_path(known, path))
        {
            Some(known) if known == path => false,
            Some(known) => {
                *known = path.to_string();
                true
            }
            None => {
                self.recent_repositories.push(path.to_string());
                true
            }
        }
    }

    /// Put the repository at `from` where `onto` is now, shifting the rest: dropped on a row
    /// above, it lands before that row; on a row below, after it. Returns true when the order
    /// changed. Unknown or equal folders change nothing.
    pub fn move_repository(&mut self, from: &str, onto: &str) -> bool {
        let position = |path: &str| {
            self.recent_repositories
                .iter()
                .position(|known| same_repository_path(known, path))
        };
        let (Some(source), Some(target)) = (position(from), position(onto)) else {
            return false;
        };
        if source == target {
            return false;
        }
        let moving = self.recent_repositories.remove(source);
        self.recent_repositories.insert(target, moving);
        true
    }

    /// Drop `path` from [`Self::recent_repositories`], and the tabs remembered for it.
    /// Returns true when it was listed.
    pub fn forget_recent(&mut self, path: &str) -> bool {
        let before = self.recent_repositories.len();
        self.recent_repositories
            .retain(|known| !same_repository_path(known, path));
        self.workspace_tabs
            .retain(|known| !same_repository_path(&known.repository, path));
        before != self.recent_repositories.len()
    }

    /// The tabs remembered for the repository opened from `repository`, if any.
    pub fn tabs_of(&self, repository: &str) -> Option<&WorkspaceTabs> {
        self.workspace_tabs
            .iter()
            .find(|known| same_repository_path(&known.repository, repository))
    }

    /// Remember which tabs the repository opened from `repository` has open, and which is in
    /// front. Its entry keeps its place; a new repository joins at the end. Returns true when
    /// anything changed, so an unchanged window never writes the settings file.
    pub fn remember_tabs(&mut self, repository: &str, tabs: &[String], active: &str) -> bool {
        let repository = repository.trim();
        if repository.is_empty() {
            return false;
        }
        match self
            .workspace_tabs
            .iter_mut()
            .find(|known| same_repository_path(&known.repository, repository))
        {
            // The same folder spelt another way is the same repository: nothing to write.
            Some(known) if known.tabs == tabs && known.active == active => false,
            Some(known) => {
                known.tabs = tabs.to_vec();
                known.active = active.to_string();
                true
            }
            None => {
                self.workspace_tabs.push(WorkspaceTabs {
                    repository: repository.to_string(),
                    tabs: tabs.to_vec(),
                    active: active.to_string(),
                });
                true
            }
        }
    }
}

/// True when two remembered paths name the same folder. Separators and a trailing
/// separator are ignored; case is ignored on Windows only, matching its filesystem.
pub fn same_repository_path(left: &str, right: &str) -> bool {
    repository_key(left) == repository_key(right)
}

/// One spelling per folder: forward slashes, no trailing slash, lower case on Windows.
/// Two paths with the same key are the same repository.
pub fn repository_key(path: &str) -> String {
    let unified = path.trim().replace('\\', "/");
    let trimmed = unified.trim_end_matches('/');
    let kept = if trimmed.is_empty() { unified.as_str() } else { trimmed };
    if cfg!(windows) {
        kept.to_lowercase()
    } else {
        kept.to_string()
    }
}

/// How a recorded action ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "state", content = "message")]
pub enum ActivityOutcome {
    Succeeded,
    /// Git's (or the shell's) own message.
    Failed(String),
    /// The user stopped it.
    Stopped,
}

/// One finished action in a repository's activity log: a push, a stash, a terminal command…
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivityRecord {
    /// When it started, in Unix seconds (UTC).
    pub started: i64,
    pub duration_ms: u64,
    /// What happened, in a word or two: "Push", "Stash pop".
    pub title: String,
    /// What it acted on: a branch, a file count, a stash message. May be empty.
    #[serde(default)]
    pub detail: String,
    /// The git command (or terminal line) it amounts to. May be empty.
    #[serde(default)]
    pub command: String,
    /// The worktree folder it ran in. Empty when it is the repository's own folder.
    #[serde(default)]
    pub worktree: String,
    pub outcome: ActivityOutcome,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: "DeepFocus".to_string(),
            repository_shortcuts: vec![],
            last_repo_path: None,
            ui_font: Self::DEFAULT_UI_FONT.to_string(),
            ui_size: Self::DEFAULT_UI_SIZE,
            agent_token: String::new(),
            agent_model: Self::DEFAULT_AGENT_MODEL.to_string(),
            recent_repositories: Vec::new(),
            worktree_tab_retention: WorktreeTabRetention::default(),
            header_mark: HeaderMark::default(),
            repositories_pinned: true,
            workspace_tabs: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remember_recent_adds_new_folders_at_the_end_and_never_moves_a_known_one() {
        let mut settings = AppSettings::default();
        assert!(settings.remember_recent("/r/a"));
        assert!(settings.remember_recent("/r/b"));
        assert!(settings.remember_recent("/r/c"));
        // Opening a listed folder again leaves the order alone.
        assert!(!settings.remember_recent("/r/b"));
        assert_eq!(settings.recent_repositories, vec!["/r/a", "/r/b", "/r/c"]);
        // A new spelling of a listed folder is kept, in the same place.
        assert!(settings.remember_recent("/r/b/"));
        assert_eq!(settings.recent_repositories, vec!["/r/a", "/r/b/", "/r/c"]);
    }

    #[test]
    fn remember_recent_ignores_blank_paths() {
        let mut settings = AppSettings::default();
        assert!(!settings.remember_recent("  "));
        assert!(settings.recent_repositories.is_empty());
    }

    #[test]
    fn remember_recent_drops_nothing_however_long_the_list() {
        let mut settings = AppSettings::default();
        for index in 0..40 {
            settings.remember_recent(&format!("/r/{index}"));
        }
        assert_eq!(settings.recent_repositories.len(), 40);
        assert_eq!(settings.recent_repositories[0], "/r/0");
    }

    #[test]
    fn move_repository_takes_the_place_of_the_row_it_is_dropped_on() {
        let mut settings = AppSettings::default();
        for path in ["/r/a", "/r/b", "/r/c", "/r/d"] {
            settings.remember_recent(path);
        }
        // Down: lands after the row dropped on.
        assert!(settings.move_repository("/r/a", "/r/c"));
        assert_eq!(settings.recent_repositories, vec!["/r/b", "/r/c", "/r/a", "/r/d"]);
        // Up: lands before it. Spelling does not matter.
        assert!(settings.move_repository("/r/d/", "/r/b"));
        assert_eq!(settings.recent_repositories, vec!["/r/d", "/r/b", "/r/c", "/r/a"]);
        // Onto itself, or an unknown folder: nothing changes.
        assert!(!settings.move_repository("/r/b", "/r/b"));
        assert!(!settings.move_repository("/r/b", "/r/zzz"));
        assert_eq!(settings.recent_repositories, vec!["/r/d", "/r/b", "/r/c", "/r/a"]);
    }

    #[test]
    fn forget_recent_drops_the_folder_whatever_its_spelling() {
        let mut settings = AppSettings::default();
        settings.remember_recent("/r/a");
        settings.remember_recent("/r/b");
        assert!(settings.forget_recent("/r/a/"));
        assert_eq!(settings.recent_repositories, vec!["/r/b"]);
        assert!(!settings.forget_recent("/r/a"));
    }

    #[test]
    fn separators_do_not_make_a_different_folder() {
        assert!(same_repository_path(r"D:\Repos\ISL", "D:/Repos/ISL/"));
        assert!(!same_repository_path("/r/a", "/r/ab"));
    }

    #[test]
    fn a_repository_remembers_its_tabs_and_keeps_its_place() {
        let mut settings = AppSettings::default();
        let tabs = vec!["/r/a".to_string(), "/r/wt/x".to_string()];
        assert!(settings.remember_tabs("/r/a", &tabs, "/r/wt/x"));
        assert!(settings.remember_tabs("/r/b", &["/r/b".to_string()], "/r/b"));
        assert!(!settings.remember_tabs("/r/a/", &tabs, "/r/wt/x"), "nothing changed: no write");

        assert!(settings.remember_tabs("/r/a", &tabs[..1], "/r/a"));
        assert_eq!(settings.workspace_tabs.len(), 2);
        assert_eq!(settings.workspace_tabs[0].tabs, vec!["/r/a"]);
        assert_eq!(settings.tabs_of("/r/a/").map(|known| known.active.as_str()), Some("/r/a"));
        assert!(settings.tabs_of("/r/elsewhere").is_none());
    }

    #[test]
    fn forgetting_a_repository_forgets_its_tabs() {
        let mut settings = AppSettings::default();
        settings.remember_recent("/r/a");
        settings.remember_tabs("/r/a", &["/r/a".to_string(), "/r/wt/x".to_string()], "/r/a");
        assert!(settings.forget_recent("/r/a"));
        assert!(settings.tabs_of("/r/a").is_none());
    }
}
