//! Implements the `GitRepository` port using `gix` (gitoxide).
//! Porcelain that gix 0.66 cannot do safely goes through [`command`].

mod command;
mod status;

#[cfg(test)]
mod tests;

use std::fmt::Display;

use command::{run_git, run_git_paths, BLANK_COMMIT_MESSAGE};
use core_domain::{Branch, Commit, DomainError, GitRepository, RepoStatus};
use gix::bstr::BString;
use gix::refs::TargetRef;
use status::{record_change, staged_paths, tracking_status, unborn_worktree_changes};

pub(crate) fn unknown(error: impl Display) -> DomainError {
    DomainError::Unknown(error.to_string())
}

pub struct GitoxideAdapter {
    repo_path: String,
}

impl GitoxideAdapter {
    pub fn new(repo_path: &str) -> Self {
        Self {
            repo_path: repo_path.to_string(),
        }
    }

    /// `Ok` when `repo_path` can be opened as a Git repository.
    pub fn discover(repo_path: &str) -> Result<Self, DomainError> {
        let adapter = Self::new(repo_path);
        adapter.open_repo()?;
        Ok(adapter)
    }

    fn open_repo(&self) -> Result<gix::Repository, DomainError> {
        gix::open(&self.repo_path).map_err(|error| match error {
            gix::open::Error::NotARepository { .. } => DomainError::RepositoryNotFound,
            _ => DomainError::Unknown(error.to_string()),
        })
    }
}

/// Tips of every local branch, plus `HEAD` when it points at a commit.
///
/// A detached `HEAD` is not a branch tip, and the commit on screen should still appear.
/// Symbolic branch names are skipped; their target is already a tip when it is local.
fn local_history_tips(repo: &gix::Repository) -> Result<Vec<gix::ObjectId>, DomainError> {
    let mut tips = Vec::new();
    if let Some(head) = repo.head().map_err(unknown)?.id() {
        tips.push(head.detach());
    }
    for reference in repo
        .references()
        .map_err(unknown)?
        .local_branches()
        .map_err(unknown)?
    {
        let reference = reference.map_err(unknown)?;
        if let Some(id) = reference.try_id() {
            tips.push(id.detach());
        }
    }
    Ok(tips)
}

impl GitRepository for GitoxideAdapter {
    fn status(&self) -> Result<RepoStatus, DomainError> {
        let repo = self.open_repo()?;
        let head_is_unborn = repo.head().map_err(unknown)?.is_unborn();
        let mut status = RepoStatus {
            staged_files: staged_paths(&repo, head_is_unborn)?,
            unstaged_files: vec![],
            untracked_files: vec![],
            tracking: tracking_status(&self.repo_path)?,
        };

        // Index ↔ worktree: modified/removed tracked files and untracked files, one path each.
        if head_is_unborn {
            for (summary, path) in unborn_worktree_changes(&repo)? {
                record_change(&mut status, summary, path);
            }
        } else {
            let changes = repo
                .status(gix::progress::Discard)
                .map_err(unknown)?
                .untracked_files(gix::status::UntrackedFiles::Files)
                .into_index_worktree_iter(Vec::<BString>::new())
                .map_err(unknown)?;
            for change in changes {
                let change = change.map_err(unknown)?;
                // `None` is a stat-only refresh of an unchanged entry.
                if let Some(summary) = change.summary() {
                    record_change(&mut status, summary, change.rela_path().to_string());
                }
            }
        }

        // The worktree is walked in parallel, so impose a stable order.
        status.unstaged_files.sort();
        status.untracked_files.sort();
        Ok(status)
    }

    fn log(&self, max_count: Option<usize>) -> Result<Vec<Commit>, DomainError> {
        let repo = self.open_repo()?;
        let tips = local_history_tips(&repo)?;
        if tips.is_empty() {
            return Ok(vec![]);
        }

        let walk = repo
            .rev_walk(tips)
            .sorting(gix::traverse::commit::simple::Sorting::ByCommitTimeNewestFirst)
            .all()
            .map_err(unknown)?;

        walk.take(max_count.unwrap_or(usize::MAX))
            .map(|info| {
                let info = info.map_err(unknown)?;
                let commit = info.object().map_err(unknown)?;
                let author = commit.author().map_err(unknown)?;
                Ok(Commit {
                    id: info.id.to_string(),
                    parent_ids: info.parent_ids.iter().map(|id| id.to_string()).collect(),
                    author: author.name.to_string(),
                    message: commit.message().map_err(unknown)?.summary().to_string(),
                    timestamp: commit.time().map_err(unknown)?.seconds,
                })
            })
            .collect()
    }

