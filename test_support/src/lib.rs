//! test_support
//! Shared test fakes for all adapter crates.
//! Add as `[dev-dependencies]` only — never in `[dependencies]`.

use core_domain::{
    Branch, Commit, DomainError, FileDisposition, GitRepository, Hunk, Patch, RepoStatus, Stash,
    Tag, Worktree,
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
    pub stashes_result: Result<Vec<Stash>, DomainError>,
    pub tags_result: Result<Vec<Tag>, DomainError>,
    pub worktrees_result: Result<Vec<Worktree>, DomainError>,
    pub add_worktree_result: Result<(), DomainError>,
    pub remove_worktree_result: Result<(), DomainError>,
    pub checkout_result: Result<(), DomainError>,
    pub create_branch_result: Result<(), DomainError>,
    pub rename_branch_result: Result<(), DomainError>,
    pub delete_branch_result: Result<(), DomainError>,
    pub cherry_pick_result: Result<(), DomainError>,
    pub reset_result: Result<(), DomainError>,
    pub merge_result: Result<(), DomainError>,
    pub abort_result: Result<(), DomainError>,
    pub continue_result: Result<(), DomainError>,
    pub resolve_result: Result<(), DomainError>,
    pub rebase_result: Result<(), DomainError>,
    pub stage_result: Result<(), DomainError>,
    pub unstage_result: Result<(), DomainError>,
    pub untrack_result: Result<(), DomainError>,
    pub commit_result: Result<(), DomainError>,
    pub fetch_result: Result<(), DomainError>,
    pub push_result: Result<(), DomainError>,
    pub pull_result: Result<(), DomainError>,
    pub commit_patch_result: Result<Vec<Patch>, DomainError>,
    pub file_patch_result: Result<Patch, DomainError>,
    pub compare_result: Result<Patch, DomainError>,
    pub discard_result: Result<(), DomainError>,
    pub stage_hunk_result: Result<(), DomainError>,
    pub discard_hunk_result: Result<(), DomainError>,
    pub stash_push_result: Result<(), DomainError>,
    pub stash_apply_result: Result<(), DomainError>,
    pub stash_pop_result: Result<(), DomainError>,
    pub stash_drop_result: Result<(), DomainError>,
}

