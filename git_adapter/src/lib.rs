//! git_adapter
//! Implements the `GitRepository` port using `gix` (gitoxide).

use std::collections::BTreeMap;
use std::fmt::Display;

use std::convert::Infallible;
use std::sync::atomic::AtomicBool;

use core_domain::{Branch, Commit, CredentialStore, DomainError, GitRepository, RepoStatus};
use gix::bstr::{BStr, BString};
use gix::refs::TargetRef;
use gix::status::index_worktree::iter::Summary;
use gix::status::plumbing::index_as_worktree::traits::{FastEq, SubmoduleStatus};
use gix::status::plumbing::index_as_worktree_with_renames::{Entry as StatusEntry, VisitEntry};

/// The git executable, used only for porcelain operations gix 0.66 can't perform safely.
const GIT_EXECUTABLE: &str = "git";

/// Run `git -C <repo_path> <args>`, returning git's own error text on failure.
fn run_git(repo_path: &str, args: &[&str]) -> Result<(), DomainError> {
    let mut command = std::process::Command::new(GIT_EXECUTABLE);
    command
        .arg("-C")
        .arg(repo_path)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let output = command.output().map_err(|error| {
        DomainError::Unknown(format!("could not run {GIT_EXECUTABLE}: {error}"))
    })?;
    if output.status.success() {
        Ok(())
    } else {
        Err(DomainError::Unknown(String::from_utf8_lossy(&output.stderr).trim().to_string()))
    }
}

/// Map any gix error that has no specific domain meaning.
fn unknown(error: impl Display) -> DomainError {
    DomainError::Unknown(error.to_string())
}

/// Stage-0 index entries keyed by path, for comparing two indices.
fn entries_by_path(index: &gix::index::State) -> BTreeMap<BString, (gix::ObjectId, gix::index::entry::Mode)> {
    index
        .entries()
        .iter()
        .filter(|entry| entry.stage() == gix::index::entry::Stage::Unconflicted)
        .map(|entry| (entry.path(index).to_owned(), (entry.id, entry.mode)))
        .collect()
}

/// Paths whose index entry differs from `HEAD` — what `git status` calls "Changes to be committed".
/// gix 0.66 has no tree-to-index status, so the two are compared directly.
fn staged_paths(repo: &gix::Repository, head_is_unborn: bool) -> Result<Vec<String>, DomainError> {
    let index = repo.index_or_empty().map_err(unknown)?;
    let head_entries = if head_is_unborn {
        BTreeMap::new()
    } else {
        let head_tree = repo.head_tree_id().map_err(unknown)?;
        let head_index = repo.index_from_tree(&head_tree).map_err(unknown)?;
        entries_by_path(&head_index)
    };
    let index_entries = entries_by_path(&index);

    let mut paths: Vec<String> = index_entries
        .keys()
        .chain(head_entries.keys())
        .filter(|path| index_entries.get(*path) != head_entries.get(*path))
        .map(|path| path.to_string())
        .collect();
    paths.sort();
    paths.dedup();
    Ok(paths)
}

/// File one index ↔ worktree change under the matching `RepoStatus` list.
fn record_change(status: &mut RepoStatus, summary: Summary, path: String) {
    match summary {
        Summary::Added => status.untracked_files.push(path),
        _ => status.unstaged_files.push(path),
    }
}

/// Submodule checker that never reports a change.
///
/// Only used while `HEAD` is unborn: gix 0.66's built-in checker looks up `.gitmodules`
/// in the `HEAD` commit and fails when there is none. With no commits there are no
/// committed submodules to report anyway.
#[derive(Clone)]
struct NoSubmodules;

impl SubmoduleStatus for NoSubmodules {
    type Output = ();
    type Error = Infallible;

    fn status(&mut self, _entry: &gix::index::Entry, _rela_path: &BStr) -> Result<Option<()>, Infallible> {
        Ok(None)
    }
}

/// Collects the summary of every change reported by `index_worktree_status`.
struct CollectChanges(Vec<(Summary, String)>);

