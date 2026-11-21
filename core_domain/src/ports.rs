use super::{MergePreview, 
    ActivityRecord, AppSettings, Branch, Commit, ConflictSide, DagNode, DomainError, FileDisposition, ForceGuard, Hunk, LineCounts, Patch,
    RepoOperation, RepoStatus, ResetMode, Stash, Tag, Worktree,
};

/// Older commits one page at a time, from a walk that continues where the last page ended.
/// Made by [`GitRepository::log_cursor`]. A page shorter than asked means the history has
/// ended; every page after that is empty.
pub trait LogCursor: Send {
    fn next_page(&mut self, count: usize) -> Result<Vec<Commit>, DomainError>;
}

/// The whole remaining log, served in pages. What [`GitRepository::log_cursor`] gives when an
/// adapter has no walk to keep open.
struct ListCursor {
    rest: std::collections::VecDeque<Commit>,
}

impl LogCursor for ListCursor {
    fn next_page(&mut self, count: usize) -> Result<Vec<Commit>, DomainError> {
        let taken = count.min(self.rest.len());
        Ok(self.rest.drain(..taken).collect())
    }
}

/// The GitRepository port.
/// Adapters such as `git_adapter` implement this trait.
pub trait GitRepository {
    fn status(&self) -> Result<RepoStatus, DomainError>;
    fn log(&self, max_count: Option<usize>) -> Result<Vec<Commit>, DomainError>;
    /// The same history as [`Self::log`], newest first, without the commits in
    /// `already_loaded`, read in pages that continue one walk: nothing is read twice.
    /// Adapters that cannot keep a walk open read the log once and page through that.
    fn log_cursor(&self, already_loaded: Vec<String>) -> Result<Box<dyn LogCursor>, DomainError> {
        let loaded: std::collections::HashSet<String> = already_loaded.into_iter().collect();
        let rest = self
            .log(None)?
            .into_iter()
            .filter(|commit| !loaded.contains(&commit.id))
            .collect();
        Ok(Box::new(ListCursor { rest }))
    }
    fn branches(&self) -> Result<Vec<Branch>, DomainError>;
    /// Tags that peel to a commit, sorted by name. The section is empty when there are none.
    fn tags(&self) -> Result<Vec<Tag>, DomainError>;
    /// `git stash list`, in the order git prints it. `stash@{0}` is the newest.
    fn stashes(&self) -> Result<Vec<Stash>, DomainError>;
    /// Ids of commits reachable from HEAD that no remote-tracking branch contains, newest first,
    /// at most 1000. Empty when there are no remotes or HEAD is unborn.
    fn unpushed_commits(&self) -> Result<Vec<String>, DomainError> {
        Err(DomainError::Unknown("this repository cannot list unpushed commits".to_string()))
    }
    /// Added and removed lines for each staged, unstaged and untracked file. `untracked` is the
    /// untracked list the caller already read from `status`, so the worktree is not walked again.
    fn line_counts(&self, _untracked: &[String]) -> Result<LineCounts, DomainError> {
        Err(DomainError::Unknown("this repository cannot count changed lines".to_string()))
    }
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
    /// `git revert --no-edit -- <commit>`: a new commit undoes `commit` on the current branch.
    /// `commit` is a hex object id. A conflict leaves the revert in progress
    /// ([`RepoStatus::operation`]) and returns git's message.
    fn revert(&self, _commit_id: &str) -> Result<(), DomainError> {
        Err(DomainError::Unknown("this repository cannot revert a commit".to_string()))
    }
    /// `git reset --soft`, `--mixed`, or `--hard` to a hex object id.
    fn reset(&self, commit_id: &str, mode: ResetMode) -> Result<(), DomainError>;
    /// `git merge [--no-ff] -- <name>`. With `no_ff` a merge commit is recorded even when a
    /// fast-forward is possible. A conflict leaves the merge in progress
    /// ([`RepoStatus::operation`]) and returns git's message.
    fn merge_branch(&self, name: &str, no_ff: bool) -> Result<(), DomainError>;
    /// `git rebase <name>`. Replays the current branch onto that tip.
    fn rebase_onto(&self, name: &str) -> Result<(), DomainError>;
    fn stage_paths(&self, paths: &[String]) -> Result<(), DomainError>;
    fn unstage_paths(&self, paths: &[String]) -> Result<(), DomainError>;
    /// `git rm --cached -r -- <paths>`: stop tracking, keep the files on disk. The next commit
    /// records the removal and the files show as unversioned. Git refuses a path whose staged
    /// content differs from both HEAD and the file, so no staged work is lost.
    fn untrack_paths(&self, paths: &[String]) -> Result<(), DomainError>;
    fn commit(&self, message: &str) -> Result<(), DomainError>;
    /// `git commit --amend -m <message>`: replace the HEAD commit with one that has this
    /// message and the staged changes. Refused with [`DomainError::HeadMoved`], changing
    /// nothing, when HEAD is not `expected_head`.
    fn amend(&self, message: &str, expected_head: &str) -> Result<(), DomainError> {
        let _ = (message, expected_head);
        Err(DomainError::Unknown("this repository cannot amend a commit".to_string()))
    }
    /// Take the HEAD commit off the branch and keep its changes staged:
    /// `git update-ref HEAD <parent> <expected_head>`, a compare-and-swap that moves the branch
    /// only if HEAD is still `expected_head`, and leaves the index and the worktree as they are.
    /// Returns the undone commit's full message. Changes nothing and refuses:
    /// - with [`DomainError::HeadMoved`] when HEAD is not `expected_head`;
    /// - while a merge, rebase, cherry-pick or revert is in progress
    ///   ("Finish or abort the <noun> first.") or the index has unmerged entries;
    /// - for a commit with no parent, and for a merge commit.
    fn undo_commit(&self, expected_head: &str) -> Result<String, DomainError> {
        let _ = expected_head;
        Err(DomainError::Unknown("this repository cannot undo a commit".to_string()))
    }
    /// The full message of one commit (summary, blank line, body), without the trailing
    /// newline. `commit_id` is a hex object id.
    fn commit_message(&self, commit_id: &str) -> Result<String, DomainError> {
        let _ = commit_id;
        Err(DomainError::Unknown("this repository cannot read a commit message".to_string()))
    }
    /// `git fetch` of the default remote, using the repository's own credential helper and SSH config.
    fn fetch(&self) -> Result<(), DomainError>;
    /// `git push` of the current branch to its upstream, with no refspec.
    fn push(&self) -> Result<(), DomainError>;
    /// The remotes' names, as `git remote` lists them. Empty when there are none.
    fn remotes(&self) -> Result<Vec<String>, DomainError> {
        Err(DomainError::Unknown("this repository cannot list remotes".to_string()))
    }
    /// `git push --set-upstream -- <remote> HEAD`: send the current branch to `remote` under
    /// its own name and track it from now on. Refuses an empty or option-like remote.
    fn publish(&self, remote: &str) -> Result<(), DomainError> {
        let _ = remote;
        Err(DomainError::Unknown("this repository cannot publish a branch".to_string()))
    }
    /// Replace the current branch's upstream with the local branch:
    /// `git push --force-with-lease [--force-if-includes] -- <remote> HEAD:<merge>`, naming the
    /// remote and branch from `branch.<name>.remote` and `.merge`, so no push configuration
    /// widens it. Refuses, changing nothing, when that upstream is not `expected_upstream`
    /// (the short name the user confirmed, such as `origin/main`), and git refuses when the
    /// remote moved since the last fetch. Returns the guard used.
    fn force_push(&self, expected_upstream: &str) -> Result<ForceGuard, DomainError> {
        let _ = expected_upstream;
        Err(DomainError::Unknown("this repository cannot force push".to_string()))
    }
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
    /// `git stash push [--include-untracked]`. A blank message keeps git's own subject.
    fn stash_push(&self, message: &str, include_untracked: bool) -> Result<(), DomainError>;
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
    /// `git merge --abort`, `git rebase --abort`… Restores the state before the operation.
    fn abort_operation(&self, operation: RepoOperation) -> Result<(), DomainError>;
    /// Finish the operation once every conflict is resolved and staged: the merge commit, or
    /// `--continue` for a rebase, cherry-pick or revert. Git's prepared message is kept.
    fn continue_operation(&self, operation: RepoOperation) -> Result<(), DomainError>;
    /// Keep one side of a conflicted file (`git checkout --ours|--theirs`) and stage it.
    fn resolve_conflict(&self, path: &str, side: ConflictSide) -> Result<(), DomainError>;
    /// What `git merge <name>` would do, without doing it (`git merge-tree`): nothing in the
    /// repository changes.
    fn merge_preview(&self, name: &str) -> Result<MergePreview, DomainError> {
        let _ = name;
        Err(DomainError::Unknown("this repository cannot preview a merge".to_string()))
    }
    /// The merge tool `git mergetool` would run here: `merge.tool` when set, otherwise the one
    /// tool with a `mergetool.<name>.cmd`, otherwise `None`. Never a guess from git's list of
    /// known tools: a guess may be a terminal tool, which would hang with no terminal.
    fn merge_tool(&self) -> Result<Option<String>, DomainError> {
        Ok(None)
    }
    /// `git mergetool --no-prompt --tool <tool> -- <path>`. Blocks until the tool closes. `Ok`
    /// when the merge was saved, which is when git stages the file; an error when the tool
    /// exited unresolved or failed. Cthulhu Resolve joins here once it is configured as a merge
    /// tool (resolver spec §8.4): the window never names it.
    fn run_merge_tool(&self, path: &str, tool: &str) -> Result<(), DomainError> {
        let _ = (path, tool);
        Err(DomainError::Unknown("this repository cannot run a merge tool".to_string()))
    }
    /// Diff of one path between two versions, both in working-tree form. `left` is a revision
    /// (branch, tag, commit, or `:2` / `:3` for the yours / theirs stage of a conflict);
    /// `right` is another, or `None` for the file on disk. A side without the file reads as empty.
    fn compare_file(&self, path: &str, left: &str, right: Option<&str>) -> Result<Patch, DomainError>;
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

/// Port for each repository's activity log, kept between runs.
pub trait ActivityStore {
    /// The newest `limit` records of the repository at `repository`, oldest first.
    /// A repository with no log yet has none.
    fn load_activity(&self, repository: &str, limit: usize) -> Result<Vec<ActivityRecord>, DomainError>;
    fn append_activity(&self, repository: &str, record: &ActivityRecord) -> Result<(), DomainError>;
    fn clear_activity(&self, repository: &str) -> Result<(), DomainError>;
}

/// Port for DAG caching.
pub trait GraphCache {
    fn get_dag(&self, cache_key: &str) -> Result<Option<Vec<DagNode>>, DomainError>;
    fn store_dag(&self, cache_key: &str, dag: &[DagNode]) -> Result<(), DomainError>;
}
