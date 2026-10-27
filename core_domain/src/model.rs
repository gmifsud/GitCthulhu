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
    /// Keep the commit message, file marks, and selection.
    Full,
    /// Keep the commit message and file marks; clear the selection.
    #[default]
    Partial,
    /// Clear the commit message, file marks, and selection.
    PathOnly,
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
}

impl AppSettings {
    pub const DEFAULT_UI_FONT: &'static str = "Segoe UI";
    pub const DEFAULT_UI_SIZE: u8 = 14;
    pub const MIN_UI_SIZE: u8 = 12;
    pub const MAX_UI_SIZE: u8 = 18;
    pub const DEFAULT_AGENT_MODEL: &'static str = "kilo-auto/free";
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
            worktree_tab_retention: WorktreeTabRetention::default(),
        }
    }
}
