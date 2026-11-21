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
fn log_cursor_continues_past_the_loaded_commits_in_pages_and_then_ends() {
    let fixture = TestRepo::new();
    for n in 0..7 {
        fixture.commit(&format!("commit {n}"), &[("a.txt", &format!("{n}\n"))], n * 60);
    }
    let adapter = fixture.adapter();
    let first_page = adapter.log(Some(2)).expect("first page");
    let loaded: Vec<String> = first_page.iter().map(|commit| commit.id.clone()).collect();
    let messages = |page: &[Commit]| page.iter().map(|commit| commit.message.clone()).collect::<Vec<_>>();

    let mut cursor = adapter.log_cursor(loaded).expect("cursor");
    assert_eq!(messages(&cursor.next_page(3).expect("page")), ["commit 4", "commit 3", "commit 2"]);
    // Short: the start of history. Later pages are empty and ask git for nothing.
    assert_eq!(messages(&cursor.next_page(3).expect("last page")), ["commit 1", "commit 0"]);
    assert_eq!(cursor.next_page(3), Ok(vec![]));
}

#[test]
fn log_cursor_of_an_unborn_repository_is_empty() {
    let fixture = TestRepo::new();
    let mut cursor = fixture.adapter().log_cursor(vec![]).expect("cursor");
    assert_eq!(cursor.next_page(5), Ok(vec![]));
}

#[test]
fn log_cursor_walks_every_local_branch_like_log_does() {
    let fixture = TestRepo::new();
    let base = fixture.commit("base", &[("a.txt", "a\n")], 0);
    fixture.commit("main tip", &[("a.txt", "main\n")], 60);
    fixture.commit_to("refs/heads/feature", "newest feature", &[("b.txt", "b\n")], base, 120);
    let adapter = fixture.adapter();

    let mut cursor = adapter.log_cursor(vec![]).expect("cursor");
    let page = cursor.next_page(10).expect("page");

    assert_eq!(page, adapter.log(None).expect("log"));
}

