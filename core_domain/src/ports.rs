use super::{AppSettings, Branch, Commit, DagNode, DomainError, RepoStatus};

/// The GitRepository port.
/// Adapters such as `git_adapter` implement this trait.
pub trait GitRepository {
    fn status(&self) -> Result<RepoStatus, DomainError>;
    fn log(&self, max_count: Option<usize>) -> Result<Vec<Commit>, DomainError>;
    fn branches(&self) -> Result<Vec<Branch>, DomainError>;
    fn checkout_branch(&self, branch_name: &str) -> Result<(), DomainError>;
    fn stage_paths(&self, paths: &[String]) -> Result<(), DomainError>;
    fn unstage_paths(&self, paths: &[String]) -> Result<(), DomainError>;
    fn commit(&self, message: &str) -> Result<(), DomainError>;
    /// `git fetch` of the default remote, using the repository's own credential helper and SSH config.
    fn fetch(&self) -> Result<(), DomainError>;
    /// `git push` of the current branch to its upstream, with no refspec.
    fn push(&self) -> Result<(), DomainError>;
    /// `git pull --no-rebase --ff-only`. A diverged branch fails and does not start a merge or rebase.
    fn pull(&self) -> Result<(), DomainError>;
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

/// Port for OAuth2 integration.
pub trait OAuthServer {
    fn start_loopback(&self, port: u16) -> Result<(), DomainError>;
    fn stop_loopback(&self) -> Result<(), DomainError>;
    fn get_callback_token(&self) -> Result<Option<String>, DomainError>;
}

/// Port for configuration persistence.
pub trait ConfigStore {
    fn load_settings(&self) -> Result<AppSettings, DomainError>;
    fn save_settings(&self, settings: &AppSettings) -> Result<(), DomainError>;
    fn watch_settings<F>(&self, callback: F) -> Result<(), DomainError>
    where
        F: Fn(AppSettings) + Send + 'static;
}

/// Port for DAG caching.
pub trait GraphCache {
    fn get_dag(&self, cache_key: &str) -> Result<Option<Vec<DagNode>>, DomainError>;
    fn store_dag(&self, cache_key: &str, dag: &[DagNode]) -> Result<(), DomainError>;
}