    fn branches(&self) -> Result<Vec<Branch>, DomainError> {
        let repo = self.open_repo()?;
        let head_name = repo.head_name().map_err(unknown)?;
        let references = repo.references().map_err(unknown)?;

        let local = references
            .local_branches()
            .map_err(unknown)?
            .map(|reference| (reference, false));
        let remote = references
            .remote_branches()
            .map_err(unknown)?
            .map(|reference| (reference, true));

        let mut branches = Vec::new();
        for (reference, is_remote) in local.chain(remote) {
            let reference = reference.map_err(unknown)?;
            if matches!(reference.target(), TargetRef::Symbolic(_)) {
                continue;
            }
            branches.push(Branch {
                name: reference.name().shorten().to_string(),
                is_remote,
                is_head: head_name
                    .as_ref()
                    .is_some_and(|head| head.as_ref() == reference.name()),
            });
        }
        Ok(branches)
    }

    /// Switch to a local branch with `git switch`.
    ///
    /// gix 0.66 keeps the checkout options a correct worktree update needs (filters,
    /// attributes, line endings) private, so this delegates to git, which also refuses
    /// to overwrite uncommitted changes. `--no-guess` stops a remote branch name from
    /// silently creating a new local tracking branch.
    fn checkout_branch(&self, branch_name: &str) -> Result<(), DomainError> {
        self.open_repo()?;
        if branch_name.is_empty() || branch_name.starts_with('-') {
            return Err(DomainError::Unknown(format!(
                "invalid branch name: {branch_name:?}"
            )));
        }
        run_git(&self.repo_path, &["switch", "--no-guess", branch_name])
    }

    fn stage_paths(&self, paths: &[String]) -> Result<(), DomainError> {
        self.open_repo()?;
        run_git_paths(&self.repo_path, &["add"], paths)
    }

    fn unstage_paths(&self, paths: &[String]) -> Result<(), DomainError> {
        self.open_repo()?;
        run_git_paths(&self.repo_path, &["restore", "--staged"], paths)
    }

    /// Commit the index with `git commit -m`.
    ///
    /// Identity, hooks, and `commit.gpgsign` stay the user's git config.
    fn commit(&self, message: &str) -> Result<(), DomainError> {
        self.open_repo()?;
        if message.trim().is_empty() {
            return Err(DomainError::Unknown(BLANK_COMMIT_MESSAGE.to_string()));
        }
        run_git(&self.repo_path, &["commit", "-m", message])
    }

    /// Fetch the default remote with `git fetch`.
    ///
    /// Credential helpers, SSH config, and the agent stay the user's. A prompt is refused
    /// (`GIT_TERMINAL_PROMPT=0` in `run_git`) and git's own message is returned.
    fn fetch(&self) -> Result<(), DomainError> {
        self.open_repo()?;
        run_git(&self.repo_path, &["fetch"])
    }

    /// Push the current branch with `git push` and no refspec.
    ///
    /// Git applies the branch's upstream. Credential helpers and SSH config stay the user's.
    fn push(&self) -> Result<(), DomainError> {
        self.open_repo()?;
        run_git(&self.repo_path, &["push"])
    }

    /// Pull with `git pull --no-rebase --ff-only`.
    ///
    /// `--no-rebase` wins over `pull.rebase`, so a diverged branch is refused instead of
    /// replayed. `--ff-only` refuses a merge commit. The worktree moves only for a fast-forward.
    fn pull(&self) -> Result<(), DomainError> {
        self.open_repo()?;
        run_git(&self.repo_path, &["pull", "--no-rebase", "--ff-only"])
    }
}