/// What streaming the first 5,000 commits costs git, chunk by chunk, re-reading from the top
/// (how it was) against one walk continued (how it is), on a real repository:
///
/// ```text
/// GITCTHULHU_BIG_REPO=D:/Repos/Backend cargo test --release -p git_adapter stream_timing -- --ignored --nocapture
/// ```
#[test]
#[ignore = "a timing report on a real repository named by GITCTHULHU_BIG_REPO"]
fn stream_timing_re_read_from_the_top_versus_one_walk() {
    use std::time::{Duration, Instant};
    const FIRST: usize = 100;
    const CHUNK: usize = 300;
    const CAP: usize = 5_000;
    let Some(path) = std::env::var_os("GITCTHULHU_BIG_REPO") else {
        eprintln!("GITCTHULHU_BIG_REPO is not set; nothing measured");
        return;
    };
    let adapter = GitoxideAdapter::new(path.to_str().expect("utf-8 path"));
    let first = adapter.log(Some(FIRST)).expect("first page");

    let mut limit = FIRST;
    let mut re_read = Duration::ZERO;
    let mut chunks = 0;
    let mut worst = Duration::ZERO;
    while limit < CAP {
        limit += CHUNK;
        let started = Instant::now();
        let page = adapter.log(Some(limit)).expect("re-read");
        let cost = started.elapsed();
        re_read += cost;
        worst = worst.max(cost);
        chunks += 1;
        if page.len() < limit {
            break;
        }
    }

    let loaded: Vec<String> = first.iter().map(|commit| commit.id.clone()).collect();
    let started = Instant::now();
    let mut cursor = adapter.log_cursor(loaded).expect("cursor");
    let mut one_walk = Duration::ZERO;
    let mut worst_page = Duration::ZERO;
    for _ in 0..chunks {
        let started = Instant::now();
        let page = cursor.next_page(CHUNK).expect("page");
        let cost = started.elapsed();
        one_walk += cost;
        worst_page = worst_page.max(cost);
        if page.len() < CHUNK {
            break;
        }
    }
    let one_walk_total = started.elapsed();

    eprintln!("stream_timing: {chunks} chunks of {CHUNK} after a first page of {FIRST}");
    eprintln!(
        "  re-read from the top: {:.0} ms in all, worst chunk {:.0} ms",
        re_read.as_secs_f64() * 1_000.0,
        worst.as_secs_f64() * 1_000.0
    );
    eprintln!(
        "  one walk continued:   {:.0} ms in all ({:.0} ms with the walk's start), worst page {:.0} ms",
        one_walk.as_secs_f64() * 1_000.0,
        one_walk_total.as_secs_f64() * 1_000.0,
        worst_page.as_secs_f64() * 1_000.0
    );
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
    let tip_id = tip.to_string();
    assert_eq!(
        find(&head),
        Some(&Branch {
            name: head.clone(),
            is_remote: false,
            is_head: true,
            tip: tip_id.clone(),
        ahead: 0,
        behind: 0,
        })
    );
    assert_eq!(
        find("feature/x"),
        Some(&Branch {
            name: "feature/x".to_string(),
            is_remote: false,
            is_head: false,
            tip: tip_id.clone(),
        ahead: 0,
        behind: 0,
        })
    );
    assert_eq!(
        find("origin/main"),
        Some(&Branch {
            name: "origin/main".to_string(),
            is_remote: true,
            is_head: false,
            tip: tip_id,
        ahead: 0,
        behind: 0,
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

    assert!(status.staged_files.is_empty());
    assert!(status.unstaged_files.is_empty());
    assert!(status.untracked_files.is_empty());
    assert!(status.tracking.is_none());
    assert_eq!(
        status.head_commit_id.as_deref(),
        Some(fixture.head_commit().to_string().as_str())
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
    use core_domain::{ChangeKind, FileDisposition};
    assert_eq!(status.kinds.of(FileDisposition::Unstaged, "a.txt"), ChangeKind::Modified);
    assert_eq!(status.kinds.of(FileDisposition::Untracked, "new.txt"), ChangeKind::Untracked);
}

#[test]
fn a_deleted_worktree_file_is_reported_as_deleted() {
    let fixture = TestRepo::new();
    let files = [("a.txt", "a\n"), ("b.txt", "b\n")];
    fixture.commit("initial", &files, 0);
    fixture.stage_and_write(&files);
    std::fs::remove_file(fixture.workdir().join("b.txt")).expect("remove");

    let status = fixture.adapter().status().expect("status");

    assert_eq!(status.unstaged_files, vec!["b.txt".to_string()]);
    assert_eq!(
        status.kinds.of(core_domain::FileDisposition::Unstaged, "b.txt"),
        core_domain::ChangeKind::Deleted
    );
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
    use core_domain::{ChangeKind, FileDisposition::Staged};
    assert_eq!(status.kinds.of(Staged, "a.txt"), ChangeKind::Modified);
    assert_eq!(status.kinds.of(Staged, "b.txt"), ChangeKind::Added);
    assert_eq!(status.kinds.of(Staged, "gone.txt"), ChangeKind::Deleted);
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
    let status = adapter.status().expect("status");
    assert!(status.staged_files.is_empty());
    assert!(status.unstaged_files.is_empty());
    assert!(status.untracked_files.is_empty());
    assert!(status.tracking.is_none());
    assert!(status.head_commit_id.is_some());
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
    let status = adapter.status().expect("status");
    assert_eq!(
        status.staged_files,
        vec!["a.txt".to_string(), "new.txt".to_string()]
    );
    assert!(status.unstaged_files.is_empty());
    assert!(status.untracked_files.is_empty());
    assert!(status.tracking.is_none());

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
    let status = adapter.status().expect("status");
    assert!(status.staged_files.is_empty());
    assert!(status.unstaged_files.is_empty());
    assert!(status.untracked_files.is_empty());
    assert!(status.tracking.is_none());
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

#[test]
fn commit_patch_is_the_diff_of_that_commit() {
    let fixture = TestRepo::new();
    fixture.commit("first", &[("a.txt", "one\n")], 0);
    let second = fixture.commit("second", &[("a.txt", "one\ntwo\n")], 1);

    let patches = fixture
        .adapter()
        .commit_patch(&second.to_string())
        .expect("patch");

    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].path, "a.txt");
    assert!(patches[0].hunks[0]
        .lines
        .iter()
        .any(|line| { line.kind == core_domain::DiffLineKind::Added && line.text == "two" }));
    assert!(fixture.adapter().commit_patch("-rf").is_err());
}

#[test]
fn untrack_keeps_the_file_on_disk_and_stages_its_removal() {
    let fixture = TestRepo::new();
    let files = [("keep.txt", "k\n"), ("secret.env", "token\n")];
    fixture.commit("first", &files, 0);
    fixture.stage_and_write(&files);
    let adapter = fixture.adapter();

    adapter.untrack_paths(&["secret.env".to_string()]).expect("untrack");

    let status = adapter.status().expect("status");
    assert!(status.staged_files.iter().any(|path| path == "secret.env"), "removal staged: {status:?}");
    assert!(status.untracked_files.iter().any(|path| path == "secret.env"), "now unversioned: {status:?}");
    assert!(fixture.workdir().join("secret.env").exists(), "the file stays on disk");
    assert!(adapter.untrack_paths(&["../outside".to_string()]).is_err());
}

#[test]
fn compare_file_diffs_the_disk_against_a_revision_and_two_revisions() {
    let fixture = TestRepo::new();
    let first = fixture.commit("first", &[("a.txt", "one\n")], 0);
    let second = fixture.commit("second", &[("a.txt", "one\ntwo\n")], 1);
    fixture.write_file("a.txt", "one\ntwo\nthree\n");
    let adapter = fixture.adapter();
    let added = |patch: &core_domain::Patch| -> Vec<String> {
        patch
            .hunks
            .iter()
            .flat_map(|hunk| hunk.lines.iter())
            .filter(|line| line.kind == core_domain::DiffLineKind::Added)
            .map(|line| line.text.clone())
            .collect()
    };

    let disk = adapter
        .compare_file("a.txt", &first.to_string(), None)
        .expect("disk vs first");
    assert_eq!(disk.path, "a.txt");
    assert_eq!(added(&disk), vec!["two", "three"]);

    let revisions = adapter
        .compare_file("a.txt", &first.to_string(), Some(&second.to_string()))
        .expect("first vs second");
    assert_eq!(added(&revisions), vec!["two"]);

    let same = adapter
        .compare_file("a.txt", &second.to_string(), Some(&second.to_string()))
        .expect("same");
    assert!(same.hunks.is_empty());

    // A file the revision lacks reads as empty, so every line is added.
    fixture.write_file("new.txt", "fresh\n");
    let missing = adapter
        .compare_file("new.txt", &first.to_string(), None)
        .expect("missing side");
    assert_eq!(added(&missing), vec!["fresh"]);

    assert!(adapter.compare_file("a.txt", "no-such-branch", None).is_err());
    assert!(adapter.compare_file("a.txt", "--output=x", None).is_err());
    assert!(adapter.compare_file("a.txt", "HEAD..HEAD~1", None).is_err());
}

#[test]
fn file_patch_stage_hunk_and_discard_hunk_round_trip() {
    let fixture = TestRepo::new();
    fixture.commit("first", &[("a.txt", "one\n")], 0);
    fixture.stage_and_write(&[("a.txt", "one\n")]);
    fixture.write_file("a.txt", "one\ntwo\n");
    let adapter = fixture.adapter();

    let patch = adapter
        .file_patch("a.txt", core_domain::FileDisposition::Unstaged)
        .expect("diff");
    let hunk = patch.hunks.first().expect("hunk").clone();
    adapter.stage_hunk("a.txt", &hunk).expect("stage hunk");
    let status = adapter.status().expect("status");
    assert!(
        status.staged_files.iter().any(|path| path == "a.txt"),
        "{status:?}"
    );

    adapter
        .discard_hunk("a.txt", &hunk, core_domain::FileDisposition::Staged)
        .expect("discard hunk");
    let status = adapter.status().expect("status after discard");
    assert!(
        !status.staged_files.iter().any(|path| path == "a.txt"),
        "{status:?}"
    );
}

#[test]
fn discard_path_deletes_an_untracked_file_and_restores_a_tracked_one() {
    let fixture = TestRepo::new();
    fixture.commit("first", &[("a.txt", "one\n")], 0);
    fixture.write_file("a.txt", "changed\n");
    fixture.write_file("extra.txt", "new\n");
    let adapter = fixture.adapter();

    adapter
        .discard_path("extra.txt", core_domain::FileDisposition::Untracked)
        .expect("discard untracked");
    assert!(!fixture.workdir().join("extra.txt").exists());
    assert!(fixture.workdir().join("a.txt").exists());

    adapter
        .discard_path("a.txt", core_domain::FileDisposition::Unstaged)
        .expect("restore tracked");
    let restored = std::fs::read_to_string(fixture.workdir().join("a.txt")).expect("read");
    assert_eq!(restored.replace("\r\n", "\n"), "one\n");
    assert!(adapter
        .discard_path("../a.txt", core_domain::FileDisposition::Untracked)
        .is_err());
}

#[test]
fn init_creates_a_repository_and_refuses_a_second_time() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().to_str().expect("utf-8 temp path");

    GitoxideAdapter::init(path).expect("init");
    assert!(GitoxideAdapter::discover(path).is_ok());

    let again = GitoxideAdapter::init(path);
    assert!(
        matches!(again, Err(DomainError::Unknown(ref message)) if message == "Already a Git repository."),
        "{again:?}"
    );
    assert!(GitoxideAdapter::init("-not-a-path").is_err());
}

#[test]
fn clone_repository_copies_a_local_repository_under_the_parent() {
    let source = TestRepo::new();
    source.commit("first", &[("a.txt", "one\n")], 0);
    let parent = tempfile::tempdir().expect("clone parent");
    let parent_path = parent.path().to_str().expect("utf-8 parent");
    let source_path = source.workdir().to_str().expect("utf-8 source");

    let created = GitoxideAdapter::clone_repository(source_path, parent_path).expect("clone");

    assert!(GitoxideAdapter::discover(&created).is_ok());
    let read = std::fs::read_to_string(std::path::Path::new(&created).join("a.txt")).expect("file");
    assert_eq!(read.replace("\r\n", "\n"), "one\n");
    assert!(GitoxideAdapter::clone_repository("-rf", parent_path).is_err());
    assert!(GitoxideAdapter::clone_repository(source_path, parent_path).is_err());
}

#[test]
fn a_clone_reports_progress_and_can_name_its_folder_and_branch() {
    let source = TestRepo::new();
    source.commit("first", &[("a.txt", "one\n")], 0);
    git_in(source.workdir(), &["branch", "release"]);
    let parent = tempfile::tempdir().expect("clone parent");
    let request = core_domain::CloneRequest {
        url: source.workdir().to_str().expect("utf-8").to_string(),
        parent: parent.path().to_str().expect("utf-8").to_string(),
        name: "my copy".to_string(),
        branch: Some("release".to_string()),
    };
    let mut lines = Vec::new();

    let created = GitoxideAdapter::clone_with_progress(&request, &mut |line| lines.push(line.to_string()), &std::sync::atomic::AtomicBool::new(false))
        .expect("clone");

    assert!(created.ends_with("my copy"));
    assert!(lines.iter().any(|line| line.starts_with("Cloning into")), "{lines:?}");
    let head = std::fs::read_to_string(std::path::Path::new(&created).join(".git").join("HEAD")).expect("HEAD");
    assert_eq!(head.trim(), "ref: refs/heads/release");
}

#[test]
fn a_cancelled_clone_leaves_no_folder_behind() {
    let source = TestRepo::new();
    source.commit("first", &[("a.txt", "one\n")], 0);
    let parent = tempfile::tempdir().expect("clone parent");
    let request = core_domain::CloneRequest {
        url: source.workdir().to_str().expect("utf-8").to_string(),
        parent: parent.path().to_str().expect("utf-8").to_string(),
        name: "copy".to_string(),
        branch: None,
    };

    let result = GitoxideAdapter::clone_with_progress(&request, &mut |_| {}, &std::sync::atomic::AtomicBool::new(true));

    assert!(matches!(&result, Err(DomainError::Unknown(message)) if message.starts_with("Clone cancelled")), "{result:?}");
    assert!(!parent.path().join("copy").exists());
}

#[test]
fn a_failed_clone_says_why_and_bad_names_are_refused_before_git_runs() {
    let parent = tempfile::tempdir().expect("clone parent");
    let mut request = core_domain::CloneRequest {
        url: parent.path().join("no such repository").to_str().expect("utf-8").to_string(),
        parent: parent.path().to_str().expect("utf-8").to_string(),
        name: "copy".to_string(),
        branch: None,
    };
    let none = std::sync::atomic::AtomicBool::new(false);
    let failed = GitoxideAdapter::clone_with_progress(&request, &mut |_| {}, &none);
    assert!(matches!(&failed, Err(DomainError::Unknown(message)) if message.contains("does not appear to be a git repository")), "{failed:?}");
    assert!(!parent.path().join("copy").exists());

    for bad in ["", "..", "-x", "a/b", r"a\b", "c:"] {
        request.name = bad.to_string();
        assert!(GitoxideAdapter::clone_with_progress(&request, &mut |_| {}, &none).is_err(), "{bad:?}");
    }
    request.name = "copy".to_string();
    request.branch = Some("--upload-pack=evil".to_string());
    assert!(GitoxideAdapter::clone_with_progress(&request, &mut |_| {}, &none).is_err());
}

#[test]
fn a_stash_lists_its_commit_and_its_patch_is_the_stashed_change() {
    let fixture = TestRepo::new();
    fixture.commit("first", &[("a.txt", "one\n")], 0);
    fixture.stage_and_write(&[("a.txt", "one\n")]);
    fixture.write_file("a.txt", "one\ntwo\n");
    let adapter = fixture.adapter();
    adapter.stash_push("wip: colon: in subject", false).expect("stash");

    let stash = adapter.stashes().expect("list").remove(0);
    assert_eq!(stash.message, format!("On {}: wip: colon: in subject", fixture.head_branch()));
    assert_eq!(stash.commit_id.len(), 40);

    // A stash is a merge commit; its first-parent diff is `git stash show -p`.
    let patches = adapter.commit_patch(&stash.commit_id).expect("stash patch");
    let patch = patches.iter().find(|patch| patch.path == "a.txt").expect("a.txt in stash");
    assert!(
        patch
            .hunks
            .iter()
            .flat_map(|hunk| &hunk.lines)
            .any(|line| line.kind == core_domain::DiffLineKind::Added && line.text == "two"),
        "{patches:?}"
    );
}

#[test]
fn a_merge_commit_patch_is_its_first_parent_diff() {
    let fixture = TestRepo::new();
    fixture.commit("base", &[("a.txt", "one\n")], 0);
    let trunk = fixture.head_branch();
    git_in(fixture.workdir(), &["checkout", "-q", "-b", "side"]);
    fixture.write_file("b.txt", "side\n");
    git_in(fixture.workdir(), &["add", "b.txt"]);
    git_in(fixture.workdir(), &["commit", "-q", "-m", "side"]);
    git_in(fixture.workdir(), &["checkout", "-q", &trunk]);
    git_in(fixture.workdir(), &["merge", "-q", "--no-ff", "-m", "merge side", "side"]);
    let merge = head_at(fixture.workdir());

    let patches = fixture.adapter().commit_patch(&merge.to_string()).expect("merge patch");
    assert_eq!(patches.iter().map(|patch| patch.path.as_str()).collect::<Vec<_>>(), ["b.txt"]);
}

#[test]
fn stash_push_apply_pop_and_drop_round_trip() {
    let fixture = TestRepo::new();
    fixture.commit("first", &[("a.txt", "one\n")], 0);
    fixture.stage_and_write(&[("a.txt", "one\n")]);
    fixture.write_file("a.txt", "one\ntwo\n");
    fixture.write_file("extra.txt", "new\n");
    let adapter = fixture.adapter();

    adapter.stash_push("wip", false).expect("stash");
    let stashes = adapter.stashes().expect("list");
    assert_eq!(stashes.len(), 1);
    assert_eq!(stashes[0].index, 0);
    assert!(stashes[0].message.contains("wip"), "{}", stashes[0].message);
    let stashed = std::fs::read_to_string(fixture.workdir().join("a.txt")).expect("read");
    assert_eq!(stashed.replace("\r\n", "\n"), "one\n");
    assert!(fixture.workdir().join("extra.txt").exists());

    adapter.stash_apply(0).expect("apply");
    let applied = std::fs::read_to_string(fixture.workdir().join("a.txt")).expect("read");
    assert_eq!(applied.replace("\r\n", "\n"), "one\ntwo\n");
    assert_eq!(adapter.stashes().expect("list after apply").len(), 1);

    adapter
        .discard_path("a.txt", core_domain::FileDisposition::Unstaged)
        .expect("clean the applied edit so pop can replay it");
    adapter.stash_pop(0).expect("pop");
    assert!(adapter.stashes().expect("list after pop").is_empty());
    let popped = std::fs::read_to_string(fixture.workdir().join("a.txt")).expect("read");
    assert_eq!(popped.replace("\r\n", "\n"), "one\ntwo\n");

    adapter
        .stash_push("", false)
        .expect("stash with git's own subject");
    assert_eq!(adapter.stashes().expect("list before drop").len(), 1);
    adapter.stash_drop(0).expect("drop");
    assert!(adapter.stashes().expect("list after drop").is_empty());
    let dropped = std::fs::read_to_string(fixture.workdir().join("a.txt")).expect("read");
    assert_eq!(dropped.replace("\r\n", "\n"), "one\n");
    assert!(adapter.stash_push("\0", false).is_err());
}

#[test]
fn create_rename_and_delete_a_local_branch() {
    let fixture = TestRepo::with_feature_branch();
    let head = fixture.head_branch();
    let adapter = fixture.adapter();

    adapter
        .create_branch("from-feature", "feature")
        .expect("create at the selected tip");
    assert_eq!(fixture.current_branch(), head);
    adapter
        .checkout_branch("from-feature")
        .expect("checkout created");
    assert_eq!(
        fixture.read_file("a.txt").as_deref(),
        Some("a on feature\n")
    );
    adapter.checkout_branch(&head).expect("return");

    adapter
        .create_branch("topic", &head)
        .expect("create at head");
    adapter.rename_branch("topic", "topic-2").expect("rename");
    let local = local_names(&adapter);
    assert!(local.iter().any(|name| name == "topic-2"));
    assert!(!local.iter().any(|name| name == "topic"));
    adapter
        .delete_branch("topic-2")
        .expect("delete a merged branch");
    assert!(!local_names(&adapter).iter().any(|name| name == "topic-2"));

    let kept = adapter.delete_branch("feature");
    assert!(kept.is_err(), "{kept:?}");
    assert!(local_names(&adapter).iter().any(|name| name == "feature"));

    adapter
        .rename_branch(&head, "renamed")
        .expect("rename current");
    assert_eq!(fixture.current_branch(), "renamed");
    assert!(adapter.delete_branch("renamed").is_err());
    assert_eq!(fixture.current_branch(), "renamed");

    for name in ["", "-d", "foo/../bar", "a@{1}", "has space"] {
        assert!(
            adapter.create_branch(name, "renamed").is_err(),
            "{name:?} should be rejected"
        );
    }
    assert!(!local_names(&adapter).iter().any(|name| name == "-d"));
}

#[test]
fn tags_list_commit_tags_by_name_and_skip_a_tag_that_is_not_a_commit() {
    let fixture = TestRepo::new();
    let commit = fixture.commit("first", &[("a.txt", "one\n")], 0);
    assert!(fixture.adapter().tags().expect("no tags yet").is_empty());

    fixture.reference("refs/tags/v1", commit);
    let id = commit.to_string();
    git_in(
        fixture.workdir(),
        &[
            "-c",
            "tag.gpgsign=false",
            "tag",
            "-a",
            "v2",
            "-m",
            "release",
            &id,
        ],
    );
    git_in(
        fixture.workdir(),
        &["-c", "tag.gpgsign=false", "tag", "tree-only", "HEAD^{tree}"],
    );

    let tags = fixture.adapter().tags().expect("tags");
    assert_eq!(
        tags,
        vec![
            core_domain::Tag {
                name: "v1".to_string(),
                commit_id: id.clone(),
            },
            core_domain::Tag {
                name: "v2".to_string(),
                commit_id: id,
            },
        ]
    );
}

#[test]
fn cherry_pick_reset_merge_and_rebase_use_the_selected_commit_or_branch() {
    let fixture = TestRepo::with_feature_branch();
    let adapter = fixture.adapter();
    let main = fixture.head_commit();
    let main_id = main.to_string();
    let feature_id = adapter
        .log(None)
        .expect("log")
        .into_iter()
        .find(|commit| commit.message.contains("feature work"))
        .expect("feature commit")
        .id;
    let branch = fixture.head_branch();

    assert!(adapter.cherry_pick("HEAD").is_err());
    assert!(adapter.reset(&branch, ResetMode::Hard).is_err());

    adapter.cherry_pick(&feature_id).expect("cherry-pick");
    assert_eq!(fixture.current_branch(), branch);
    assert_eq!(fixture.read_file("b.txt").as_deref(), Some("b\n"));
    assert_ne!(fixture.head_commit(), main);

    adapter
        .reset(&main_id, ResetMode::Hard)
        .expect("hard reset");
    assert_eq!(fixture.head_commit(), main);
    assert!(fixture.read_file("b.txt").is_none());

    fixture.write_file("c.txt", "c\n");
    git_in(fixture.workdir(), &["add", "--", "c.txt"]);
    git_in(fixture.workdir(), &["commit", "-m", "extra"]);
    let extra = fixture.head_commit();
    adapter
        .reset(&main_id, ResetMode::Soft)
        .expect("soft reset");
    assert_eq!(fixture.head_commit(), main);
    let staged = adapter.status().expect("status after soft reset");
    assert!(
        staged.staged_files.iter().any(|path| path == "c.txt"),
        "{:?}",
        staged.staged_files
    );

    adapter
        .reset(&extra.to_string(), ResetMode::Mixed)
        .expect("mixed reset");
    assert_eq!(fixture.head_commit(), extra);
    assert!(adapter
        .status()
        .expect("status after mixed reset")
        .staged_files
        .is_empty());

    adapter
        .reset(&main_id, ResetMode::Hard)
        .expect("back to main");
    adapter.merge_branch("feature", true).expect("explicit merge");
    let tip = adapter.log(Some(1)).expect("log after merge");
    assert_eq!(tip[0].parent_ids.len(), 2);
    assert_eq!(fixture.read_file("b.txt").as_deref(), Some("b\n"));
    assert_eq!(fixture.current_branch(), branch);
    assert!(adapter.merge_branch("-feature", false).is_err());

    adapter
        .reset(&main_id, ResetMode::Hard)
        .expect("clean main");
    fixture.write_file("c.txt", "c\n");
    git_in(fixture.workdir(), &["add", "--", "c.txt"]);
    git_in(fixture.workdir(), &["commit", "-m", "extra again"]);
    adapter.rebase_onto("feature").expect("rebase onto feature");
    assert_eq!(fixture.current_branch(), branch);
    assert_eq!(fixture.read_file("b.txt").as_deref(), Some("b\n"));
    assert_eq!(fixture.read_file("c.txt").as_deref(), Some("c\n"));
    assert!(adapter.rebase_onto("").is_err());
}

fn local_names(adapter: &GitoxideAdapter) -> Vec<String> {
    adapter
        .branches()
        .expect("branches")
        .into_iter()
        .filter(|branch| !branch.is_remote)
        .map(|branch| branch.name)
        .collect()
}

// ── Worktrees ─────────────────────────────────────────────────────────────

/// A directory for a linked worktree that git has not created yet.
fn linked_path(parent: &tempfile::TempDir, name: &str) -> String {
    parent
        .path()
        .join(name)
        .to_str()
        .expect("utf-8 temp path")
        .to_string()
}

fn same_path(left: &str, right: &str) -> bool {
    let normalise = |path: &str| {
        path.replace('\\', "/")
            .trim_end_matches('/')
            .to_ascii_lowercase()
    };
    normalise(left) == normalise(right)
}

#[test]
fn worktrees_lists_main_after_init() {
    let fixture = TestRepo::with_feature_branch();
    let list = fixture.adapter().worktrees().expect("list worktrees");
    assert_eq!(list.len(), 1, "{list:?}");
    assert!(list[0].is_main);
    assert!(!list[0].locked);
    assert_eq!(list[0].branch, fixture.head_branch());
}

#[test]
fn add_worktree_checks_out_branch_and_lists() {
    let fixture = TestRepo::with_feature_branch();
    let adapter = fixture.adapter();
    let parent = tempfile::tempdir().expect("temp dir");
    let path = linked_path(&parent, "linked");

    adapter
        .add_worktree(&path, "feature", false, None)
        .expect("add worktree on an existing branch");

    let list = adapter.worktrees().expect("list worktrees");
    assert_eq!(list.len(), 2, "{list:?}");
    assert!(list[0].is_main);
    let linked = &list[1];
    assert!(!linked.is_main);
    assert_eq!(linked.branch, "feature");
    assert!(same_path(&linked.path, &path), "{} vs {path}", linked.path);
    let head = gix::open(&path)
        .expect("open linked worktree")
        .head_name()
        .unwrap()
        .expect("on a branch")
        .shorten()
        .to_string();
    assert_eq!(head, "feature");
    assert_eq!(
        std::fs::read_to_string(std::path::Path::new(&path).join("a.txt"))
            .ok()
            .as_deref(),
        Some("a on feature\n")
    );
}

#[test]
fn add_worktree_can_create_a_new_branch() {
    let fixture = TestRepo::with_feature_branch();
    let adapter = fixture.adapter();
    let parent = tempfile::tempdir().expect("temp dir");
    let path = linked_path(&parent, "topic-tree");

    adapter
        .add_worktree(&path, "topic", true, None)
        .expect("add worktree with a new branch");

    let list = adapter.worktrees().expect("list worktrees");
    assert_eq!(list.len(), 2, "{list:?}");
    assert_eq!(list[1].branch, "topic");
    assert!(adapter.add_worktree(&path, "other", true, None).is_err());
    assert!(adapter.add_worktree("-bad", "topic-2", true, None).is_err());
    assert!(adapter.add_worktree(&path, "bad name", true, None).is_err());
}

#[test]
fn add_worktree_can_create_a_branch_at_a_commit() {
    let fixture = TestRepo::with_feature_branch();
    let adapter = fixture.adapter();
    let base = fixture.head_commit().to_string();
    let parent = tempfile::tempdir().expect("temp dir");
    let path = linked_path(&parent, "from-commit");

    adapter
        .add_worktree(&path, "from-base", true, Some(&base))
        .expect("add worktree at commit");

    let list = adapter.worktrees().expect("list");
    assert_eq!(list.len(), 2, "{list:?}");
    assert_eq!(list[1].branch, "from-base");
}

#[test]
fn checkout_detached_points_head_at_the_commit() {
    let fixture = TestRepo::with_feature_branch();
    let adapter = fixture.adapter();
    let main = fixture.head_branch();
    adapter.checkout_branch(&main).expect("on main");
    let tip = fixture.head_commit().to_string();
    adapter.checkout_branch("feature").expect("on feature");

    adapter
        .checkout_detached(&tip)
        .expect("detach at main tip");

    let status = adapter.status().expect("status");
    assert_eq!(status.head_commit_id.as_deref(), Some(tip.as_str()));
    assert!(
        !adapter
            .branches()
            .expect("branches")
            .iter()
            .any(|branch| branch.is_head && !branch.is_remote)
    );
    assert!(adapter.checkout_detached("").is_err());
    assert!(adapter.checkout_detached("-oops").is_err());
}

#[test]
fn remove_worktree_drops_linked() {
    let fixture = TestRepo::with_feature_branch();
    let adapter = fixture.adapter();
    let parent = tempfile::tempdir().expect("temp dir");
    let path = linked_path(&parent, "linked");
    adapter
        .add_worktree(&path, "feature", false, None)
        .expect("add worktree");
    assert_eq!(adapter.worktrees().unwrap().len(), 2);

    adapter
        .remove_worktree(&path, false)
        .expect("remove clean worktree");

    let list = adapter.worktrees().expect("list worktrees");
    assert_eq!(list.len(), 1, "{list:?}");
    assert!(list[0].is_main);
    assert!(!std::path::Path::new(&path).exists());
}

#[test]
fn force_remove_dirty_linked_worktree() {
    let fixture = TestRepo::with_feature_branch();
    let adapter = fixture.adapter();
    let parent = tempfile::tempdir().expect("temp dir");
    let path = linked_path(&parent, "linked");
    adapter
        .add_worktree(&path, "feature", false, None)
        .expect("add worktree");
    std::fs::write(std::path::Path::new(&path).join("a.txt"), "dirty\n").expect("dirty a file");

    let refused = adapter.remove_worktree(&path, false);
    assert!(refused.is_err(), "{refused:?}");
    assert_eq!(adapter.worktrees().unwrap().len(), 2);

    adapter
        .remove_worktree(&path, true)
        .expect("force remove dirty worktree");
    assert_eq!(adapter.worktrees().unwrap().len(), 1);
}

#[test]
fn remove_worktree_refuses_main_and_bad_paths() {
    let fixture = TestRepo::with_feature_branch();
    let adapter = fixture.adapter();
    let main = fixture
        .workdir()
        .to_str()
        .expect("utf-8 temp path")
        .to_string();
    assert!(adapter.remove_worktree(&main, true).is_err());
    assert!(adapter.remove_worktree("", false).is_err());
    assert!(adapter.remove_worktree("-f", false).is_err());
}

/// `main` and `feature` both change `a.txt` from the same base, so merging conflicts.
fn conflicting_branches() -> TestRepo {
    let fixture = TestRepo::new();
    fixture.commit("base", &[("a.txt", "base\n"), ("b.txt", "b\n")], 0);
    fixture.stage_and_write(&[("a.txt", "base\n"), ("b.txt", "b\n")]);
    let main = fixture.current_branch();
    git_in(fixture.workdir(), &["checkout", "-q", "-b", "feature"]);
    fixture.write_file("a.txt", "theirs\n");
    git_in(fixture.workdir(), &["commit", "-qam", "feature edit"]);
    git_in(fixture.workdir(), &["checkout", "-q", &main]);
    fixture.write_file("a.txt", "yours\n");
    git_in(fixture.workdir(), &["commit", "-qam", "main edit"]);
    fixture
}

#[test]
fn a_conflicting_merge_stays_in_progress_with_the_path_listed_once() {
    let fixture = conflicting_branches();
    let adapter = fixture.adapter();

    assert!(adapter.merge_branch("feature", false).is_err(), "git reports the conflict");
    let status = adapter.status().expect("status");

    assert_eq!(status.operation, Some(core_domain::RepoOperation::Merge));
    assert_eq!(status.conflicted_files, vec!["a.txt".to_string()]);
    assert!(!status.staged_files.contains(&"a.txt".to_string()), "not a staged deletion: {:?}", status.staged_files);
    assert!(!status.unstaged_files.contains(&"a.txt".to_string()));
}

#[test]
fn taking_their_side_then_continuing_commits_the_merge() {
    let fixture = conflicting_branches();
    let adapter = fixture.adapter();
    let _ = adapter.merge_branch("feature", false);

    adapter.resolve_conflict("a.txt", core_domain::ConflictSide::Theirs).expect("theirs");
    assert_eq!(fixture.read_file("a.txt").as_deref(), Some("theirs\n"));
    assert!(adapter.status().expect("status").conflicted_files.is_empty());

    adapter.continue_operation(core_domain::RepoOperation::Merge).expect("commit merge");
    let status = adapter.status().expect("status after");
    assert_eq!(status.operation, None);
    assert_eq!(adapter.log(Some(1)).expect("log")[0].parent_ids.len(), 2);
}

#[test]
fn the_merge_tool_is_the_chosen_one_or_the_only_one_defined_never_a_guess() {
    assert_eq!(pick_merge_tool("p4merge", "", false), Some("p4merge".into()));
    let one = "mergetool.p4merge.cmd\nmergetool.p4merge.cmd\n";
    assert_eq!(pick_merge_tool("", one, false), Some("p4merge".into()), "defined twice (global and local) is one tool");
    let two = "mergetool.p4merge.cmd\nmergetool.cthulhu.cmd\n";
    assert_eq!(pick_merge_tool("", two, false), None, "two tools and no choice: ask the user to choose");
    assert_eq!(pick_merge_tool("cthulhu", two, false), Some("cthulhu".into()));
    assert_eq!(pick_merge_tool("", "", false), None);
    assert_eq!(pick_merge_tool("--evil", "mergetool.-x.cmd\n", false), None, "never an option-like name");
}

#[test]
fn the_built_in_resolver_outranks_a_tool_that_is_only_defined_but_not_one_the_user_chose() {
    let one = "mergetool.p4merge.cmd
";
    assert_eq!(pick_merge_tool("", one, true), Some(BUILTIN_MERGE_TOOL.into()));
    assert_eq!(pick_merge_tool("", "", true), Some(BUILTIN_MERGE_TOOL.into()));
    assert_eq!(pick_merge_tool("p4merge", one, true), Some("p4merge".into()), "merge.tool is the user's choice");
}

#[test]
fn a_merge_preview_names_the_conflicts_and_changes_nothing() {
    let fixture = conflicting_branches();
    let adapter = fixture.adapter();
    let before = adapter.status().expect("status before");
    let head = adapter.log(Some(1)).expect("log")[0].id.clone();

    let preview = adapter.merge_preview("feature").expect("preview");

    assert_eq!(preview.commits, 1);
    assert!(!preview.fast_forward);
    assert_eq!(preview.files, vec!["a.txt"]);
    assert_eq!(preview.conflicts, vec!["a.txt"]);
    assert_eq!(adapter.status().expect("status after"), before, "nothing staged, nothing in progress");
    assert_eq!(adapter.log(Some(1)).expect("log")[0].id, head);
    assert!(adapter.merge_preview("--evil").is_err(), "an option-like name is refused");
}

#[test]
fn a_merge_preview_of_a_branch_already_merged_is_empty() {
    let fixture = conflicting_branches();
    let adapter = fixture.adapter();
    let head = adapter.log(Some(1)).expect("log")[0].id.clone();
    assert_eq!(adapter.merge_preview(&head), Ok(MergePreview::default()));
}

#[test]
fn the_repositorys_own_merge_tool_setting_is_read() {
    let fixture = TestRepo::new();
    fixture.commit("base", &[("a.txt", "a\n")], 0);
    git_in(fixture.workdir(), &["config", "merge.tool", "fixture-tool"]);
    assert_eq!(fixture.adapter().merge_tool(), Ok(Some("fixture-tool".into())));
}

#[test]
fn a_merge_tool_that_saves_the_file_resolves_and_stages_it() {
    let fixture = conflicting_branches();
    let adapter = fixture.adapter();
    let _ = adapter.merge_branch("feature", false);
    // A tool that keeps their side and says so through its exit code, as a real tool would.
    git_in(fixture.workdir(), &["config", "mergetool.fixture.cmd", r#"cp "$REMOTE" "$MERGED""#]);
    git_in(fixture.workdir(), &["config", "mergetool.fixture.trustExitCode", "true"]);
    git_in(fixture.workdir(), &["config", "mergetool.keepBackup", "false"]);

    adapter.run_merge_tool("a.txt", "fixture").expect("the tool saved the merge");

    assert_eq!(fixture.read_file("a.txt").as_deref(), Some("theirs\n"));
    assert!(adapter.status().expect("status").conflicted_files.is_empty(), "git staged the result");
}

#[test]
fn the_built_in_resolver_runs_with_nothing_in_git_config() {
    let fixture = conflicting_branches();
    let adapter = fixture.adapter();
    let _ = adapter.merge_branch("feature", false);
    // A stand-in for Cthulhu Resolve that takes the same arguments and keeps their side.
    let tools = tempfile::tempdir().expect("tools");
    let stand_in = tools.path().join("stand in.sh");
    std::fs::write(
        &stand_in,
        "#!/bin/sh
[ \"$1\" = merge ] || exit 2
shift
while [ $# -gt 1 ]; do case \"$1\" in --remote) remote=\"$2\";; --merged) merged=\"$2\";; esac; shift 2; done
cp \"$remote\" \"$merged\"
",
    )
    .expect("write the stand-in");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&stand_in, std::fs::Permissions::from_mode(0o755)).expect("executable");
    }
    std::env::set_var("GITCTHULHU_RESOLVER", &stand_in);

    assert_eq!(adapter.merge_tool(), Ok(Some(BUILTIN_MERGE_TOOL.into())), "no merge.tool: the built-in one");
    let ran = adapter.run_merge_tool("a.txt", BUILTIN_MERGE_TOOL);
    std::env::remove_var("GITCTHULHU_RESOLVER");

    ran.expect("the resolver saved the merge");
    assert_eq!(fixture.read_file("a.txt").as_deref(), Some("theirs
"));
    assert!(adapter.status().expect("status").conflicted_files.is_empty(), "git staged the result");
    assert!(fixture.read_file("a.txt.orig").is_none(), "no backup file is left behind");
}

#[test]
fn a_merge_tool_that_exits_unresolved_leaves_the_conflict() {
    let fixture = conflicting_branches();
    let adapter = fixture.adapter();
    let _ = adapter.merge_branch("feature", false);
    git_in(fixture.workdir(), &["config", "mergetool.quitter.cmd", "exit 1"]);
    git_in(fixture.workdir(), &["config", "mergetool.quitter.trustExitCode", "true"]);
    git_in(fixture.workdir(), &["config", "mergetool.keepBackup", "false"]);

    assert!(adapter.run_merge_tool("a.txt", "quitter").is_err());
    assert_eq!(adapter.status().expect("status").conflicted_files, vec!["a.txt".to_string()]);
    assert!(adapter.run_merge_tool("a.txt", "--tool=x").is_err(), "an option-like name is refused");
}

#[test]
fn aborting_a_merge_restores_your_side() {
    let fixture = conflicting_branches();
    let adapter = fixture.adapter();
    let _ = adapter.merge_branch("feature", false);

    adapter.abort_operation(core_domain::RepoOperation::Merge).expect("abort");
    let status = adapter.status().expect("status");
    assert_eq!(status.operation, None);
    assert!(status.conflicted_files.is_empty());
    assert_eq!(fixture.read_file("a.txt").as_deref(), Some("yours\n"));
}

#[test]
fn a_stash_can_take_untracked_files() {
    let fixture = TestRepo::new();
    fixture.commit("base", &[("a.txt", "a\n")], 0);
    fixture.stage_and_write(&[("a.txt", "a\n")]);
    fixture.write_file("new.txt", "new\n");
    let adapter = fixture.adapter();

    adapter.stash_push("with new", true).expect("stash -u");
    assert!(adapter.status().expect("status").untracked_files.is_empty());
    assert_eq!(adapter.stashes().expect("stashes").len(), 1);
}


// ── Opening: the reads the window does first, on awkward repositories ─────────

/// Discover, then every read the window's first load makes. Panics with the failing read.
fn open_like_the_window(path: &str) -> GitoxideAdapter {
    let adapter = GitoxideAdapter::discover(path).expect("discover");
    adapter.status().expect("status");
    adapter.log(Some(50)).expect("log");
    adapter.branches().expect("branches");
    adapter.stashes().expect("stashes");
    adapter.tags().expect("tags");
    adapter
}

#[test]
fn opening_an_empty_unborn_repository_reads_cleanly() {
    let fixture = TestRepo::new();
    let adapter = open_like_the_window(fixture.workdir().to_str().expect("utf-8"));
    let status = adapter.status().expect("status");
    assert_eq!(status.tracking, None);
    assert!(adapter.log(None).expect("log").is_empty());
}

#[test]
fn opening_a_detached_head_reads_cleanly_with_no_upstream() {
    let fixture = TestRepo::new();
    let base = fixture.commit("base", &[("a.txt", "a\n")], 0);
    fixture.reference("HEAD", base);
    let adapter = open_like_the_window(fixture.workdir().to_str().expect("utf-8"));
    assert_eq!(adapter.status().expect("status").tracking, None);
}

#[test]
fn opening_a_branch_without_an_upstream_reports_none_not_zero() {
    let fixture = TestRepo::new();
    fixture.commit("base", &[("a.txt", "a\n")], 0);
    let adapter = open_like_the_window(fixture.workdir().to_str().expect("utf-8"));
    assert_eq!(adapter.status().expect("status").tracking, None);
}

#[test]
fn opening_a_linked_worktree_reads_its_own_branch() {
    let fixture = TestRepo::with_feature_branch();
    let parent = tempfile::tempdir().expect("temp dir");
    let path = linked_path(&parent, "linked");
    fixture
        .adapter()
        .add_worktree(&path, "feature", false, None)
        .expect("add worktree");

    let adapter = open_like_the_window(&path);
    let branches = adapter.branches().expect("branches");
    let head = branches.iter().find(|branch| branch.is_head).expect("a head branch");
    assert_eq!(head.name, "feature");
}

#[test]
fn opening_a_removed_worktree_folder_fails_without_panicking() {
    let fixture = TestRepo::with_feature_branch();
    let parent = tempfile::tempdir().expect("temp dir");
    let path = linked_path(&parent, "gone");
    fixture
        .adapter()
        .add_worktree(&path, "feature", false, None)
        .expect("add worktree");
    std::fs::remove_dir_all(&path).expect("remove the worktree folder");

    assert!(GitoxideAdapter::discover(&path).is_err());
    // The main worktree still lists it (git calls it prunable) instead of failing.
    let list = fixture.adapter().worktrees().expect("list after removal");
    assert!(!list.is_empty());
}

#[test]
fn opening_a_repository_under_a_non_ascii_path_reads_cleanly() {
    let parent = tempfile::tempdir().expect("temp dir");
    let folder = parent.path().join("répo-日本-ü");
    std::fs::create_dir(&folder).expect("create non-ASCII folder");
    git_in(&folder, &["init", "-q"]);
    std::fs::write(folder.join("fïle.txt"), "a\n").expect("write file");
    git_in(&folder, &["add", "-A"]);
    git_in(&folder, &["-c", "commit.gpgsign=false", "commit", "-q", "-m", "base"]);
    std::fs::write(folder.join("ñew.txt"), "b\n").expect("write untracked");

    let adapter = open_like_the_window(folder.to_str().expect("utf-8"));
    assert_eq!(adapter.log(None).expect("log").len(), 1);
    assert_eq!(adapter.status().expect("status").untracked_files.len(), 1);
}

#[test]
fn init_fails_for_a_missing_folder_and_for_an_existing_repository() {
    let parent = tempfile::tempdir().expect("temp dir");
    let missing = parent.path().join("missing");
    assert!(GitoxideAdapter::init(missing.to_str().expect("utf-8")).is_err());

    let fixture = TestRepo::new();
    assert!(GitoxideAdapter::init(fixture.workdir().to_str().expect("utf-8")).is_err());
}

#[test]
fn discover_of_a_plain_folder_is_not_found() {
    let parent = tempfile::tempdir().expect("temp dir");
    assert!(matches!(
        GitoxideAdapter::discover(parent.path().to_str().expect("utf-8")),
        Err(DomainError::RepositoryNotFound)
    ));
}

#[test]
fn unpushed_lists_commits_on_no_remote_newest_first() {
    let origin = TestRepo::new();
    origin.commit("first", &[("a.txt", "one\n")], 0);
    let dest = cloned_from(&origin);
    commit_file(dest.path(), "a.txt", "two\n", "second");
    let second = head_at(dest.path());
    commit_file(dest.path(), "a.txt", "three\n", "third");
    let third = head_at(dest.path());

    let unpushed = GitoxideAdapter::new(dest.path().to_str().expect("utf-8 clone"))
        .unpushed_commits()
        .expect("unpushed commits");

    assert_eq!(unpushed, vec![third.to_string(), second.to_string()]);
}

#[test]
fn unpushed_is_empty_in_a_repository_without_remotes() {
    let fixture = TestRepo::new();
    assert_eq!(fixture.adapter().unpushed_commits().expect("unborn, no remote"), Vec::<String>::new());

    fixture.commit("first", &[("a.txt", "one
")], 0);
    assert_eq!(fixture.adapter().unpushed_commits().expect("born, no remote"), Vec::<String>::new());
}

#[test]
fn unpushed_is_empty_on_an_unborn_branch_that_has_a_remote() {
    let fixture = TestRepo::new();
    let elsewhere = tempfile::tempdir().expect("bare remote");
    git_in(elsewhere.path(), &["init", "--bare"]);
    git_in(
        fixture.workdir(),
        &["remote", "add", "origin", elsewhere.path().to_str().expect("utf-8 remote")],
    );

    assert_eq!(fixture.adapter().unpushed_commits().expect("unborn, with remote"), Vec::<String>::new());
}

/// The full message of HEAD, as git stores it.
fn head_message(dir: &std::path::Path) -> String {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["log", "-1", "--format=%B"])
        .stdin(std::process::Stdio::null())
        .output()
        .expect("run git log");
    assert!(output.status.success(), "git log failed");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

#[test]
fn undo_moves_head_back_keeps_the_change_staged_and_returns_the_message() {
    let fixture = TestRepo::new();
    let dir = fixture.workdir();
    commit_file(dir, "a.txt", "one\n", "first");
    let first = head_at(dir);
    commit_file(dir, "a.txt", "two\n", "second\n\nbody");
    let second = head_at(dir);
    let adapter = fixture.adapter();

    let message = adapter.undo_commit(&second.to_string()).expect("undo");

    assert_eq!(message, "second\n\nbody");
    assert_eq!(head_at(dir), first);
    let status = adapter.status().expect("status");
    assert_eq!(status.staged_files, vec!["a.txt".to_string()]);
    assert!(status.unstaged_files.is_empty());
    assert_eq!(std::fs::read_to_string(dir.join("a.txt")).expect("read a.txt"), "two\n");
}

#[test]
fn undo_moves_the_branch_and_leaves_an_added_file_staged() {
    let fixture = TestRepo::new();
    let dir = fixture.workdir();
    commit_file(dir, "a.txt", "one\n", "first");
    let first = head_at(dir);
    let branch = fixture.current_branch();
    commit_file(dir, "b.txt", "new\n", "add b");
    let adapter = fixture.adapter();

    adapter.undo_commit(&head_at(dir).to_string()).expect("undo");

    assert_eq!(fixture.current_branch(), branch, "HEAD still names the branch");
    assert_eq!(head_at(dir), first, "the branch moved back");
    let status = adapter.status().expect("status");
    assert_eq!(status.staged_files, vec!["b.txt".to_string()], "the index still holds the undone change");
    assert!(status.untracked_files.is_empty());
    assert_eq!(std::fs::read_to_string(dir.join("b.txt")).expect("read b.txt"), "new\n");
}

#[test]
fn a_commit_message_round_trips_every_paragraph() {
    let fixture = TestRepo::new();
    let dir = fixture.workdir();
    commit_file(dir, "a.txt", "one\n", "Fix the parser\n\nIt dropped the last line.\n\nSee #12.");
    let adapter = fixture.adapter();

    assert_eq!(
        adapter.commit_message(&head_at(dir).to_string()),
        Ok("Fix the parser\n\nIt dropped the last line.\n\nSee #12.".to_string())
    );
    assert!(matches!(adapter.commit_message("--all"), Err(DomainError::Unknown(_))));
}

/// `main` gets "main change" and `topic` "topic change" to a.txt, both on top of "first".
/// Returns the branch HEAD is on (`main` here), checked out at its tip.
fn diverged_on_a_txt(dir: &std::path::Path) -> String {
    commit_file(dir, "a.txt", "one\n", "first");
    let main = gix::open(dir).expect("open").head_name().unwrap().expect("on a branch").shorten().to_string();
    git_in(dir, &["checkout", "-b", "topic"]);
    commit_file(dir, "a.txt", "topic\n", "topic change");
    git_in(dir, &["checkout", &main]);
    commit_file(dir, "a.txt", "main\n", "main change");
    main
}

#[test]
fn undo_is_refused_during_a_conflicted_merge_and_with_unmerged_entries() {
    let fixture = TestRepo::new();
    let dir = fixture.workdir();
    diverged_on_a_txt(dir);
    let head = head_at(dir);
    // Conflicts, so git exits non-zero and leaves the merge in progress.
    let merged = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["merge", "topic"])
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(std::process::Stdio::null())
        .output()
        .expect("run git merge");
    assert!(!merged.status.success(), "the merge conflicts");
    let merge_head = fixture.repo.git_dir().join("MERGE_HEAD");
    assert!(merge_head.exists());
    let adapter = fixture.adapter();

    assert_eq!(
        adapter.undo_commit(&head.to_string()),
        Err(DomainError::Unknown("Finish or abort the merge first.".to_string()))
    );
    assert_eq!(head_at(dir), head, "the branch tip is unchanged");
    assert!(merge_head.exists(), "the merge is still in progress");

    // Without MERGE_HEAD no operation shows, but the index still has unmerged entries.
    std::fs::remove_file(&merge_head).expect("remove MERGE_HEAD");
    assert_eq!(
        adapter.undo_commit(&head.to_string()),
        Err(DomainError::Unknown("Resolve the conflicts first.".to_string()))
    );
    assert_eq!(head_at(dir), head);
}

#[test]
fn undo_of_a_merge_commit_is_refused() {
    let fixture = TestRepo::new();
    let dir = fixture.workdir();
    let main = diverged_on_a_txt(dir);
    // Take topic's side so the merge commits without a conflict.
    git_in(dir, &["-c", "commit.gpgsign=false", "merge", "--no-ff", "-X", "theirs", "-m", "Merge topic", "topic"]);
    let merge = head_at(dir);
    assert_eq!(fixture.current_branch(), main);

    assert_eq!(
        fixture.adapter().undo_commit(&merge.to_string()),
        Err(DomainError::Unknown("HEAD is a merge commit: undo it from the terminal".to_string()))
    );
    assert_eq!(head_at(dir), merge);
}

#[test]
fn undo_of_the_first_commit_is_refused() {
    let fixture = TestRepo::new();
    let dir = fixture.workdir();
    commit_file(dir, "a.txt", "one\n", "first");
    let first = head_at(dir);

    let result = fixture.adapter().undo_commit(&first.to_string());

    assert!(
        matches!(&result, Err(DomainError::Unknown(text)) if text.contains("first commit")),
        "{result:?}"
    );
    assert_eq!(head_at(dir), first);
}

#[test]
fn undo_and_amend_refuse_when_head_is_not_the_one_expected() {
    let fixture = TestRepo::new();
    let dir = fixture.workdir();
    commit_file(dir, "a.txt", "one\n", "first");
    let first = head_at(dir);
    commit_file(dir, "a.txt", "two\n", "second");
    let second = head_at(dir);
    // A staged change an amend would take, had it run.
    std::fs::write(dir.join("b.txt"), "new\n").expect("write b.txt");
    git_in(dir, &["add", "b.txt"]);
    let adapter = fixture.adapter();
    let moved = Err(DomainError::HeadMoved {
        expected: first.to_string(),
        actual: second.to_string(),
    });

    assert_eq!(adapter.undo_commit(&first.to_string()), moved);
    assert_eq!(adapter.amend("rewritten", &first.to_string()), moved.map(|_: String| ()));

    assert_eq!(head_at(dir), second);
    assert_eq!(head_message(dir), "second");
    assert_eq!(adapter.status().expect("status").staged_files, vec!["b.txt".to_string()]);
}

#[test]
fn undo_and_amend_before_the_first_commit_refuse_without_panicking() {
    let fixture = TestRepo::new();
    let adapter = fixture.adapter();
    let moved = Err(DomainError::HeadMoved {
        expected: "abc".to_string(),
        actual: String::new(),
    });

    assert_eq!(adapter.undo_commit("abc"), moved);
    assert_eq!(adapter.amend("first", "abc"), moved.map(|_: String| ()));
}

#[test]
fn amend_rewrites_the_message_and_takes_staged_changes() {
    let fixture = TestRepo::new();
    let dir = fixture.workdir();
    commit_file(dir, "a.txt", "one\n", "first");
    let first = head_at(dir);
    commit_file(dir, "a.txt", "two\n", "second");
    let second = head_at(dir);
    std::fs::write(dir.join("b.txt"), "new\n").expect("write b.txt");
    git_in(dir, &["add", "b.txt"]);
    let adapter = fixture.adapter();

    adapter
        .amend("rewritten\n\nwith a body", &second.to_string())
        .expect("amend");

    let amended = head_at(dir);
    assert_ne!(amended, second);
    let log = adapter.log(Some(1)).expect("log");
    assert_eq!(log[0].id, amended.to_string());
    assert_eq!(log[0].parent_ids, vec![first.to_string()]);
    assert_eq!(head_message(dir), "rewritten\n\nwith a body");
    let status = adapter.status().expect("status");
    assert!(status.staged_files.is_empty(), "b.txt went into the amended commit");
    assert!(status.untracked_files.is_empty());
}

#[test]
fn amend_with_a_blank_message_does_not_move_head() {
    let fixture = TestRepo::new();
    let dir = fixture.workdir();
    commit_file(dir, "a.txt", "one\n", "first");
    let first = head_at(dir);

    let result = fixture.adapter().amend(" \n\t", &first.to_string());

    assert_eq!(
        result,
        Err(DomainError::Unknown(BLANK_COMMIT_MESSAGE.to_string()))
    );
    assert_eq!(head_at(dir), first);
}

#[test]
fn line_counts_cover_staged_unstaged_and_untracked() {
    let fixture = TestRepo::new();
    fixture.commit("initial", &[("a.txt", "a1\na2\n"), ("b.txt", "b1\nb2\n")], 0);
    fixture.stage_and_write(&[("a.txt", "a1\na2\n"), ("b.txt", "b1\nb2\n")]);
    fixture.write_file("a.txt", "a1\na2\na3\na4\n");
    git_in(fixture.workdir(), &["add", "a.txt"]);
    fixture.write_file("b.txt", "b1\nb2\nb3\n");
    fixture.write_file("new.txt", "n1\nn2\nn3\nn4\n");
    fixture.write_file("no_final_newline.txt", "x\ny");
    std::fs::write(fixture.workdir().join("blob.bin"), b"ab\0cd").expect("write binary file");
    std::fs::write(fixture.workdir().join("empty.txt"), b"").expect("write empty file");

    let counts = fixture.adapter().line_counts().expect("line counts");

    assert_eq!(counts.staged["a.txt"], Some((2, 0)));
    assert_eq!(counts.unstaged["b.txt"], Some((1, 0)));
    assert_eq!(counts.untracked["new.txt"], Some((4, 0)));
    assert_eq!(counts.untracked["no_final_newline.txt"], Some((2, 0)));
    assert_eq!(counts.untracked["blob.bin"], None);
    assert_eq!(counts.untracked["empty.txt"], Some((0, 0)));
}

#[test]
fn line_counts_of_a_repository_with_no_changes_are_empty() {
    let fixture = TestRepo::new();
    fixture.commit("initial", &[("a.txt", "a\n")], 0);
    fixture.stage_and_write(&[("a.txt", "a\n")]);

    assert_eq!(fixture.adapter().line_counts().expect("line counts"), core_domain::LineCounts::default());
}
