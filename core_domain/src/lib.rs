//! core_domain
//! This crate contains the pure business logic and ports (interfaces).
//! It has zero dependencies on any UI framework (iced) or Git implementation (gitoxide).

use serde::{Serialize, Deserialize};

/// Represents a Git commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Commit {
    pub id: String,
    pub parent_ids: Vec<String>,
    pub author: String,
    pub message: String,
    pub timestamp: i64,
}

/// Represents a node in the visual commit DAG
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DagNode {
    pub commit_id: String,
    pub lane: usize,
    pub connections: Vec<(String, usize)>, // connected to (commit_id, lane)
}

/// Helper function to build a DAG from a list of commits in parallel
pub fn build_dag(commits: &[Commit]) -> Vec<DagNode> {
    use rayon::prelude::*;
    
    // In a real implementation, this would perform a topological sort
    // and assign lanes using graph traversal. For large repositories,
    // rayon is used to parallelize the lane assignment and geometry calculations.
    
    // Simulating a parallel map over commits for geometric lane assignment
    commits
        .par_iter()
        .enumerate()
        .map(|(index, commit)| {
            DagNode {
                commit_id: commit.id.clone(),
                lane: index % 3, // simplified lane assignment
                connections: vec![],
            }
        })
        .collect()
}

/// Represents a Git branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Branch {
    pub name: String,
    pub is_remote: bool,
    pub is_head: bool,
}

/// Represents the status of the working directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoStatus {
    pub staged_files: Vec<String>,
    pub unstaged_files: Vec<String>,
    pub untracked_files: Vec<String>,
}

/// Domain errors that can occur during Git operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainError {
    RepositoryNotFound,
    AccessDenied,
    AuthenticationFailed,
    Unknown(String),
}

/// The GitRepository Port.
/// Adapters (like `git_adapter`) will implement this trait to provide actual Git functionality.
pub trait GitRepository {
    fn status(&self) -> Result<RepoStatus, DomainError>;
    fn log(&self, max_count: Option<usize>) -> Result<Vec<Commit>, DomainError>;
    fn branches(&self) -> Result<Vec<Branch>, DomainError>;
    fn checkout_branch(&self, branch_name: &str) -> Result<(), DomainError>;
    fn fetch(&self, store: &dyn CredentialStore) -> Result<(), DomainError>;
    fn fetch_ssh(&self, ssh_manager: &dyn SshManager) -> Result<(), DomainError>;
}

pub trait CredentialStore {
    fn save_token(&self, user: &str, token: &str) -> Result<(), DomainError>;
    fn get_token(&self, user: &str) -> Result<String, DomainError>;
    fn delete_token(&self, user: &str) -> Result<(), DomainError>;
}

pub trait SshManager {
    fn get_default_key_path(&self) -> Result<String, DomainError>;
    fn resolve_subsystem_path(&self, path: &str) -> String;
    fn prepare_ssh_env(&self) -> Result<(String, String), DomainError>;
    fn preflight_check(&self) -> Result<(), DomainError>;
}

/// State machine for Provider Authentication workflows
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthState {
    Idle,
    AwaitingLoopback,
    TokenReceived(String), // The token or authorization code
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportState {
    Healthy,
    Degraded(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppPhase {
    PreFlightValidation,
    Ready,
}

/// Port for OAuth2 integration
pub trait OAuthServer {
    fn start_loopback(&self, port: u16) -> Result<(), DomainError>;
    fn stop_loopback(&self) -> Result<(), DomainError>;
    fn get_callback_token(&self) -> Result<Option<String>, DomainError>;
}

/// User-defined settings for the application
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppSettings {
    pub theme: String,
    pub repository_shortcuts: Vec<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: "DeepFocus".to_string(),
            repository_shortcuts: vec![],
        }
    }
}

/// Port for configuration persistence
pub trait ConfigStore {
    fn load_settings(&self) -> Result<AppSettings, DomainError>;
    fn save_settings(&self, settings: &AppSettings) -> Result<(), DomainError>;
    fn watch_settings<F>(&self, callback: F) -> Result<(), DomainError> 
    where
        F: Fn(AppSettings) + Send + 'static;
}

/// Port for High-Performance DAG caching
pub trait GraphCache {
    fn get_dag(&self, cache_key: &str) -> Result<Option<Vec<DagNode>>, DomainError>;
    fn store_dag(&self, cache_key: &str, dag: &[DagNode]) -> Result<(), DomainError>;
}

/// Commands triggered by the user via the UI.
#[derive(Debug, Clone)]
pub enum UserCommand {
    RefreshRepository,
    Checkout(String),
}

/// Events emitted by the core domain to update the UI (View-Model).
#[derive(Debug, Clone)]
pub enum ViewEvent {
    PreFlightCompleted,
    PreFlightFailed(String),
    StatusUpdated(RepoStatus),
    LogUpdated(Vec<Commit>),
    BranchesUpdated(Vec<Branch>),
    AuthenticationRequired,
    SettingsUpdated(AppSettings),
    ErrorOccurred(DomainError),
}

/// Deterministic State Machine for the application.
pub struct AppState {
    pub phase: AppPhase,
    pub transport_state: TransportState,
    pub current_status: Option<RepoStatus>,
    pub commits: Vec<Commit>,
    pub dag_nodes: Vec<DagNode>,
    pub branches: Vec<Branch>,
    pub loading: bool,
    pub auth_state: AuthState,
    pub settings: AppSettings,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            phase: AppPhase::PreFlightValidation,
            transport_state: TransportState::Healthy,
            current_status: None,
            commits: vec![],
            dag_nodes: vec![],
            branches: vec![],
            loading: false,
            auth_state: AuthState::Idle,
            settings: AppSettings::default(),
        }
    }

    /// Process an event and transition state.
    pub fn apply_event(&mut self, event: ViewEvent) {
        match event {
            ViewEvent::PreFlightCompleted => {
                self.phase = AppPhase::Ready;
                self.transport_state = TransportState::Healthy;
            }
            ViewEvent::PreFlightFailed(reason) => {
                self.transport_state = TransportState::Degraded(reason);
            }
            ViewEvent::StatusUpdated(status) => {
                self.current_status = Some(status);
                self.loading = false;
            }
            ViewEvent::LogUpdated(commits) => {
                self.dag_nodes = build_dag(&commits);
                self.commits = commits;
                self.loading = false;
            }
            ViewEvent::BranchesUpdated(branches) => {
                self.branches = branches;
                self.loading = false;
            }
            ViewEvent::AuthenticationRequired => {
                self.auth_state = AuthState::AwaitingLoopback;
                self.loading = false;
            }
            ViewEvent::SettingsUpdated(settings) => {
                self.settings = settings;
            }
            ViewEvent::ErrorOccurred(_) => {
                self.loading = false;
            }
        }
    }

    /// Complete authentication and save to keyring securely mapped to remote_domain
    pub fn handle_token_received(&mut self, token: &str, store: &dyn CredentialStore, remote_domain: &str) -> Result<(), DomainError> {
        self.auth_state = AuthState::TokenReceived(token.to_string());
        store.save_token(remote_domain, token)?;
        Ok(())
    }
}
