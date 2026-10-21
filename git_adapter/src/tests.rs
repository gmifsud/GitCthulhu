use super::command::{BLANK_COMMIT_MESSAGE, INVALID_PATH, NO_PATHS};
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

    assert!(
        head_result.is_ok(),
        "Should successfully read HEAD reference"
    );
}

#[test]
fn discover_opens_a_repository() {
    let repo = TestRepo::new();
    assert!(GitoxideAdapter::discover(repo.workdir().to_str().expect("utf-8 temp path")).is_ok());
}

// ── Fixture ───────────────────────────────────────────────────────────────
//
// Repositories are built with gix itself (no `git` binary), with fixed
// signatures and timestamps, so results are identical on every machine.

use gix::objs::tree::{Entry, EntryKind};
use gix::refs::transaction::{Change, LogChange, PreviousValue, RefEdit, RefLog};
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
        // Pin line endings, identity, and signing so a global git config can't change results.
        let config = repo.git_dir().join("config");
        let mut contents = std::fs::read_to_string(&config).expect("read repo config");
        contents.push_str(
            "[core]\n\tautocrlf = false\n\
                 [user]\n\tname = Test Author\n\temail = author@example.com\n\
                 [commit]\n\tgpgsign = false\n",
        );
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
        self.repo
            .head_name()
            .unwrap()
            .expect("HEAD is a branch")
            .shorten()
            .to_string()
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
        self.repo
            .write_object(gix::objs::Tree { entries })
            .expect("write tree")
            .detach()
    }

    /// Commit `files` as the full tree on top of HEAD, at `BASE_TIME + offset` seconds.
    fn commit(&self, message: &str, files: &[(&str, &str)], offset: i64) -> gix::ObjectId {
        let tree = self.write_tree(files);
        let signature = gix::actor::Signature {
            name: "Test Author".into(),
            email: "author@example.com".into(),
            time: gix::date::Time::new(BASE_TIME + offset, 0),
        };
        let parents: Vec<gix::ObjectId> = self
            .repo
            .head_id()
            .ok()
            .map(|id| id.detach())
            .into_iter()
            .collect();
        self.repo
            .commit_as(
                signature.to_ref(),
                signature.to_ref(),
                "HEAD",
                message,
                tree,
                parents,
            )
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
    fn commit_to(
        &self,
        reference: &str,
        message: &str,
        files: &[(&str, &str)],
        parent: gix::ObjectId,
        offset: i64,
    ) -> gix::ObjectId {
        let tree = self.write_tree(files);
        let signature = gix::actor::Signature {
            name: "Test Author".into(),
            email: "author@example.com".into(),
            time: gix::date::Time::new(BASE_TIME + offset, 0),
        };
        self.repo
            .commit_as(
                signature.to_ref(),
                signature.to_ref(),
                reference,
                message,
                tree,
                [parent],
            )
            .expect("commit to reference")
            .detach()
    }

    /// The commit HEAD points at, read fresh from disk (git may have added one).
    fn head_commit(&self) -> gix::ObjectId {
        gix::open(self.workdir())
            .expect("reopen")
            .head_id()
            .expect("born HEAD")
            .detach()
    }

    /// The branch HEAD points at, read fresh from disk (git may have changed it).
    fn current_branch(&self) -> String {
        gix::open(self.workdir())
            .expect("reopen")
            .head_name()
            .unwrap()
            .expect("on a branch")
            .shorten()
            .to_string()
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
            3600,
        );
        fixture
    }

    fn reference(&self, name: &str, target: gix::ObjectId) {
        self.repo
            .reference(name, target, PreviousValue::Any, "test")
            .expect("create reference");
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

    fn delete_reference(&self, name: &str) {
        self.repo
            .edit_reference(RefEdit {
                change: Change::Delete {
                    expected: PreviousValue::Any,
                    log: RefLog::AndReference,
                },
                name: name.try_into().expect("valid ref name"),
                deref: false,
            })
            .expect("delete reference");
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
        fixture.commit(
            &format!("commit {n}"),
            &[("a.txt", &format!("{n}\n"))],
            n * 60,
        );
    }

    let log = fixture.adapter().log(Some(2)).expect("log");

    assert_eq!(log.len(), 2);
    assert_eq!(log[0].message, "commit 2");
}

#[test]
fn log_includes_every_local_branch_once() {
    let fixture = TestRepo::new();
    let base = fixture.commit("base", &[("a.txt", "a\n")], 0);
    fixture.commit_to(
        "refs/heads/feature",
        "feature work",
        &[("b.txt", "b\n")],
        base,
        60,
    );

    let log = fixture.adapter().log(None).expect("log");
    let messages: Vec<_> = log.iter().map(|commit| commit.message.as_str()).collect();

    assert_eq!(messages, vec!["feature work", "base"]);
}

