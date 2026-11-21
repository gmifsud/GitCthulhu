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
    Branch, CloneRequest, Commit, ConflictSide, DomainError, FileDisposition, GitRepository, Hunk, LogCursor, MergePreview,
    Patch, RepoOperation, RepoStatus, ResetMode, Stash, Tag, Worktree,
};
use diff::{parse_patches, patch_from_worktree, render_hunk};
use gix::bstr::BString;
use gix::refs::TargetRef;
use status::{record_change, staged_changes, tracking_status, unborn_worktree_changes};
use worktree::{parse_worktree_porcelain, require_worktree_path};

pub(crate) fn unknown(error: impl Display) -> DomainError {
    DomainError::Unknown(error.to_string())
}

/// An adapter error with every cause behind it. gix's top-level messages ("Could not create
/// index from tree at …") say what failed, not why; the why is in the sources.
pub(crate) fn unknown_with_causes(error: impl std::error::Error) -> DomainError {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        let cause_text = cause.to_string();
        if !message.contains(&cause_text) {
            message.push_str(": ");
            message.push_str(&cause_text);
        }
        source = cause.source();
    }
    DomainError::Unknown(message)
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
        let request = CloneRequest {
            url: url.to_string(),
            parent: parent.to_string(),
            name: repository_name(url)?,
            branch: None,
        };
        Self::clone_with_progress(&request, &mut |_| {}, &std::sync::atomic::AtomicBool::new(false))
    }

    /// `git clone --progress`, handing each progress line to `progress` as git prints it, and
    /// stopping when `cancel` is set. The new folder is removed if the clone does not finish:
    /// it did not exist before (that is checked first), so nothing of the user's is touched.
    ///
    /// Sign-in is left to git's credential helper (Git Credential Manager opens its own window
    /// for GitHub and Azure DevOps); git never waits on a terminal, which this window has not.
    pub fn clone_with_progress(
        request: &CloneRequest,
        progress: &mut dyn FnMut(&str),
        cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<String, DomainError> {
        use std::io::Read;
        use std::sync::atomic::Ordering;
        validate_clone_url(&request.url)?;
        validate_directory(&request.parent)?;
        validate_folder_name(&request.name)?;
        if let Some(branch) = &request.branch {
            require_branch_name(branch)?;
        }
        let destination = std::path::Path::new(&request.parent).join(&request.name);
        if destination.exists() {
            return Err(DomainError::Unknown(format!("{} already exists in {}", request.name, request.parent)));
        }
        let mut args = vec!["clone", "--progress"];
        if let Some(branch) = &request.branch {
            args.extend(["--branch", branch.as_str()]);
        }
        args.extend(["--", request.url.trim(), request.name.as_str()]);
        let mut child = command::git_program(&request.parent, &args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|error| DomainError::Unknown(format!("could not run git: {error}")))?;
        // git rewrites its progress line in place with \r: read raw and split on both.
        let mut stderr = child.stderr.take().expect("stderr is piped");
        let (lines, received) = std::sync::mpsc::channel::<String>();
        let reader = std::thread::spawn(move || {
            let mut pending = Vec::new();
            let mut chunk = [0u8; 4096];
            while let Ok(read) = stderr.read(&mut chunk) {
                if read == 0 {
                    break;
                }
                for &byte in &chunk[..read] {
                    if byte == b'\r' || byte == b'\n' {
                        if !pending.is_empty() {
                            let _ = lines.send(String::from_utf8_lossy(&pending).into_owned());
                            pending.clear();
                        }
                    } else {
                        pending.push(byte);
                    }
                }
            }
            if !pending.is_empty() {
                let _ = lines.send(String::from_utf8_lossy(&pending).into_owned());
            }
        });
        let mut last = std::collections::VecDeque::with_capacity(CLONE_ERROR_LINES);
        let mut cancelled = false;
        loop {
            if cancel.load(Ordering::Relaxed) && !cancelled {
                cancelled = true;
                let _ = child.kill();
            }
            match received.recv_timeout(std::time::Duration::from_millis(100)) {
                Ok(line) => {
                    progress(&line);
                    if last.len() == CLONE_ERROR_LINES {
                        last.pop_front();
                    }
                    last.push_back(line);
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
        let _ = reader.join();
        let status = child.wait().map_err(|error| DomainError::Unknown(format!("git clone did not finish: {error}")))?;
        if cancelled || !status.success() {
            let leftover = remove_partial_clone(&destination);
            let reason = if cancelled {
                "Clone cancelled.".to_string()
            } else {
                // Only git's own messages, not its progress counters.
                let said: Vec<&str> = last
                    .iter()
                    .map(String::as_str)
                    .filter(|line| !line.contains('%') && !line.starts_with("Cloning into"))
                    .collect();
                if said.is_empty() { "git clone failed.".to_string() } else { said.join("\n") }
            };
            return Err(DomainError::Unknown(match leftover {
                Some(error) => format!("{reason}\nThe partly cloned folder {} could not be removed: {error}", destination.display()),
                None => reason,
            }));
        }
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
        .map(|info| commit_from_info(info.map_err(unknown)?))
        .collect()
}

/// The merge tool to run: `chosen` (`merge.tool`) when set, else the only tool named in
/// `defined` (`mergetool.<name>.cmd` keys, one per line), else none. Names are as git keeps
/// them; a name git could read as an option is refused.
/// The name the built-in resolver runs under. It needs nothing in the user's git config: its
/// command is handed to `git mergetool` for that one run.
pub(crate) const BUILTIN_MERGE_TOOL: &str = "cthulhu-resolve";

/// Cthulhu Resolve, when it is installed beside the app or built in this checkout.
/// `GITCTHULHU_RESOLVER` names it outright. Tests see only that variable, so what they find
/// never depends on what happens to be built on the machine.
fn bundled_resolver() -> Option<std::path::PathBuf> {
    if let Some(named) = std::env::var_os("GITCTHULHU_RESOLVER") {
        let named = std::path::PathBuf::from(named);
        return named.is_file().then_some(named);
    }
    if cfg!(test) {
        return None;
    }
    let file = if cfg!(windows) { "cthulhu-resolve.exe" } else { "cthulhu-resolve" };
    let exe = std::env::current_exe().ok()?;
    exe.ancestors().skip(1).take(4).find_map(|dir| {
        [
            dir.join(file),
            dir.join("resolver").join(file),
            dir.join("resolver/src/Resolve.App/bin/Release/net10.0").join(file),
        ]
        .into_iter()
        .find(|candidate| candidate.is_file())
    })
}

/// The tool to run: the one the user chose in `merge.tool`; otherwise the built-in resolver
/// when it is there; otherwise the only tool with a command. A tool merely defined (a
/// `mergetool.<name>.cmd` left by another program) never outranks the built-in one.
fn pick_merge_tool(chosen: &str, defined: &str, builtin: bool) -> Option<String> {
    let usable = |name: &str| !name.is_empty() && !name.starts_with('-') && !name.contains(char::is_whitespace);
    if usable(chosen) {
        return Some(chosen.to_string());
    }
    if builtin {
        return Some(BUILTIN_MERGE_TOOL.to_string());
    }
    let mut names: Vec<&str> = defined
        .lines()
        .filter_map(|key| key.trim().strip_prefix("mergetool.")?.strip_suffix(".cmd"))
        .filter(|name| usable(name))
        .collect();
    names.sort_unstable();
    names.dedup();
    match names.as_slice() {
        [only] => Some((*only).to_string()),
        _ => None,
    }
}

/// The [`Commit`] a walk step points at.
fn commit_from_info(info: gix::revision::walk::Info<'_>) -> Result<Commit, DomainError> {
    let commit = info.object().map_err(unknown)?;
    let author = commit.author().map_err(unknown)?;
    Ok(Commit {
        id: info.id.to_string(),
        parent_ids: info.parent_ids.iter().map(|id| id.to_string()).collect(),
        author: author.name.to_string(),
        message: commit.message().map_err(unknown)?.summary().to_string(),
        timestamp: commit.time().map_err(unknown)?.seconds,
    })
}

/// A [`LogCursor`] whose walk lives on its own thread, which owns the repository handle and
/// the walk (the walk borrows the handle, so they cannot be kept across calls otherwise).
/// A page is asked for over `asks` and comes back over `pages`; dropping the cursor ends the
/// thread at its next ask.
struct WalkCursor {
    asks: std::sync::mpsc::Sender<usize>,
    pages: std::sync::mpsc::Receiver<Result<Vec<Commit>, DomainError>>,
    ended: bool,
}

impl LogCursor for WalkCursor {
    fn next_page(&mut self, count: usize) -> Result<Vec<Commit>, DomainError> {
        if self.ended || count == 0 {
            return Ok(vec![]);
        }
        if self.asks.send(count).is_err() {
            // The walk thread is gone: it may have left why.
            self.ended = true;
            return match self.pages.try_recv() {
                Ok(Err(error)) => Err(error),
                _ => Ok(vec![]),
            };
        }
        let page = self
            .pages
            .recv()
            .unwrap_or_else(|_| Err(unknown("the history walk stopped unexpectedly")));
        if page.as_ref().map_or(true, |page| page.len() < count) {
            self.ended = true;
        }
        page
    }
}

/// The walk thread: answers each ask with the next `count` commits not in `already_loaded`,
/// and ends after an error, a short page, or the cursor going away.
fn walk_pages(
    path: &str,
    already_loaded: Vec<String>,
    asks: &std::sync::mpsc::Receiver<usize>,
    pages: &std::sync::mpsc::SyncSender<Result<Vec<Commit>, DomainError>>,
) {
    let loaded: std::collections::HashSet<gix::ObjectId> = already_loaded
        .iter()
        .filter_map(|id| gix::ObjectId::from_hex(id.as_bytes()).ok())
        .collect();
    let repo = match gix::open(path) {
        Ok(repo) => repo,
        Err(error) => {
            let _ = pages.send(Err(unknown(error)));
            return;
        }
    };
    let tips = match local_history_tips(&repo) {
        Ok(tips) => tips,
        Err(error) => {
            let _ = pages.send(Err(error));
            return;
        }
    };
    if tips.is_empty() {
        if asks.recv().is_ok() {
            let _ = pages.send(Ok(vec![]));
        }
        return;
    }
    let mut walk = match repo
        .rev_walk(tips)
        .sorting(gix::traverse::commit::simple::Sorting::ByCommitTimeNewestFirst)
        .all()
    {
        Ok(walk) => walk,
        Err(error) => {
            let _ = pages.send(Err(unknown(error)));
            return;
        }
    };
    while let Ok(count) = asks.recv() {
        let mut page = Vec::with_capacity(count);
        let mut outcome = Ok(());
        while page.len() < count {
            match walk.next() {
                None => break,
                Some(Err(error)) => {
                    outcome = Err(unknown(error));
                    break;
                }
                Some(Ok(info)) => {
                    if loaded.contains(&info.id) {
                        continue;
                    }
                    match commit_from_info(info) {
                        Ok(commit) => page.push(commit),
                        Err(error) => {
                            outcome = Err(error);
                            break;
                        }
                    }
                }
            }
        }
        let stop = outcome.is_err() || page.len() < count;
        if pages.send(outcome.map(|()| page)).is_err() || stop {
            return;
        }
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
        let head_commit_id = if head_is_unborn {
            None
        } else {
            repo.head()
                .ok()
                .and_then(|head| head.id().map(|id| id.to_string()))
        };
        let (staged, conflicted_files) = staged_changes(&repo, head_is_unborn)?;
        let mut status = RepoStatus {
            operation: operation_of(repo.state()),
            conflicted_files,
            staged_files: staged.keys().cloned().collect(),
            unstaged_files: vec![],
            untracked_files: vec![],
            tracking: tracking_status(&self.repo_path)?,
            head_commit_id,
            kinds: core_domain::ChangeKinds {
                staged,
                ..Default::default()
            },
        };

        // Index ↔ worktree: modified/removed tracked files and untracked files, one path each.
        if head_is_unborn {
            for (summary, path) in unborn_worktree_changes(&repo)? {
                record_change(&mut status, summary, path);
            }
        } else {
            let changes = repo
                .status(gix::progress::Discard)
                .map_err(unknown_with_causes)?
                .untracked_files(gix::status::UntrackedFiles::Files)
                .into_index_worktree_iter(Vec::<BString>::new())
                .map_err(unknown_with_causes)?;
            for change in changes {
                let change = change.map_err(unknown_with_causes)?;
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

    fn log_cursor(&self, already_loaded: Vec<String>) -> Result<Box<dyn LogCursor>, DomainError> {
        // A repository that cannot be opened fails here, on the caller's thread.
        self.open_repo()?;
        let path = self.repo_path.clone();
        let (asks, ask_receiver) = std::sync::mpsc::channel::<usize>();
        let (page_sender, pages) = std::sync::mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("history-walk".to_string())
            .spawn(move || walk_pages(&path, already_loaded, &ask_receiver, &page_sender))
            .map_err(unknown)?;
        Ok(Box::new(WalkCursor { asks, pages, ended: false }))
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

    fn merge_branch(&self, name: &str, no_ff: bool) -> Result<(), DomainError> {
        self.open_repo()?;
        require_branch_name(name)?;
        let mut args = vec!["merge"];
        if no_ff {
            args.push("--no-ff");
        }
        // No editor: git's prepared "Merge branch …" message is kept.
        args.extend(["--no-edit", "--", name]);
        run_git(&self.repo_path, &args)
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

    fn untrack_paths(&self, paths: &[String]) -> Result<(), DomainError> {
        self.open_repo()?;
        for path in paths {
            require_safe_path(path)?;
        }
        // No --force: git keeps a staged version that differs from both HEAD and the disk.
        run_git_paths(&self.repo_path, &["rm", "--cached", "-r", "--quiet"], paths)
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
            // -m --first-parent: a merge (and a stash, which is a merge commit) diffs against
            // its first parent. Without it `git show` prints a combined diff, usually empty.
            &["show", "--format=", "--unified=3", "-m", "--first-parent", commit_id],
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
        let text = run_git_stdout(&self.repo_path, &stash::LIST_ARGS)?;
        stash::parse_stash_list(&text)
    }

    fn unpushed_commits(&self) -> Result<Vec<String>, DomainError> {
        self.open_repo()?;
        if run_git_stdout(&self.repo_path, &["remote"])?.is_empty() {
            return Ok(vec![]);
        }
        // An unborn HEAD has no commits to be unpushed.
        let head = command::git_output(&self.repo_path, &["rev-parse", "--verify", "--quiet", "HEAD"])?;
        if !head.status.success() {
            return Ok(vec![]);
        }
        let text = run_git_stdout(
            &self.repo_path,
            &["rev-list", "--max-count=1000", "HEAD", "--not", "--remotes"],
        )?;
        Ok(text.lines().map(str::to_string).collect())
    }

    fn stash_push(&self, message: &str, include_untracked: bool) -> Result<(), DomainError> {
        self.stash(message, &[], include_untracked)
    }

    fn stash_push_paths(&self, message: &str, paths: &[String]) -> Result<(), DomainError> {
        self.stash(message, paths, false)
    }

    fn abort_operation(&self, operation: RepoOperation) -> Result<(), DomainError> {
        self.open_repo()?;
        run_git(&self.repo_path, &[operation_command(operation), "--abort"])
    }

    fn continue_operation(&self, operation: RepoOperation) -> Result<(), DomainError> {
        self.open_repo()?;
        // `true` as the editor keeps git's prepared message without opening anything.
        let quiet = ["-c", "core.editor=true", "-c", "sequence.editor=true"];
        match operation {
            RepoOperation::Merge => run_git(&self.repo_path, &[quiet[0], quiet[1], "commit", "--no-edit"]),
            other => run_git(
                &self.repo_path,
                &[quiet[0], quiet[1], quiet[2], quiet[3], operation_command(other), "--continue"],
            ),
        }
    }

    fn resolve_conflict(&self, path: &str, side: ConflictSide) -> Result<(), DomainError> {
        self.open_repo()?;
        let flag = match side {
            ConflictSide::Yours => "--ours",
            ConflictSide::Theirs => "--theirs",
        };
        let paths = [path.to_string()];
        run_git_paths(&self.repo_path, &["checkout", flag], &paths)?;
        run_git_paths(&self.repo_path, &["add"], &paths)
    }

    fn merge_tool(&self) -> Result<Option<String>, DomainError> {
        self.open_repo()?;
        // `git config` exits 1 when a key is unset: that is an answer, not a failure.
        let read = |args: &[&str]| -> Result<String, DomainError> {
            let output = command::git_output(&self.repo_path, args)?;
            match output.status.code() {
                Some(0) | Some(1) => Ok(String::from_utf8_lossy(&output.stdout).into_owned()),
                _ => Err(unknown(String::from_utf8_lossy(&output.stderr).trim())),
            }
        };
        let chosen = read(&["config", "--get", "merge.tool"])?;
        let defined = read(&["config", "--name-only", "--get-regexp", r"^mergetool\..+\.cmd$"])?;
        Ok(pick_merge_tool(chosen.trim(), &defined, bundled_resolver().is_some()))
    }

    fn merge_preview(&self, name: &str) -> Result<MergePreview, DomainError> {
        self.open_repo()?;
        require_branch_name(name)?;
        let lines = |text: &str| -> Vec<String> {
            text.lines().map(str::trim).filter(|line| !line.is_empty()).map(str::to_string).collect()
        };
        let count = command::run_git_stdout(&self.repo_path, &["rev-list", "--count", &format!("HEAD..{name}")])?;
        let commits = count.trim().parse::<usize>().map_err(unknown)?;
        if commits == 0 {
            return Ok(MergePreview::default());
        }
        // Exit 1 is "not an ancestor": an answer, not a failure.
        let ancestor = command::git_output(&self.repo_path, &["merge-base", "--is-ancestor", "HEAD", name])?;
        let fast_forward = ancestor.status.code() == Some(0);
        let files = lines(&command::run_git_stdout(
            &self.repo_path,
            &["diff", "--name-only", &format!("HEAD...{name}")],
        )?);
        // `merge-tree --write-tree` merges in memory: it writes objects, never the index, the
        // working tree or a ref. Exit 0 is a clean merge, 1 a merge with conflicts.
        let merged = command::git_output(
            &self.repo_path,
            &["merge-tree", "--write-tree", "--name-only", "--no-messages", "HEAD", name],
        )?;
        let conflicts = match merged.status.code() {
            Some(0) => Vec::new(),
            // The first line is the merged tree; the conflicted paths follow.
            Some(1) => lines(&String::from_utf8_lossy(&merged.stdout)).into_iter().skip(1).collect(),
            _ => return Err(unknown(String::from_utf8_lossy(&merged.stderr).trim())),
        };
        Ok(MergePreview { commits, fast_forward, files, conflicts })
    }

    fn run_merge_tool(&self, path: &str, tool: &str) -> Result<(), DomainError> {
        self.open_repo()?;
        if tool.is_empty() || tool.starts_with('-') || tool.contains(char::is_whitespace) {
            return Err(unknown(format!("not a merge tool name: {tool:?}")));
        }
        if tool == BUILTIN_MERGE_TOOL {
            let resolver = bundled_resolver()
                .ok_or_else(|| unknown("Cthulhu Resolve is not built or installed beside the app"))?;
            // git's shell expands $BASE and the rest; the path is quoted for it.
            let command = format!(
                r#"mergetool.{tool}.cmd="{}" merge --base "$BASE" --local "$LOCAL" --remote "$REMOTE" --merged "$MERGED""#,
                resolver.to_string_lossy().replace('\\', "/"),
            );
            let trust = format!("mergetool.{tool}.trustExitCode=true");
            // The resolver keeps its own backup before the first save.
            return run_git_paths(
                &self.repo_path,
                &["-c", &command, "-c", &trust, "-c", "mergetool.keepBackup=false", "mergetool", "--no-prompt", "--tool", tool],
                &[path.to_string()],
            );
        }
        run_git_paths(
            &self.repo_path,
            &["mergetool", "--no-prompt", "--tool", tool],
            &[path.to_string()],
        )
    }

    fn compare_file(&self, path: &str, left: &str, right: Option<&str>) -> Result<Patch, DomainError> {
        self.open_repo()?;
        require_safe_path(path)?;
        let left_bytes = self.version_bytes(path, Some(left))?;
        let right_bytes = self.version_bytes(path, right)?;
        let scratch = CompareScratch::new()?;
        let left_file = scratch.write("left", &left_bytes)?;
        let right_file = scratch.write("right", &right_bytes)?;
        let output = command::git_output(
            &self.repo_path,
            &[
                "diff",
                "--no-index",
                "--no-color",
                "--no-ext-diff",
                "--unified=3",
                "--",
                &left_file,
                &right_file,
            ],
        )?;
        // 0: same, 1: different. Anything else is git refusing.
        if !matches!(output.status.code(), Some(0) | Some(1)) {
            return Err(DomainError::Unknown(
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ));
        }
        let text = String::from_utf8_lossy(&output.stdout);
        let mut patch = parse_patches(&text).into_iter().next().unwrap_or(Patch {
            path: path.to_string(),
            hunks: Vec::new(),
            binary: false,
        });
        patch.path = path.to_string();
        Ok(patch)
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

/// git's last lines kept to explain a failed clone.
const CLONE_ERROR_LINES: usize = 12;

/// Remove the folder a clone that did not finish left behind. git may still be releasing its
/// files for a moment after it is stopped, so a few tries are made. `None` when it is gone.
fn remove_partial_clone(destination: &std::path::Path) -> Option<std::io::Error> {
    let mut last = None;
    for attempt in 0..10 {
        if !destination.exists() {
            return None;
        }
        match std::fs::remove_dir_all(destination) {
            Ok(()) => return None,
            Err(error) => last = Some(error),
        }
        std::thread::sleep(std::time::Duration::from_millis(100 * (attempt + 1)));
    }
    last
}

/// A folder name to clone into: one plain name, never a path or something git reads as an option.
fn validate_folder_name(name: &str) -> Result<(), DomainError> {
    let bad = name.trim().is_empty()
        || name != name.trim()
        || name == "."
        || name == ".."
        || name.starts_with('-')
        || name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|', '\0', '\n', '\r']);
    if bad {
        Err(DomainError::Unknown(format!("not a folder name: {name:?}")))
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

impl GitoxideAdapter {
    /// One side of a compare in working-tree form: the file on disk when `revision` is `None`,
    /// else `<revision>:<path>` through the checkout filters (so CRLF matches the disk).
    /// A revision that exists but lacks the file, or a missing file on disk, reads as empty.
    fn version_bytes(&self, path: &str, revision: Option<&str>) -> Result<Vec<u8>, DomainError> {
        let Some(revision) = revision else {
            let full = std::path::Path::new(&self.repo_path).join(path);
            return match std::fs::read(&full) {
                Ok(bytes) => Ok(bytes),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
                Err(error) => Err(DomainError::Unknown(format!("could not read {path}: {error}"))),
            };
        };
        require_compare_revision(revision)?;
        // `HEAD:a.txt`, or `:2:a.txt` for an index stage.
        let spec = format!("{revision}:{path}");
        let output = command::git_output(&self.repo_path, &["cat-file", "--filters", &spec])?;
        if output.status.success() {
            return Ok(output.stdout);
        }
        // Tell "no such file at that revision" (empty side) from "no such revision" (error).
        let known = revision.starts_with(':')
            || command::git_output(
                &self.repo_path,
                &["rev-parse", "--verify", "--quiet", &format!("{revision}^{{commit}}")],
            )?
            .status
            .success();
        if known {
            Ok(Vec::new())
        } else {
            Err(DomainError::Unknown(format!("unknown revision: {revision}")))
        }
    }
}

/// A revision to compare against: a ref, a commit, or an index stage (`:1`, `:2`, `:3`).
/// Never option-like, never a range, never with whitespace or NUL.
fn require_compare_revision(revision: &str) -> Result<(), DomainError> {
    let stage = matches!(revision, ":1" | ":2" | ":3");
    let bad = revision.is_empty()
        || revision.starts_with('-')
        || revision.contains(['\0', '\n', '\r', ' ', '\t'])
        || revision.contains("..")
        || (revision.contains(':') && !stage);
    if bad {
        Err(DomainError::Unknown(format!("invalid revision: {revision:?}")))
    } else {
        Ok(())
    }
}

/// Two temporary files for `git diff --no-index`, removed when dropped.
struct CompareScratch {
    folder: std::path::PathBuf,
}

impl CompareScratch {
    fn new() -> Result<Self, DomainError> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let folder = std::env::temp_dir().join(format!(
            "gitcthulhu-compare-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&folder).map_err(unknown)?;
        Ok(Self { folder })
    }

    fn write(&self, name: &str, bytes: &[u8]) -> Result<String, DomainError> {
        let file = self.folder.join(name);
        std::fs::write(&file, bytes).map_err(unknown)?;
        Ok(file.to_string_lossy().into_owned())
    }
}

impl Drop for CompareScratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.folder);
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

/// The git subcommand that owns `operation`'s `--abort` and `--continue`.
fn operation_command(operation: RepoOperation) -> &'static str {
    match operation {
        RepoOperation::Merge => "merge",
        RepoOperation::Rebase => "rebase",
        RepoOperation::CherryPick => "cherry-pick",
        RepoOperation::Revert => "revert",
    }
}

/// The operation `state` records as in progress, in the port's terms.
fn operation_of(state: Option<gix::state::InProgress>) -> Option<RepoOperation> {
    use gix::state::InProgress;
    match state? {
        InProgress::Merge => Some(RepoOperation::Merge),
        InProgress::Rebase | InProgress::RebaseInteractive | InProgress::ApplyMailboxRebase => {
            Some(RepoOperation::Rebase)
        }
        InProgress::CherryPick | InProgress::CherryPickSequence => Some(RepoOperation::CherryPick),
        InProgress::Revert | InProgress::RevertSequence => Some(RepoOperation::Revert),
        InProgress::ApplyMailbox | InProgress::Bisect => None,
    }
}

impl GitoxideAdapter {
    /// `git stash push [-u] [-m <message>] [-- <paths>…]`.
    fn stash(&self, message: &str, paths: &[String], include_untracked: bool) -> Result<(), DomainError> {
        self.open_repo()?;
        if message.contains('\0') {
            return Err(DomainError::Unknown(
                "stash message contains a null byte".to_string(),
            ));
        }
        let message = message.trim();
        let mut args: Vec<&str> = vec!["stash", "push"];
        if include_untracked {
            args.push("--include-untracked");
        }
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
}
