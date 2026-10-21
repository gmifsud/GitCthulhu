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

/// Represents a Git branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Branch {
    pub name: String,
    pub is_remote: bool,
    pub is_head: bool,
}

/// How the current branch compares with its upstream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackingStatus {
    /// Short upstream name, such as `origin/main`.
    pub upstream: String,
    pub ahead: usize,
    pub behind: usize,
}

/// Represents the status of the working directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoStatus {
    pub staged_files: Vec<String>,
    pub unstaged_files: Vec<String>,
    pub untracked_files: Vec<String>,
    /// `None` when the current branch has no upstream, or `HEAD` is detached or unborn.
    pub tracking: Option<TrackingStatus>,
}

/// Domain errors that can occur during Git operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainError {
    RepositoryNotFound,
    AccessDenied,
    AuthenticationFailed,
    Unknown(String),
}

/// User-defined settings for the application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppSettings {
    pub theme: String,
    pub repository_shortcuts: Vec<String>,
    /// Last folder opened, when one has been remembered.
    pub last_repo_path: Option<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: "DeepFocus".to_string(),
            repository_shortcuts: vec![],
            last_repo_path: None,
        }
    }
}
