//! Implements the `GitRepository` port using `gix` (gitoxide).
//! Porcelain that gix 0.66 cannot do safely goes through [`command`].

mod command;
mod diff;
mod stash;
mod status;
mod worktree;

#[cfg(test)]
mod tests;

use std::fmt::Display;

use command::{run_git, run_git_paths, run_git_stdin, run_git_stdout, BLANK_COMMIT_MESSAGE};
use core_domain::{
    Branch, Commit, DomainError, FileDisposition, GitRepository, Hunk, Patch, RepoStatus,
    ResetMode, Stash, Tag, Worktree,
};
use diff::{parse_patches, patch_from_worktree, render_hunk};
use gix::bstr::BString;
use gix::refs::TargetRef;
use status::{record_change, staged_paths, tracking_status, unborn_worktree_changes};
use worktree::{parse_worktree_porcelain, require_worktree_path};

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

    /// `git init` in an existing folder that is not already a repository.
    pub fn init(path: &str) -> Result<(), DomainError> {
        validate_directory(path)?;
        if Self::discover(path).is_ok() {
            return Err(DomainError::Unknown(
                "Already a Git repository.".to_string(),
            ));
        }
        run_git(path, &["init"])
    }

    /// `git clone` of `url` into a new directory under `parent`. Returns that directory.
    ///
    /// The name comes from the URL. No hosting provider is contacted except by git itself.
    pub fn clone_repository(url: &str, parent: &str) -> Result<String, DomainError> {
        validate_clone_url(url)?;
        validate_directory(parent)?;
        let name = repository_name(url)?;
        let destination = std::path::Path::new(parent).join(&name);
        if destination.exists() {
            return Err(DomainError::Unknown(format!(
                "{name} already exists in {parent}"
            )));
        }
        run_git(parent, &["clone", "--", url.trim(), &name])?;
        Ok(destination.to_string_lossy().into_owned())
    }

    fn open_repo(&self) -> Result<gix::Repository, DomainError> {
        gix::open(&self.repo_path).map_err(|error| match error {
            gix::open::Error::NotARepository { .. } => DomainError::RepositoryNotFound,
            _ => DomainError::Unknown(error.to_string()),
        })
    }
}