impl FakeGitRepository {
    /// All methods return `Ok` with empty/no-op payloads.
    pub fn ok() -> Self {
        Self {
            status_result: Ok(RepoStatus {
                staged_files: vec![],
                unstaged_files: vec![],
                untracked_files: vec![],
                tracking: None,
                head_commit_id: None,
                kinds: Default::default(),
                operation: None,
                conflicted_files: vec![],
            }),
            log_result: Ok(vec![]),
            branches_result: Ok(vec![]),
            stashes_result: Ok(vec![]),
            tags_result: Ok(vec![]),
            worktrees_result: Ok(vec![]),
            add_worktree_result: Ok(()),
            remove_worktree_result: Ok(()),
            checkout_result: Ok(()),
            create_branch_result: Ok(()),
            rename_branch_result: Ok(()),
            delete_branch_result: Ok(()),
            cherry_pick_result: Ok(()),
            reset_result: Ok(()),
            merge_result: Ok(()),
            abort_result: Ok(()),
            continue_result: Ok(()),
            resolve_result: Ok(()),
            rebase_result: Ok(()),
            stage_result: Ok(()),
            unstage_result: Ok(()),
            untrack_result: Ok(()),
            commit_result: Ok(()),
            fetch_result: Ok(()),
            push_result: Ok(()),
            pull_result: Ok(()),
            commit_patch_result: Ok(vec![]),
            file_patch_result: Ok(Patch {
                path: String::new(),
                hunks: vec![],
                binary: false,
            }),
            compare_result: Ok(Patch {
                path: String::new(),
                hunks: vec![],
                binary: false,
            }),
            discard_result: Ok(()),
            stage_hunk_result: Ok(()),
            discard_hunk_result: Ok(()),
            stash_push_result: Ok(()),
            stash_apply_result: Ok(()),
            stash_pop_result: Ok(()),
            stash_drop_result: Ok(()),
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

    fn stashes(&self) -> Result<Vec<Stash>, DomainError> {
        self.stashes_result.clone()
    }

    fn tags(&self) -> Result<Vec<Tag>, DomainError> {
        self.tags_result.clone()
    }

    fn worktrees(&self) -> Result<Vec<Worktree>, DomainError> {
        self.worktrees_result.clone()
    }

    fn add_worktree(
        &self,
        _path: &str,
        _branch: &str,
        _create_branch: bool,
        _start_point: Option<&str>,
    ) -> Result<(), DomainError> {
        self.add_worktree_result.clone()
    }

    fn remove_worktree(&self, _path: &str, _force: bool) -> Result<(), DomainError> {
        self.remove_worktree_result.clone()
    }

    fn checkout_branch(&self, _branch_name: &str) -> Result<(), DomainError> {
        self.checkout_result.clone()
    }

    fn checkout_detached(&self, _commit_id: &str) -> Result<(), DomainError> {
        self.checkout_result.clone()
    }

    fn create_branch(&self, _name: &str, _start_point: &str) -> Result<(), DomainError> {
        self.create_branch_result.clone()
    }

    fn rename_branch(&self, _from: &str, _to: &str) -> Result<(), DomainError> {
        self.rename_branch_result.clone()
    }

    fn delete_branch(&self, _name: &str) -> Result<(), DomainError> {
        self.delete_branch_result.clone()
    }

    fn cherry_pick(&self, _commit_id: &str) -> Result<(), DomainError> {
        self.cherry_pick_result.clone()
    }

    fn reset(&self, _commit_id: &str, _mode: core_domain::ResetMode) -> Result<(), DomainError> {
        self.reset_result.clone()
    }

    fn merge_branch(&self, _name: &str, _no_ff: bool) -> Result<(), DomainError> {
        self.merge_result.clone()
    }

    fn rebase_onto(&self, _name: &str) -> Result<(), DomainError> {
        self.rebase_result.clone()
    }

    fn stage_paths(&self, _paths: &[String]) -> Result<(), DomainError> {
        self.stage_result.clone()
    }

    fn unstage_paths(&self, _paths: &[String]) -> Result<(), DomainError> {
        self.unstage_result.clone()
    }

    fn untrack_paths(&self, _paths: &[String]) -> Result<(), DomainError> {
        self.untrack_result.clone()
    }

    fn commit(&self, _message: &str) -> Result<(), DomainError> {
        self.commit_result.clone()
    }

    fn fetch(&self) -> Result<(), DomainError> {
        self.fetch_result.clone()
    }

    fn push(&self) -> Result<(), DomainError> {
        self.push_result.clone()
    }

    fn pull(&self) -> Result<(), DomainError> {
        self.pull_result.clone()
    }

    fn commit_patch(&self, _commit_id: &str) -> Result<Vec<Patch>, DomainError> {
        self.commit_patch_result.clone()
    }

    fn file_patch(&self, _path: &str, _place: FileDisposition) -> Result<Patch, DomainError> {
        self.file_patch_result.clone()
    }

    fn discard_path(&self, _path: &str, _place: FileDisposition) -> Result<(), DomainError> {
        self.discard_result.clone()
    }

    fn stage_hunk(&self, _path: &str, _hunk: &Hunk) -> Result<(), DomainError> {
        self.stage_hunk_result.clone()
    }

    fn discard_hunk(
        &self,
        _path: &str,
        _hunk: &Hunk,
        _place: FileDisposition,
    ) -> Result<(), DomainError> {
        self.discard_hunk_result.clone()
    }

    fn stash_push(&self, _message: &str, _include_untracked: bool) -> Result<(), DomainError> {
        self.stash_push_result.clone()
    }

    fn stash_push_paths(&self, _message: &str, _paths: &[String]) -> Result<(), DomainError> {
        self.stash_push_result.clone()
    }

    fn append_gitignore(&self, _patterns: &[String]) -> Result<(), DomainError> {
        Ok(())
    }

    fn stash_apply(&self, _index: usize) -> Result<(), DomainError> {
        self.stash_apply_result.clone()
    }

    fn stash_pop(&self, _index: usize) -> Result<(), DomainError> {
        self.stash_pop_result.clone()
    }

    fn stash_drop(&self, _index: usize) -> Result<(), DomainError> {
        self.stash_drop_result.clone()
    }

    fn abort_operation(&self, _operation: core_domain::RepoOperation) -> Result<(), DomainError> {
        self.abort_result.clone()
    }

    fn continue_operation(&self, _operation: core_domain::RepoOperation) -> Result<(), DomainError> {
        self.continue_result.clone()
    }

    fn resolve_conflict(&self, _path: &str, _side: core_domain::ConflictSide) -> Result<(), DomainError> {
        self.resolve_result.clone()
    }

    fn compare_file(&self, path: &str, _left: &str, _right: Option<&str>) -> Result<Patch, DomainError> {
        self.compare_result.clone().map(|mut patch| {
            patch.path = path.to_string();
            patch
        })
    }
}