#[test]
fn log_keeps_the_newest_commits_across_local_branches() {
    let fixture = TestRepo::new();
    let base = fixture.commit("base", &[("a.txt", "a\n")], 0);
    fixture.commit("main tip", &[("a.txt", "main\n")], 60);
    fixture.commit_to(
        "refs/heads/feature",
        "newest feature",
        &[("b.txt", "b\n")],
        base,
        120,
    );

    let log = fixture.adapter().log(Some(1)).expect("log");

    assert_eq!(log.len(), 1);
    assert_eq!(log[0].message, "newest feature");
}

#[test]
fn log_ignores_commits_that_are_only_on_a_remote() {
    let fixture = TestRepo::new();
    let base = fixture.commit("base", &[("a.txt", "a\n")], 0);
    fixture.commit_to(
        "refs/remotes/origin/topic",
        "remote only",
        &[("c.txt", "c\n")],
        base,
        120,
    );

    let log = fixture.adapter().log(None).expect("log");
    let messages: Vec<_> = log.iter().map(|commit| commit.message.as_str()).collect();

    assert_eq!(messages, vec!["base"]);
}

#[test]
fn log_includes_a_detached_head() {
    let fixture = TestRepo::new();
    let base = fixture.commit("base", &[("a.txt", "a\n")], 0);
    let loose = fixture.commit_to(
        "refs/heads/loose",
        "detached work",
        &[("a.txt", "d\n")],
        base,
        60,
    );
    fixture.reference("HEAD", loose);
    fixture.delete_reference("refs/heads/loose");

    let log = fixture.adapter().log(None).expect("log");
    let messages: Vec<_> = log.iter().map(|commit| commit.message.as_str()).collect();

    assert_eq!(messages, vec!["detached work", "base"]);
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
    assert_eq!(
        find(&head),
        Some(&Branch {
            name: head.clone(),
            is_remote: false,
            is_head: true
        })
    );
    assert_eq!(
        find("feature/x"),
        Some(&Branch {
            name: "feature/x".to_string(),
            is_remote: false,
            is_head: false
        })
    );
    assert_eq!(
        find("origin/main"),
        Some(&Branch {
            name: "origin/main".to_string(),
            is_remote: true,
            is_head: false
        })
    );
    assert!(
        find("origin/HEAD").is_none(),
        "symbolic refs are not branches"
    );
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

    assert_eq!(
        status,
        RepoStatus {
            staged_files: vec![],
            unstaged_files: vec![],
            untracked_files: vec![],
            tracking: None,
        }
    );
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

// ── fetch ─────────────────────────────────────────────────────────────────
//
// `git fetch` against a local repository, so the test stays off the network.

fn remote_tip(repo_path: &std::path::Path, branch: &str) -> gix::ObjectId {
    gix::open(repo_path)
        .expect("open clone")
        .find_reference(format!("refs/remotes/origin/{branch}").as_str())
        .expect("remote-tracking ref")
        .id()
        .detach()
}

#[test]
fn fetch_brings_new_commits_from_a_local_remote() {
    let origin = TestRepo::new();
    let first = origin.commit("first", &[("a.txt", "one\n")], 0);
    let branch = origin.head_branch();
    let dest = tempfile::tempdir().expect("clone dir");
    let cloned = std::process::Command::new("git")
        .args([
            "clone",
            origin.workdir().to_str().expect("utf-8 origin"),
            dest.path().to_str().expect("utf-8 clone"),
        ])
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(std::process::Stdio::null())
        .status()
        .expect("run git clone");
    assert!(cloned.success(), "git clone failed");
    assert_eq!(remote_tip(dest.path(), &branch), first);

    let second = origin.commit("second", &[("a.txt", "two\n")], 60);
    GitoxideAdapter::new(dest.path().to_str().expect("utf-8 clone"))
        .fetch()
        .expect("fetch");

    assert_eq!(remote_tip(dest.path(), &branch), second);
}

#[test]
fn fetch_of_a_missing_remote_reports_git_message() {
    let fixture = TestRepo::new();
    fixture.commit("first", &[("a.txt", "one\n")], 0);
    let missing = fixture.workdir().join("missing-remote");
    let added = std::process::Command::new("git")
        .args([
            "-C",
            fixture.workdir().to_str().expect("utf-8 workdir"),
            "remote",
            "add",
            "origin",
            missing.to_str().expect("utf-8 missing path"),
        ])
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(std::process::Stdio::null())
        .status()
        .expect("git remote add");
    assert!(added.success(), "git remote add failed");

    let error = fixture.adapter().fetch().expect_err("missing remote");

    match error {
        DomainError::Unknown(message) => {
            assert!(
                message.contains("does not appear to be a git repository"),
                "{message}"
            );
        }
        other => panic!("expected git's message, got {other:?}"),
    }
}

fn git_in(dir: &std::path::Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_AUTHOR_NAME", "Test Author")
        .env("GIT_AUTHOR_EMAIL", "author@example.com")
        .env("GIT_COMMITTER_NAME", "Test Author")
        .env("GIT_COMMITTER_EMAIL", "author@example.com")
        .stdin(std::process::Stdio::null())
        .status()
        .expect("run git");
    assert!(status.success(), "git {args:?} failed");
}

#[test]
fn push_sends_a_new_commit_to_the_upstream() {
    let origin = TestRepo::new();
    origin.commit("first", &[("a.txt", "one\n")], 0);
    let before = origin.head_commit();
    // A non-bare checkout refuses a push onto the branch it has checked out.
    git_in(
        origin.workdir(),
        &["config", "receive.denyCurrentBranch", "ignore"],
    );
    let dest = tempfile::tempdir().expect("clone dir");
    git_in(
        std::env::temp_dir().as_path(),
        &[
            "clone",
            origin.workdir().to_str().expect("utf-8 origin"),
            dest.path().to_str().expect("utf-8 clone"),
        ],
    );
    std::fs::write(dest.path().join("a.txt"), "two\n").expect("edit clone");
    git_in(dest.path(), &["-c", "commit.gpgsign=false", "add", "a.txt"]);
    git_in(
        dest.path(),
        &[
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-m",
            "from the clone",
        ],
    );

    GitoxideAdapter::new(dest.path().to_str().expect("utf-8 clone"))
        .push()
        .expect("push");

    assert_ne!(origin.head_commit(), before);
    let pushed = gix::open(dest.path())
        .expect("open clone")
        .head_id()
        .expect("clone head")
        .detach();
    assert_eq!(origin.head_commit(), pushed);
}

#[test]
fn push_without_an_upstream_reports_git_message() {
    let fixture = TestRepo::new();
    fixture.commit("first", &[("a.txt", "one\n")], 0);
    let elsewhere = tempfile::tempdir().expect("bare remote");
    git_in(elsewhere.path(), &["init", "--bare"]);
    git_in(
        fixture.workdir(),
        &[
            "remote",
            "add",
            "origin",
            elsewhere.path().to_str().expect("utf-8 remote"),
        ],
    );

    let error = fixture.adapter().push().expect_err("no upstream");

    match error {
        DomainError::Unknown(message) => {
            assert!(
                message.to_ascii_lowercase().contains("no upstream"),
                "{message}"
            );
        }
        other => panic!("expected git's message, got {other:?}"),
    }
}

fn cloned_from(origin: &TestRepo) -> tempfile::TempDir {
    let dest = tempfile::tempdir().expect("clone dir");
    git_in(
        std::env::temp_dir().as_path(),
        &[
            "clone",
            origin.workdir().to_str().expect("utf-8 origin"),
            dest.path().to_str().expect("utf-8 clone"),
        ],
    );
    dest
}

fn commit_file(dir: &std::path::Path, name: &str, content: &str, message: &str) {
    std::fs::write(dir.join(name), content).expect("write file");
    git_in(dir, &["-c", "commit.gpgsign=false", "add", name]);
    git_in(
        dir,
        &["-c", "commit.gpgsign=false", "commit", "-m", message],
    );
}

fn head_at(dir: &std::path::Path) -> gix::ObjectId {
    gix::open(dir)
        .expect("open repository")
        .head_id()
        .expect("born HEAD")
        .detach()
}

#[test]
fn pull_fast_forwards_and_updates_the_worktree() {
    let origin = TestRepo::new();
    origin.commit("first", &[("a.txt", "one\n")], 0);
    let dest = cloned_from(&origin);
    // A configured rebase must not change a fast-forward into something else.
    git_in(dest.path(), &["config", "pull.rebase", "true"]);
    origin.commit("second", &[("a.txt", "two\n")], 60);

    GitoxideAdapter::new(dest.path().to_str().expect("utf-8 clone"))
        .pull()
        .expect("pull");

    assert_eq!(head_at(dest.path()), origin.head_commit());
    let worktree = std::fs::read_to_string(dest.path().join("a.txt"))
        .expect("read worktree")
        .replace("\r\n", "\n");
    assert_eq!(worktree, "two\n");
}

#[test]
fn pull_of_diverged_history_does_not_start_a_merge() {
    let origin = TestRepo::new();
    origin.commit("first", &[("a.txt", "one\n")], 0);
    let dest = cloned_from(&origin);
    git_in(dest.path(), &["config", "pull.rebase", "true"]);
    origin.commit("from origin", &[("a.txt", "origin\n")], 60);
    commit_file(dest.path(), "a.txt", "clone\n", "from the clone");
    let before = head_at(dest.path());

    let error = GitoxideAdapter::new(dest.path().to_str().expect("utf-8 clone"))
        .pull()
        .expect_err("diverged");

    match error {
        DomainError::Unknown(message) => {
            assert!(
                message.to_ascii_lowercase().contains("fast-forward"),
                "{message}"
            );
        }
        other => panic!("expected git's message, got {other:?}"),
    }
    assert_eq!(head_at(dest.path()), before);
    let git_dir = dest.path().join(".git");
    assert!(!git_dir.join("MERGE_HEAD").exists());
    assert!(!git_dir.join("rebase-merge").exists());
    assert!(!git_dir.join("rebase-apply").exists());
}

#[test]
fn status_reports_ahead_and_behind_the_upstream() {
    let origin = TestRepo::new();
    origin.commit("first", &[("a.txt", "one\n")], 0);
    let branch = origin.head_branch();
    let dest = cloned_from(&origin);
    let adapter = GitoxideAdapter::new(dest.path().to_str().expect("utf-8 clone"));

    let tracking = adapter
        .status()
        .expect("status")
        .tracking
        .expect("upstream");
    assert_eq!(tracking.upstream, format!("origin/{branch}"));
    assert_eq!((tracking.ahead, tracking.behind), (0, 0));

    origin.commit("second", &[("b.txt", "b\n")], 60);
    adapter.fetch().expect("fetch");
    let tracking = adapter
        .status()
        .expect("status")
        .tracking
        .expect("upstream");
    assert_eq!((tracking.ahead, tracking.behind), (0, 1));

    commit_file(dest.path(), "c.txt", "c\n", "local");
    let tracking = adapter
        .status()
        .expect("status")
        .tracking
        .expect("upstream");
    assert_eq!((tracking.ahead, tracking.behind), (1, 1));
}

#[test]
fn status_without_an_upstream_has_no_tracking() {
    let fixture = TestRepo::new();
    fixture.commit("first", &[("a.txt", "one\n")], 0);
    assert_eq!(fixture.adapter().status().expect("status").tracking, None);
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
    assert_eq!(
        fixture.read_file("a.txt").as_deref(),
        Some("a on feature\n")
    );
    assert_eq!(fixture.read_file("b.txt").as_deref(), Some("b\n"));
    assert_eq!(
        adapter.status(),
        Ok(RepoStatus {
            staged_files: vec![],
            unstaged_files: vec![],
            untracked_files: vec![],
            tracking: None,
        })
    );
}

#[test]
fn checkout_refuses_to_overwrite_local_changes() {
    let fixture = TestRepo::with_feature_branch();
    let main = fixture.head_branch();
    fixture.write_file("a.txt", "uncommitted edit\n");

    let result = fixture.adapter().checkout_branch("feature");

    assert!(
        matches!(result, Err(DomainError::Unknown(ref message)) if !message.is_empty()),
        "{result:?}"
    );
    assert_eq!(fixture.current_branch(), main);
    assert_eq!(
        fixture.read_file("a.txt").as_deref(),
        Some("uncommitted edit\n")
    );
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
        assert!(
            fixture.adapter().checkout_branch(name).is_err(),
            "{name:?} should be rejected"
        );
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
    assert!(reopened
        .try_find_reference("refs/heads/topic")
        .unwrap()
        .is_none());
}

#[test]
fn directory_that_is_not_a_repository_is_reported_as_not_found() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let adapter = GitoxideAdapter::new(dir.path().to_str().expect("utf-8 temp path"));

    assert_eq!(adapter.status(), Err(DomainError::RepositoryNotFound));
    assert_eq!(
        GitoxideAdapter::discover(dir.path().to_str().expect("utf-8 temp path")).err(),
        Some(DomainError::RepositoryNotFound)
    );
    assert_eq!(adapter.log(None), Err(DomainError::RepositoryNotFound));
    assert_eq!(adapter.branches(), Err(DomainError::RepositoryNotFound));
    assert_eq!(
        adapter.stage_paths(&[]),
        Err(DomainError::RepositoryNotFound)
    );
    assert_eq!(
        adapter.stage_paths(&["a.txt".to_string()]),
        Err(DomainError::RepositoryNotFound)
    );
    assert_eq!(
        adapter.commit("hello"),
        Err(DomainError::RepositoryNotFound)
    );
    assert_eq!(adapter.fetch(), Err(DomainError::RepositoryNotFound));
    assert_eq!(adapter.push(), Err(DomainError::RepositoryNotFound));
    assert_eq!(adapter.pull(), Err(DomainError::RepositoryNotFound));
}

