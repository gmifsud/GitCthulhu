use super::{
    AppSettings, Branch, Commit, DagNode, DomainError, FileDisposition, Hunk, Patch, RepoStatus,
    ResetMode, Stash, Tag, Worktree,
};

/// The GitRepository port.
/// Adapters such as `git_adapter` implement this trait.
pub trait GitRepository {
    fn status(&self) -> Result<RepoStatus, DomainError>;
    fn log(&self, max_count: Option<usize>) -> Result<Vec<Commit>, DomainError>;
    fn branches(&self) -> Result<Vec<Branch>, DomainError>;
    /// Tags that peel to a commit, sorted by name. The section is empty when there are none.
    fn tags(&self) -> Result<Vec<Tag>, DomainError>;
    /// `git stash list`, in the order git prints it. `stash@{0}` is the newest.
    fn stashes(&self) -> Result<Vec<Stash>, DomainError>;
    /// `git worktree list --porcelain`. The first row is the main worktree; prunable rows are omitted.
    fn worktrees(&self) -> Result<Vec<Worktree>, DomainError>;
    /// `git worktree add`. With `create_branch`, makes `branch` (`-b`); `start_point` is the tip
    /// when set (otherwise HEAD). Without `create_branch`, checks out the existing `branch`.
    fn add_worktree(
        &self,
        path: &str,
        branch: &str,
        create_branch: bool,
        start_point: Option<&str>,
    ) -> Result<(), DomainError>;
    /// `git worktree remove`. `force` also removes a worktree with uncommitted changes; git refuses the main one.
    fn remove_worktree(&self, path: &str, force: bool) -> Result<(), DomainError>;
    fn checkout_branch(&self, branch_name: &str) -> Result<(), DomainError>;
    /// `git checkout --detach <commit_id>`. Refuses empty or option-like ids.
    fn checkout_detached(&self, commit_id: &str) -> Result<(), DomainError>;
    /// `git branch -- <name> <start_point>`. Does not check the new branch out.
    fn create_branch(&self, name: &str, start_point: &str) -> Result<(), DomainError>;
    /// `git branch -m -- <from> <to>`. Renaming the current branch moves HEAD with it.
    fn rename_branch(&self, from: &str, to: &str) -> Result<(), DomainError>;
    /// `git branch -d -- <name>`. Refuses the current branch and a branch that is not merged.
    fn delete_branch(&self, name: &str) -> Result<(), DomainError>;
    /// `git cherry-pick -- <commit>`. `commit` is a hex object id.
    fn cherry_pick(&self, commit_id: &str) -> Result<(), DomainError>;
    /// `git reset --soft`, `--mixed`, or `--hard` to a hex object id.
    fn reset(&self, commit_id: &str, mode: ResetMode) -> Result<(), DomainError>;
    /// `git merge --no-ff -- <name>`. Records a merge commit even when a fast-forward is possible.
    fn merge_branch(&self, name: &str) -> Result<(), DomainError>;
    /// `git rebase <name>`. Replays the current branch onto that tip.
    fn rebase_onto(&self, name: &str) -> Result<(), DomainError>;
    fn stage_paths(&self, paths: &[String]) -> Result<(), DomainError>;
    fn unstage_paths(&self, paths: &[String]) -> Result<(), DomainError>;
    fn commit(&self, message: &str) -> Result<(), DomainError>;
    /// `git fetch` of the default remote, using the repository's own credential helper and SSH config.
    fn fetch(&self) -> Result<(), DomainError>;
    /// `git push` of the current branch to its upstream, with no refspec.
    fn push(&self) -> Result<(), DomainError>;
    /// `git pull --no-rebase --ff-only`. A diverged branch fails and does not start a merge or rebase.
    fn pull(&self) -> Result<(), DomainError>;
    /// Unified diff of `commit_id` against its first parent. An unknown id is an error.
    fn commit_patch(&self, commit_id: &str) -> Result<Vec<Patch>, DomainError>;
    /// Unified diff of one path. Untracked files are read from the worktree.
    fn file_patch(&self, path: &str, place: FileDisposition) -> Result<Patch, DomainError>;
    /// Drop the worktree change for one path. Untracked files are deleted. Tracked files are restored to `HEAD`.
    fn discard_path(&self, path: &str, place: FileDisposition) -> Result<(), DomainError>;
    /// Apply one hunk to the index.
    fn stage_hunk(&self, path: &str, hunk: &Hunk) -> Result<(), DomainError>;
    /// Reverse one hunk in the worktree, or in the index when `place` is staged.
    fn discard_hunk(
        &self,
        path: &str,
        hunk: &Hunk,
        place: FileDisposition,
    ) -> Result<(), DomainError>;
    /// `git stash push`. A blank message keeps git's own subject. Untracked files stay put.
    fn stash_push(&self, message: &str) -> Result<(), DomainError>;
    /// `git stash push [-- <paths>…]`. Empty `paths` matches [`Self::stash_push`].
    fn stash_push_paths(&self, message: &str, paths: &[String]) -> Result<(), DomainError>;
    /// Append `patterns` to `.gitignore` (creating the file when missing). Patterns are relative.
    fn append_gitignore(&self, patterns: &[String]) -> Result<(), DomainError>;
    /// `git stash apply stash@{index}`. The stash entry remains.
    fn stash_apply(&self, index: usize) -> Result<(), DomainError>;
    /// `git stash pop stash@{index}`. Git drops the entry only when the apply succeeds.
    fn stash_pop(&self, index: usize) -> Result<(), DomainError>;
    /// `git stash drop stash@{index}`. The saved change is discarded.
    fn stash_drop(&self, index: usize) -> Result<(), DomainError>;
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