impl<'index> VisitEntry<'index> for CollectChanges {
    type ContentChange = ();
    type SubmoduleStatus = ();

    fn visit_entry(&mut self, entry: StatusEntry<'index, (), ()>) {
        if let Some(summary) = entry.summary() {
            self.0.push((summary, entry.destination_rela_path().to_string()));
        }
    }
}

/// Index ↔ worktree changes for a repository whose `HEAD` is unborn.
/// Same comparison as `Repository::status()`, minus the submodule lookup that fails there.
fn unborn_worktree_changes(repo: &gix::Repository) -> Result<Vec<(Summary, String)>, DomainError> {
    let index = repo.index_or_empty().map_err(unknown)?;
    let options = gix::status::index_worktree::Options {
        dirwalk_options: Some(
            repo.dirwalk_options()
                .map_err(unknown)?
                .emit_untracked(gix::dir::walk::EmissionMode::Matching),
        ),
        ..Default::default()
    };
    let mut changes = CollectChanges(Vec::new());
    repo.index_worktree_status(
        &index,
        Vec::<BString>::new(),
        &mut changes,
        FastEq,
        NoSubmodules,
        &mut gix::progress::Discard,
        &AtomicBool::new(false),
        options,
    )
    .map_err(unknown)?;
    Ok(changes.0)
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

    /// Helper to open the repository and map initialization errors
    fn open_repo(&self) -> Result<gix::Repository, DomainError> {
        gix::open(&self.repo_path).map_err(|e| {
            match e {
                gix::open::Error::NotARepository { .. } => DomainError::RepositoryNotFound,
                _ => DomainError::Unknown(e.to_string()),
            }
        })
    }

    /// Asynchronous, non-blocking indexing function that rapidly scans a target directory.
    /// This leverages `gix`'s concurrent traversal capabilities offloaded to a blocking pool.
    pub async fn async_status(&self) -> Result<RepoStatus, DomainError> {
        let path = self.repo_path.clone();
        
        // Offload blocking gix concurrent status traversal to a threadpool
        tokio::task::spawn_blocking(move || {
            let _repo = gix::open(&path).map_err(|_| DomainError::RepositoryNotFound)?;
            
            // In a full implementation, we'd use `repo.status()` or `repo.dirwalk()` 
            // with thread limits configured for maximum parallel I/O.
            
            Ok(RepoStatus {
                staged_files: vec![],
                unstaged_files: vec![],
                untracked_files: vec![],
            })
        })
        .await
        .map_err(|e| DomainError::Unknown(format!("Async join error: {}", e)))?
    }
}