// ── stage, unstage, commit ─────────────────────────────────────────
//
// These run real `git add`, `git restore`, and `git commit`, so they need git on PATH.

#[test]
fn stage_unstage_and_commit_a_modification_and_an_untracked_file() {
    let fixture = TestRepo::new();
    fixture.commit("initial", &[("a.txt", "a\n")], 0);
    fixture.stage_and_write(&[("a.txt", "a\n")]);
    fixture.write_file("a.txt", "a changed\n");
    fixture.write_file("new.txt", "new\n");
    let adapter = fixture.adapter();

    adapter
        .stage_paths(&["a.txt".to_string(), "new.txt".to_string()])
        .expect("stage");
    assert_eq!(
        adapter.status().expect("status"),
        RepoStatus {
            staged_files: vec!["a.txt".to_string(), "new.txt".to_string()],
            unstaged_files: vec![],
            untracked_files: vec![],
            tracking: None,
        }
    );

    adapter
        .unstage_paths(&["new.txt".to_string()])
        .expect("unstage new file");
    let status = adapter.status().expect("status");
    assert_eq!(status.staged_files, vec!["a.txt".to_string()]);
    assert_eq!(status.untracked_files, vec!["new.txt".to_string()]);

    adapter
        .unstage_paths(&["a.txt".to_string()])
        .expect("unstage modification");
    let status = adapter.status().expect("status");
    assert_eq!(status.unstaged_files, vec!["a.txt".to_string()]);
    assert_eq!(status.untracked_files, vec!["new.txt".to_string()]);

    adapter
        .stage_paths(&["a.txt".to_string(), "new.txt".to_string()])
        .expect("stage again");
    adapter.commit("record the change").expect("commit");

    let log = adapter.log(Some(1)).expect("log");
    assert_eq!(log[0].message, "record the change");
    assert_eq!(
        adapter.status().expect("status"),
        RepoStatus {
            staged_files: vec![],
            unstaged_files: vec![],
            untracked_files: vec![],
            tracking: None,
        }
    );
}