fn walk_commits(
    repo: &gix::Repository,
    tips: Vec<gix::ObjectId>,
    limit: usize,
) -> Result<Vec<Commit>, DomainError> {
    if tips.is_empty() || limit == 0 {
        return Ok(vec![]);
    }
    let walk = repo
        .rev_walk(tips)
        .sorting(gix::traverse::commit::simple::Sorting::ByCommitTimeNewestFirst)
        .all()
        .map_err(unknown)?;

    walk.take(limit)
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
        let head_commit_id = if head_is_unborn {
            None
        } else {
            repo.head()
                .ok()
                .and_then(|head| head.id().map(|id| id.to_string()))
        };
        let mut status = RepoStatus {
            staged_files: staged_paths(&repo, head_is_unborn)?,
            unstaged_files: vec![],
            untracked_files: vec![],
            tracking: tracking_status(&self.repo_path)?,
            head_commit_id,
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
        let limit = max_count.unwrap_or(usize::MAX);
        let head_id = repo
            .head()
            .ok()
            .and_then(|head| head.id().map(|id| id.detach()));

        let mut commits = walk_commits(&repo, tips, limit)?;

        // Other local branches can fill a multi-commit page with newer tips and
        // push HEAD off it — then Focus / the current checkout looks empty.
        // For page-sized loads, keep the checkout tip (and its ancestors) present.
        if limit > 1 {
            if let Some(head) = head_id {
                let head_key = head.to_string();
                if !commits.iter().any(|commit| commit.id == head_key) {
                    let mut from_head = walk_commits(&repo, vec![head], limit)?;
                    let mut seen: std::collections::HashSet<String> =
                        from_head.iter().map(|c| c.id.clone()).collect();
                    for commit in commits {
                        if seen.insert(commit.id.clone()) {
                            from_head.push(commit);
                        }
                    }
                    from_head.truncate(limit);
                    commits = from_head;
                }
            }
        }

        Ok(commits)
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
            let mut reference = reference.map_err(unknown)?;
            if matches!(reference.target(), TargetRef::Symbolic(_)) {
                continue;
            }
            let name = reference.name().shorten().to_string();
            let is_head = head_name
                .as_ref()
                .is_some_and(|head| head.as_ref() == reference.name());
            let Ok(commit) = reference.peel_to_commit() else {
                continue;
            };
            branches.push(Branch {
                name,
                is_remote,
                is_head,
                tip: commit.id().to_string(),
                ahead: 0,
                behind: 0,
            });
        }
        if let Ok(tracking) = status::local_branch_tracking(&self.repo_path) {
            for branch in &mut branches {
                if branch.is_remote {
                    continue;
                }
                if let Some(&(ahead, behind)) = tracking.get(&branch.name) {
                    branch.ahead = ahead;
                    branch.behind = behind;
                }
            }
        }
        Ok(branches)
    }

    fn tags(&self) -> Result<Vec<Tag>, DomainError> {
        let repo = self.open_repo()?;
        let mut tags = Vec::new();
        for reference in repo
            .references()
            .map_err(unknown)?
            .tags()
            .map_err(unknown)?
        {
            let mut reference = reference.map_err(unknown)?;
            let name = reference.name().shorten().to_string();
            let Ok(commit) = reference.peel_to_commit() else {
                continue;
            };
            tags.push(Tag {
                name,
                commit_id: commit.id().to_string(),
            });
        }
        tags.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(tags)
    }

    fn worktrees(&self) -> Result<Vec<Worktree>, DomainError> {
        self.open_repo()?;
        let out = run_git_stdout(&self.repo_path, &["worktree", "list", "--porcelain"])?;
        Ok(parse_worktree_porcelain(&out))
    }

    fn add_worktree(
        &self,
        path: &str,
        branch: &str,
        create_branch: bool,
        start_point: Option<&str>,
    ) -> Result<(), DomainError> {
        self.open_repo()?;
        require_worktree_path(path)?;
        require_branch_name(branch)?;
        if create_branch {
            match start_point {
                Some(start) => {
                    if start.is_empty() || start.starts_with('-') {
                        return Err(DomainError::Unknown(format!(
                            "invalid start point: {start:?}"
                        )));
                    }
                    // `git worktree add -b <branch> <path> <commit-ish>`
                    run_git(
                        &self.repo_path,
                        &["worktree", "add", "-b", branch, path, start],
                    )
                }
                None => run_git(
                    &self.repo_path,
                    &["worktree", "add", "-b", branch, "--", path],
                ),
            }
        } else {
            run_git(&self.repo_path, &["worktree", "add", "--", path, branch])
        }
    }

    fn remove_worktree(&self, path: &str, force: bool) -> Result<(), DomainError> {
        self.open_repo()?;
        require_worktree_path(path)?;
        if force {
            run_git(
                &self.repo_path,
                &["worktree", "remove", "--force", "--", path],
            )
        } else {
            run_git(&self.repo_path, &["worktree", "remove", "--", path])
        }
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

    fn checkout_detached(&self, commit_id: &str) -> Result<(), DomainError> {
        self.open_repo()?;
        if commit_id.is_empty() || commit_id.starts_with('-') {
            return Err(DomainError::Unknown(format!(
                "invalid commit id: {commit_id:?}"
            )));
        }
        run_git(&self.repo_path, &["checkout", "--detach", commit_id])
    }

    fn create_branch(&self, name: &str, start_point: &str) -> Result<(), DomainError> {
        self.open_repo()?;
        require_branch_name(name)?;
        require_branch_name(start_point)?;
        run_git(&self.repo_path, &["branch", "--", name, start_point])
    }

    fn rename_branch(&self, from: &str, to: &str) -> Result<(), DomainError> {
        self.open_repo()?;
        require_branch_name(from)?;
        require_branch_name(to)?;
        run_git(&self.repo_path, &["branch", "-m", "--", from, to])
    }

    fn delete_branch(&self, name: &str) -> Result<(), DomainError> {
        self.open_repo()?;
        require_branch_name(name)?;
        run_git(&self.repo_path, &["branch", "-d", "--", name])
    }

    fn cherry_pick(&self, commit_id: &str) -> Result<(), DomainError> {
        self.open_repo()?;
        require_hex_commit(commit_id)?;
        run_git(&self.repo_path, &["cherry-pick", "--", commit_id])
    }

    fn reset(&self, commit_id: &str, mode: ResetMode) -> Result<(), DomainError> {
        self.open_repo()?;
        require_hex_commit(commit_id)?;
        let flag = match mode {
            ResetMode::Soft => "--soft",
            ResetMode::Mixed => "--mixed",
            ResetMode::Hard => "--hard",
        };
        // A hex id cannot be an option. `--` would make git treat it as a path.
        run_git(&self.repo_path, &["reset", flag, commit_id])
    }

    fn merge_branch(&self, name: &str) -> Result<(), DomainError> {
        self.open_repo()?;
        require_branch_name(name)?;
        run_git(&self.repo_path, &["merge", "--no-ff", "--", name])
    }

    fn rebase_onto(&self, name: &str) -> Result<(), DomainError> {
        self.open_repo()?;
        require_branch_name(name)?;
        run_git(&self.repo_path, &["rebase", "--", name])
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

    fn commit_patch(&self, commit_id: &str) -> Result<Vec<Patch>, DomainError> {
        self.open_repo()?;
        if !is_hex_revision(commit_id) {
            return Err(DomainError::Unknown(format!(
                "invalid commit: {commit_id:?}"
            )));
        }
        let text = run_git_stdout(
            &self.repo_path,
            &["show", "--format=", "--unified=3", commit_id],
        )?;
        Ok(parse_patches(&text))
    }

    fn file_patch(&self, path: &str, place: FileDisposition) -> Result<Patch, DomainError> {
        self.open_repo()?;
        require_safe_path(path)?;
        if place == FileDisposition::Untracked {
            let full = std::path::Path::new(&self.repo_path).join(path);
            let bytes = std::fs::read(&full)
                .map_err(|error| DomainError::Unknown(format!("could not read {path}: {error}")))?;
            return Ok(patch_from_worktree(path, &bytes));
        }
        let mut args = vec!["diff", "--unified=3"];
        if place == FileDisposition::Staged {
            args.push("--cached");
        }
        args.push("--");
        args.push(path);
        let text = run_git_stdout(&self.repo_path, &args)?;
        Ok(parse_patches(&text).into_iter().next().unwrap_or(Patch {
            path: path.to_string(),
            hunks: Vec::new(),
            binary: false,
        }))
    }

    fn discard_path(&self, path: &str, place: FileDisposition) -> Result<(), DomainError> {
        self.open_repo()?;
        require_safe_path(path)?;
        if place == FileDisposition::Untracked {
            let full = std::path::Path::new(&self.repo_path).join(path);
            return std::fs::remove_file(full).map_err(|error| {
                DomainError::Unknown(format!("could not discard {path}: {error}"))
            });
        }
        if tracked_at_head(&self.repo_path, path)? {
            run_git(
                &self.repo_path,
                &[
                    "restore",
                    "--source=HEAD",
                    "--staged",
                    "--worktree",
                    "--",
                    path,
                ],
            )
        } else {
            run_git(&self.repo_path, &["rm", "-f", "--", path])
        }
    }

    fn stage_hunk(&self, path: &str, hunk: &Hunk) -> Result<(), DomainError> {
        self.open_repo()?;
        require_safe_path(path)?;
        let patch = render_hunk(path, hunk)?;
        run_git_stdin(
            &self.repo_path,
            &["apply", "--cached", "--recount", "--whitespace=nowarn"],
            patch.as_bytes(),
        )
    }

    fn discard_hunk(
        &self,
        path: &str,
        hunk: &Hunk,
        place: FileDisposition,
    ) -> Result<(), DomainError> {
        self.open_repo()?;
        require_safe_path(path)?;
        let patch = render_hunk(path, hunk)?;
        let args: &[&str] = if place == FileDisposition::Staged {
            &[
                "apply",
                "--cached",
                "--reverse",
                "--recount",
                "--whitespace=nowarn",
            ]
        } else {
            &["apply", "--reverse", "--recount", "--whitespace=nowarn"]
        };
        run_git_stdin(&self.repo_path, args, patch.as_bytes())
    }

    fn stashes(&self) -> Result<Vec<Stash>, DomainError> {
        self.open_repo()?;
        let text = run_git_stdout(&self.repo_path, &["stash", "list"])?;
        stash::parse_stash_list(&text)
    }

    fn stash_push(&self, message: &str) -> Result<(), DomainError> {
        self.stash_push_paths(message, &[])
    }

    fn stash_push_paths(&self, message: &str, paths: &[String]) -> Result<(), DomainError> {
        self.open_repo()?;
        if message.contains('\0') {
            return Err(DomainError::Unknown(
                "stash message contains a null byte".to_string(),
            ));
        }
        let message = message.trim();
        let mut args: Vec<&str> = vec!["stash", "push"];
        if !message.is_empty() {
            args.push("-m");
            args.push(message);
        }
        if paths.is_empty() {
            run_git(&self.repo_path, &args)
        } else {
            run_git_paths(&self.repo_path, &args, paths)
        }
    }

    fn append_gitignore(&self, patterns: &[String]) -> Result<(), DomainError> {
        self.open_repo()?;
        if patterns.is_empty() {
            return Err(DomainError::Unknown(
                "gitignore patterns must not be empty".to_string(),
            ));
        }
        for pattern in patterns {
            require_gitignore_pattern(pattern)?;
        }
        let path = std::path::Path::new(&self.repo_path).join(".gitignore");
        let mut body = if path.exists() {
            std::fs::read_to_string(&path).map_err(unknown)?
        } else {
            String::new()
        };
        if !body.is_empty() && !body.ends_with('\n') {
            body.push('\n');
        }
        for pattern in patterns {
            if body.lines().any(|line| line.trim() == pattern.trim()) {
                continue;
            }
            body.push_str(pattern.trim());
            body.push('\n');
        }
        std::fs::write(&path, body).map_err(unknown)
    }

    fn stash_apply(&self, index: usize) -> Result<(), DomainError> {
        self.open_repo()?;
        let stash_ref = stash_ref(index);
        run_git(&self.repo_path, &["stash", "apply", &stash_ref])
    }

    fn stash_pop(&self, index: usize) -> Result<(), DomainError> {
        self.open_repo()?;
        let stash_ref = stash_ref(index);
        run_git(&self.repo_path, &["stash", "pop", &stash_ref])
    }

    fn stash_drop(&self, index: usize) -> Result<(), DomainError> {
        self.open_repo()?;
        let stash_ref = stash_ref(index);
        run_git(&self.repo_path, &["stash", "drop", &stash_ref])
    }
}

fn stash_ref(index: usize) -> String {
    format!("stash@{{{index}}}")
}

fn validate_directory(path: &str) -> Result<(), DomainError> {
    if path.trim().is_empty() || path.contains('\0') || path.starts_with('-') {
        Err(DomainError::Unknown(format!("invalid path: {path:?}")))
    } else {
        Ok(())
    }
}

fn validate_clone_url(url: &str) -> Result<(), DomainError> {
    let url = url.trim();
    if url.is_empty() || url.contains(['\0', '\n', '\r']) || url.starts_with('-') {
        Err(DomainError::Unknown(format!("invalid clone URL: {url:?}")))
    } else {
        Ok(())
    }
}

/// The last path segment of a clone URL, without a trailing `.git`.
fn repository_name(url: &str) -> Result<String, DomainError> {
    let trimmed = url.trim().trim_end_matches(['/', '\\']);
    let segment = trimmed.rsplit(['/', '\\', ':']).next().unwrap_or("").trim();
    let name = segment.strip_suffix(".git").unwrap_or(segment);
    if name.is_empty() || name == "." || name == ".." {
        Err(DomainError::Unknown(
            "Clone URL has no repository name.".to_string(),
        ))
    } else {
        Ok(name.to_string())
    }
}

fn is_hex_revision(commit_id: &str) -> bool {
    let length = commit_id.len();
    (4..=40).contains(&length) && commit_id.chars().all(|c| c.is_ascii_hexdigit())
}

fn require_hex_commit(commit_id: &str) -> Result<(), DomainError> {
    if is_hex_revision(commit_id) {
        Ok(())
    } else {
        Err(DomainError::Unknown(format!(
            "invalid commit: {commit_id:?}"
        )))
    }
}

fn require_branch_name(name: &str) -> Result<(), DomainError> {
    let component_bad = name
        .split('/')
        .any(|part| part.is_empty() || part == "." || part == ".." || part.ends_with(".lock"));
    let bad = name.is_empty()
        || name.starts_with('-')
        || name.starts_with('/')
        || name.ends_with('/')
        || component_bad
        || name.contains([
            '\0', '\n', '\r', ' ', '\t', '~', '^', ':', '?', '*', '[', '\\',
        ])
        || name.contains("..")
        || name.contains("@{");
    if bad {
        Err(DomainError::Unknown(format!(
            "invalid branch name: {name:?}"
        )))
    } else {
        Ok(())
    }
}

fn require_safe_path(path: &str) -> Result<(), DomainError> {
    let candidate = std::path::Path::new(path);
    let escapes = path.is_empty()
        || path.contains('\0')
        || path.starts_with('-')
        || candidate.is_absolute()
        || candidate
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir));
    if escapes {
        Err(DomainError::Unknown(format!("invalid path: {path:?}")))
    } else {
        Ok(())
    }
}

fn require_gitignore_pattern(pattern: &str) -> Result<(), DomainError> {
    let trimmed = pattern.trim();
    if trimmed.is_empty() || trimmed.contains('\0') || trimmed.contains(['\n', '\r']) {
        return Err(DomainError::Unknown(format!(
            "invalid gitignore pattern: {pattern:?}"
        )));
    }
    // Strip a trailing slash for the path check; folder patterns keep it when written.
    let for_path = trimmed.trim_end_matches('/');
    require_safe_path(for_path)
}

fn tracked_at_head(repo_path: &str, path: &str) -> Result<bool, DomainError> {
    match run_git_stdout(repo_path, &["ls-tree", "--name-only", "HEAD", "--", path]) {
        Ok(names) => Ok(!names.is_empty()),
        Err(DomainError::Unknown(message))
            if message.to_ascii_lowercase().contains("not a valid object")
                || message.to_ascii_lowercase().contains("bad revision")
                || message.to_ascii_lowercase().contains("ambiguous argument") =>
        {
            Ok(false)
        }
        Err(error) => Err(error),
    }
}