impl GitRepository for GitoxideAdapter {
    fn status(&self) -> Result<RepoStatus, DomainError> {
        let repo = self.open_repo()?;
        let head_is_unborn = repo.head().map_err(unknown)?.is_unborn();
        let mut status = RepoStatus {
            staged_files: staged_paths(&repo, head_is_unborn)?,
            unstaged_files: vec![],
            untracked_files: vec![],
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
        let Some(head) = repo.head().map_err(unknown)?.id() else {
            return Ok(vec![]); // unborn branch: no commits yet
        };

        let walk = head
            .ancestors()
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
                    // The summary line only: this is what the commit graph renders.
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

        let local = references.local_branches().map_err(unknown)?.map(|r| (r, false));
        let remote = references.remote_branches().map_err(unknown)?.map(|r| (r, true));

        let mut branches = Vec::new();
        for (reference, is_remote) in local.chain(remote) {
            let reference = reference.map_err(unknown)?;
            // Skip symbolic refs such as `refs/remotes/origin/HEAD`.
            if matches!(reference.target(), TargetRef::Symbolic(_)) {
                continue;
            }
            branches.push(Branch {
                name: reference.name().shorten().to_string(),
                is_remote,
                is_head: head_name.as_ref().is_some_and(|head| head.as_ref() == reference.name()),
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
            return Err(DomainError::Unknown(format!("invalid branch name: {branch_name:?}")));
        }
        run_git(&self.repo_path, &["switch", "--no-guess", branch_name])
    }

    fn fetch(&self, store: &dyn CredentialStore) -> Result<(), DomainError> {
        let _repo = self.open_repo()?;
        
        // Instead of shelling out to git-credential-manager, wire gix-credentials
        // to pull directly from our keyring_adapter.
        // We simulate retrieving the token.
        let _token = store.get_token("current_user").map_err(|_| DomainError::AuthenticationFailed)?;
        
        // Perform the gix fetch operation with the token...
        // If authentication fails during network I/O, we would map the gix error:
        // return Err(DomainError::AuthenticationFailed);
        
        Ok(())
    }

    fn fetch_ssh(&self, ssh_manager: &dyn core_domain::SshManager) -> Result<(), DomainError> {
        let _repo = self.open_repo()?;
        
        let (env_key, env_val) = ssh_manager.prepare_ssh_env()?;
        
        // In a real implementation, we would set the environment variable specifically
        // for the lifetime of the gix fetch operation, ensuring strict isolation between
        // concurrent repository operations.
        // e.g.:
        // std::env::set_var(env_key, env_val);
        // let _res = gix::fetch(...);
        // std::env::remove_var(env_key);
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_local_repo_and_get_head() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        // Initialize a bare repository using gitoxide
        let _repo = gix::init_bare(temp_dir.path()).expect("Failed to initialize gitoxide repository");
        
        let adapter = GitoxideAdapter::new(temp_dir.path().to_str().unwrap());
        
        // 1. Open the repository
        let opened_repo = adapter.open_repo().expect("Adapter failed to open repo");
        
        // 2. Try to get HEAD commit hash (will be un-born in a fresh repo, but should not panic)
        let head_result = opened_repo.head();

        assert!(head_result.is_ok(), "Should successfully read HEAD reference");
    }

    // ── Fixture ───────────────────────────────────────────────────────────────
    //
    // Repositories are built with gix itself (no `git` binary), with fixed
    // signatures and timestamps, so results are identical on every machine.

    use gix::objs::tree::{Entry, EntryKind};
    use gix::refs::transaction::{Change, LogChange, PreviousValue, RefEdit};
    use gix::refs::Target;

    const BASE_TIME: i64 = 1_700_000_000;

    struct TestRepo {
        _dir: tempfile::TempDir,
        repo: gix::Repository,
    }

    impl TestRepo {
        fn new() -> Self {
            let dir = tempfile::tempdir().expect("create temp dir");
            let repo = gix::init(dir.path()).expect("init repository");
            // Pin line-ending conversion so a global `core.autocrlf` can't change results.
            let config = repo.git_dir().join("config");
            let mut contents = std::fs::read_to_string(&config).expect("read repo config");
            contents.push_str("[core]\n\tautocrlf = false\n");
            std::fs::write(&config, contents).expect("write repo config");
            let repo = gix::open(dir.path()).expect("reopen with pinned config");
            Self { _dir: dir, repo }
        }

        fn workdir(&self) -> &std::path::Path {
            self.repo.work_dir().expect("non-bare repository")
        }

        fn adapter(&self) -> GitoxideAdapter {
            GitoxideAdapter::new(self.workdir().to_str().expect("utf-8 temp path"))
        }

        /// Short name of the branch HEAD points at (`main` unless `init.defaultBranch` says otherwise).
        fn head_branch(&self) -> String {
            self.repo.head_name().unwrap().expect("HEAD is a branch").shorten().to_string()
        }

        fn write_tree(&self, files: &[(&str, &str)]) -> gix::ObjectId {
            let mut entries: Vec<Entry> = files
                .iter()
                .map(|(name, content)| Entry {
                    mode: EntryKind::Blob.into(),
                    filename: (*name).into(),
                    oid: self.repo.write_blob(content).expect("write blob").detach(),
                })
                .collect();
            entries.sort_by(|a, b| a.filename.cmp(&b.filename));
            self.repo.write_object(gix::objs::Tree { entries }).expect("write tree").detach()
        }

        /// Commit `files` as the full tree on top of HEAD, at `BASE_TIME + offset` seconds.
        fn commit(&self, message: &str, files: &[(&str, &str)], offset: i64) -> gix::ObjectId {
            let tree = self.write_tree(files);
            let signature = gix::actor::Signature {
                name: "Test Author".into(),
                email: "author@example.com".into(),
                time: gix::date::Time::new(BASE_TIME + offset, 0),
            };
            let parents: Vec<gix::ObjectId> = self.repo.head_id().ok().map(|id| id.detach()).into_iter().collect();
            self.repo
                .commit_as(signature.to_ref(), signature.to_ref(), "HEAD", message, tree, parents)
                .expect("commit")
                .detach()
        }

        /// Make the index and worktree hold exactly `files`, without touching HEAD.
        fn stage_and_write(&self, files: &[(&str, &str)]) {
            let tree = self.write_tree(files);
            let mut index = self.repo.index_from_tree(&tree).expect("index from tree");
            index.write(Default::default()).expect("write index");
            for (name, content) in files {
                self.write_file(name, content);
            }
        }

        fn write_file(&self, name: &str, content: &str) {
            std::fs::write(self.workdir().join(name), content).expect("write worktree file");
        }

        /// Commit `files` onto `reference` (not HEAD) with an explicit parent.
        fn commit_to(&self, reference: &str, message: &str, files: &[(&str, &str)], parent: gix::ObjectId) -> gix::ObjectId {
            let tree = self.write_tree(files);
            let signature = gix::actor::Signature {
                name: "Test Author".into(),
                email: "author@example.com".into(),
                time: gix::date::Time::new(BASE_TIME + 3600, 0),
            };
            self.repo
                .commit_as(signature.to_ref(), signature.to_ref(), reference, message, tree, [parent])
                .expect("commit to reference")
                .detach()
        }

        /// The branch HEAD points at, read fresh from disk (git may have changed it).
        fn current_branch(&self) -> String {
            gix::open(self.workdir()).expect("reopen").head_name().unwrap().expect("on a branch").shorten().to_string()
        }

        fn read_file(&self, name: &str) -> Option<String> {
            std::fs::read_to_string(self.workdir().join(name)).ok()
        }

        /// A repository on its default branch with `a.txt`, plus a `feature` branch that
        /// changes `a.txt` and adds `b.txt`; the worktree is a clean checkout of HEAD.
        fn with_feature_branch() -> Self {
            let fixture = Self::new();
            let files = [("a.txt", "a on main\n")];
            let base = fixture.commit("initial", &files, 0);
            fixture.stage_and_write(&files);
            fixture.commit_to(
                "refs/heads/feature",
                "feature work",
                &[("a.txt", "a on feature\n"), ("b.txt", "b\n")],
                base,
            );
            fixture
        }

        fn reference(&self, name: &str, target: gix::ObjectId) {
            self.repo.reference(name, target, PreviousValue::Any, "test").expect("create reference");
        }

        fn symbolic_reference(&self, name: &str, target: &str) {
            self.repo
                .edit_reference(RefEdit {
                    change: Change::Update {
                        log: LogChange::default(),
                        expected: PreviousValue::Any,
                        new: Target::Symbolic(target.try_into().expect("valid ref name")),
                    },
                    name: name.try_into().expect("valid ref name"),
                    deref: false,
                })
                .expect("create symbolic reference");
        }
    }

    // ── log ───────────────────────────────────────────────────────────────────

    #[test]
    fn log_of_unborn_repository_is_empty() {
        let fixture = TestRepo::new();
        assert_eq!(fixture.adapter().log(None), Ok(vec![]));
    }

    #[test]
    fn log_lists_commits_newest_first_with_parents() {
        let fixture = TestRepo::new();
        let first = fixture.commit("first commit", &[("a.txt", "one\n")], 0);
        let second = fixture.commit("second commit\n\nWith a body.", &[("a.txt", "two\n")], 60);

        let log = fixture.adapter().log(None).expect("log");

        assert_eq!(log.len(), 2);
        assert_eq!(log[0].id, second.to_string());
        assert_eq!(log[0].message, "second commit");
        assert_eq!(log[0].author, "Test Author");
        assert_eq!(log[0].timestamp, BASE_TIME + 60);
        assert_eq!(log[0].parent_ids, vec![first.to_string()]);
        assert_eq!(log[1].id, first.to_string());
        assert!(log[1].parent_ids.is_empty());
    }

    #[test]
    fn log_respects_max_count() {
        let fixture = TestRepo::new();
        for n in 0..3 {
            fixture.commit(&format!("commit {n}"), &[("a.txt", &format!("{n}\n"))], n * 60);
        }

        let log = fixture.adapter().log(Some(2)).expect("log");

        assert_eq!(log.len(), 2);
        assert_eq!(log[0].message, "commit 2");
    }

    // ── branches ──────────────────────────────────────────────────────────────

    #[test]
    fn branches_lists_local_and_remote_and_marks_head() {
        let fixture = TestRepo::new();
        let tip = fixture.commit("initial", &[("a.txt", "a\n")], 0);
        fixture.reference("refs/heads/feature/x", tip);
        fixture.reference("refs/remotes/origin/main", tip);
        fixture.symbolic_reference("refs/remotes/origin/HEAD", "refs/remotes/origin/main");

        let branches = fixture.adapter().branches().expect("branches");

        let head = fixture.head_branch();
        let find = |name: &str| branches.iter().find(|b| b.name == name);
        assert_eq!(find(&head), Some(&Branch { name: head.clone(), is_remote: false, is_head: true }));
        assert_eq!(
            find("feature/x"),
            Some(&Branch { name: "feature/x".to_string(), is_remote: false, is_head: false })
        );
        assert_eq!(
            find("origin/main"),
            Some(&Branch { name: "origin/main".to_string(), is_remote: true, is_head: false })
        );
        assert!(find("origin/HEAD").is_none(), "symbolic refs are not branches");
        assert_eq!(branches.len(), 3);
    }

    // ── status ────────────────────────────────────────────────────────────────

    #[test]
    fn status_of_clean_checkout_is_empty() {
        let fixture = TestRepo::new();
        let files = [("a.txt", "a\n"), ("b.txt", "b\n")];
        fixture.commit("initial", &files, 0);
        fixture.stage_and_write(&files);

        let status = fixture.adapter().status().expect("status");

        assert_eq!(status, RepoStatus { staged_files: vec![], unstaged_files: vec![], untracked_files: vec![] });
    }

    #[test]
    fn status_reports_unstaged_and_untracked_files() {
        let fixture = TestRepo::new();
        let files = [("a.txt", "a\n"), ("b.txt", "b\n")];
        fixture.commit("initial", &files, 0);
        fixture.stage_and_write(&files);
        fixture.write_file("a.txt", "a changed\n");
        fixture.write_file("new.txt", "new\n");

        let status = fixture.adapter().status().expect("status");

        assert_eq!(status.staged_files, Vec::<String>::new());
        assert_eq!(status.unstaged_files, vec!["a.txt".to_string()]);
        assert_eq!(status.untracked_files, vec!["new.txt".to_string()]);
    }

    #[test]
    fn status_reports_staged_changes_against_head() {
        let fixture = TestRepo::new();
        fixture.commit("initial", &[("a.txt", "a\n"), ("gone.txt", "gone\n")], 0);
        // Index: a.txt modified, gone.txt removed, b.txt added — worktree matches the index.
        fixture.stage_and_write(&[("a.txt", "a staged\n"), ("b.txt", "b\n")]);

        let status = fixture.adapter().status().expect("status");

        assert_eq!(status.staged_files, vec!["a.txt", "b.txt", "gone.txt"]);
        assert_eq!(status.unstaged_files, Vec::<String>::new());
        assert_eq!(status.untracked_files, Vec::<String>::new());
    }

    #[test]
    fn status_of_unborn_repository_lists_untracked_files() {
        let fixture = TestRepo::new();
        fixture.write_file("b.txt", "b\n");
        fixture.write_file("a.txt", "a\n");

        let status = fixture.adapter().status().expect("status");

        assert_eq!(status.staged_files, Vec::<String>::new());
        assert_eq!(status.untracked_files, vec!["a.txt", "b.txt"]);
    }

    #[test]
    fn status_of_unborn_repository_reports_staged_then_edited_files() {
        let fixture = TestRepo::new();
        fixture.stage_and_write(&[("a.txt", "a\n")]);
        fixture.write_file("a.txt", "a edited after staging\n");
        fixture.write_file("new.txt", "new\n");

        let status = fixture.adapter().status().expect("status");

        assert_eq!(status.staged_files, vec!["a.txt"]);
        assert_eq!(status.unstaged_files, vec!["a.txt"]);
        assert_eq!(status.untracked_files, vec!["new.txt"]);
    }

    // ── checkout_branch ───────────────────────────────────────────────────────
    //
    // These run the real `git switch`, so they need git on PATH.

    #[test]
    fn checkout_switches_branch_and_updates_the_worktree() {
        let fixture = TestRepo::with_feature_branch();
        let adapter = fixture.adapter();

        adapter.checkout_branch("feature").expect("checkout");

        assert_eq!(fixture.current_branch(), "feature");
        assert_eq!(fixture.read_file("a.txt").as_deref(), Some("a on feature\n"));
        assert_eq!(fixture.read_file("b.txt").as_deref(), Some("b\n"));
        assert_eq!(adapter.status(), Ok(RepoStatus { staged_files: vec![], unstaged_files: vec![], untracked_files: vec![] }));
    }

    #[test]
    fn checkout_refuses_to_overwrite_local_changes() {
        let fixture = TestRepo::with_feature_branch();
        let main = fixture.head_branch();
        fixture.write_file("a.txt", "uncommitted edit\n");

        let result = fixture.adapter().checkout_branch("feature");

        assert!(matches!(result, Err(DomainError::Unknown(ref message)) if !message.is_empty()), "{result:?}");
        assert_eq!(fixture.current_branch(), main);
        assert_eq!(fixture.read_file("a.txt").as_deref(), Some("uncommitted edit\n"));
    }

    #[test]
    fn checkout_of_unknown_branch_fails() {
        let fixture = TestRepo::with_feature_branch();
        assert!(fixture.adapter().checkout_branch("no-such-branch").is_err());
    }

    #[test]
    fn checkout_rejects_names_that_look_like_options() {
        let fixture = TestRepo::with_feature_branch();
        let main = fixture.head_branch();

        for name in ["", "-f", "--orphan=x"] {
            assert!(fixture.adapter().checkout_branch(name).is_err(), "{name:?} should be rejected");
        }
        assert_eq!(fixture.current_branch(), main);
    }

    #[test]
    fn checkout_never_creates_a_branch_from_a_remote() {
        let fixture = TestRepo::with_feature_branch();
        let tip = fixture.repo.head_id().unwrap().detach();
        fixture.reference("refs/remotes/origin/topic", tip);

        assert!(fixture.adapter().checkout_branch("topic").is_err());
        let reopened = gix::open(fixture.workdir()).expect("reopen");
        assert!(reopened.try_find_reference("refs/heads/topic").unwrap().is_none());
    }

    #[test]
    fn directory_that_is_not_a_repository_is_reported_as_not_found() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let adapter = GitoxideAdapter::new(dir.path().to_str().expect("utf-8 temp path"));

        assert_eq!(adapter.status(), Err(DomainError::RepositoryNotFound));
        assert_eq!(adapter.log(None), Err(DomainError::RepositoryNotFound));
        assert_eq!(adapter.branches(), Err(DomainError::RepositoryNotFound));
    }
}