#[test]
fn blank_commit_message_does_not_move_head() {
    let fixture = TestRepo::with_feature_branch();
    let head = fixture.head_commit();

    let result = fixture.adapter().commit("  \n\t");

    assert_eq!(
        result,
        Err(DomainError::Unknown(BLANK_COMMIT_MESSAGE.to_string()))
    );
    assert_eq!(fixture.head_commit(), head);
}

#[test]
fn empty_or_invalid_paths_do_not_move_head() {
    let fixture = TestRepo::with_feature_branch();
    let head = fixture.head_commit();
    let adapter = fixture.adapter();

    assert_eq!(
        adapter.stage_paths(&[]),
        Err(DomainError::Unknown(NO_PATHS.to_string()))
    );
    assert_eq!(
        adapter.unstage_paths(&[]),
        Err(DomainError::Unknown(NO_PATHS.to_string()))
    );
    for paths in [vec![String::new()], vec!["a\0b".to_string()]] {
        assert_eq!(
            adapter.stage_paths(&paths),
            Err(DomainError::Unknown(INVALID_PATH.to_string())),
            "{paths:?}"
        );
        assert_eq!(
            adapter.unstage_paths(&paths),
            Err(DomainError::Unknown(INVALID_PATH.to_string())),
            "{paths:?}"
        );
    }
    assert_eq!(fixture.head_commit(), head);
}

#[test]
fn commit_of_a_clean_index_does_not_move_head() {
    let fixture = TestRepo::with_feature_branch();
    let head = fixture.head_commit();

    let result = fixture.adapter().commit("nothing to record");

    assert!(
        matches!(result, Err(DomainError::Unknown(ref message)) if !message.is_empty()),
        "{result:?}"
    );
    assert_eq!(fixture.head_commit(), head);
}
