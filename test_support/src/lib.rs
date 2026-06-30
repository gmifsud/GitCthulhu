//! test_support
//! Shared test fakes for all adapter crates.
//! Add as `[dev-dependencies]` only — never in `[dependencies]`.

use core_domain::{
    Branch, Commit, CredentialStore, DomainError, GitRepository, RepoStatus, SshManager,
};

/// Pre-configured fake that implements [`GitRepository`].
///
/// All methods return clones of the stored `Result` fields, so each call
/// is independent and deterministic.  Construct with [`FakeGitRepository::ok`]
/// for an all-success baseline and override individual fields for error paths.
pub struct FakeGitRepository {
    pub status_result: Result<RepoStatus, DomainError>,
    pub log_result: Result<Vec<Commit>, DomainError>,
    pub branches_result: Result<Vec<Branch>, DomainError>,
    pub checkout_result: Result<(), DomainError>,
    pub fetch_result: Result<(), DomainError>,
    pub fetch_ssh_result: Result<(), DomainError>,
}

impl FakeGitRepository {
    /// All methods return `Ok` with empty/no-op payloads.
    pub fn ok() -> Self {
        Self {
            status_result: Ok(RepoStatus {
                staged_files: vec![],
                unstaged_files: vec![],
                untracked_files: vec![],
            }),
            log_result: Ok(vec![]),
            branches_result: Ok(vec![]),
            checkout_result: Ok(()),
            fetch_result: Ok(()),
            fetch_ssh_result: Ok(()),
        }
    }
}

impl GitRepository for FakeGitRepository {
    fn status(&self) -> Result<RepoStatus, DomainError> {
        self.status_result.clone()
    }

    fn log(&self, _max_count: Option<usize>) -> Result<Vec<Commit>, DomainError> {
        self.log_result.clone()
    }

    fn branches(&self) -> Result<Vec<Branch>, DomainError> {
        self.branches_result.clone()
    }

    fn checkout_branch(&self, _branch_name: &str) -> Result<(), DomainError> {
        self.checkout_result.clone()
    }

    fn fetch(&self, _store: &dyn CredentialStore) -> Result<(), DomainError> {
        self.fetch_result.clone()
    }

    fn fetch_ssh(&self, _ssh_manager: &dyn SshManager) -> Result<(), DomainError> {
        self.fetch_ssh_result.clone()
    }
}
