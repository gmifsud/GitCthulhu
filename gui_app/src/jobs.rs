use std::future::Future;
use std::sync::Arc;
use std::thread;

use core_domain::{DomainError, FileDisposition, GitRepository, Hunk, Patch, ResetMode, ViewEvent};
use iced::futures::channel::oneshot;

/// A repository adapter that can be shared with background work.
pub(crate) type SharedRepository = Arc<dyn GitRepository + Send + Sync>;

/// A diff the patch pane asked to read.
#[derive(Debug, Clone)]
pub(crate) enum PatchQuery {
    Commit(String),
    File {
        path: String,
        place: FileDisposition,
    },
}

/// A stage, unstage, or commit requested by the ribbon.
#[derive(Debug, Clone)]
pub(crate) enum WorktreeEdit {
    Stage(Vec<String>),
    Unstage(Vec<String>),
    Commit(String),
}

/// Number of commits requested from the repository and rendered in the graph pane.
pub(crate) const COMMIT_PAGE_SIZE: usize = 50;

/// Read the repository through the port and translate each result into a [`ViewEvent`].
///
/// Synchronous by design: the caller chooses where it runs.
/// Each call is reported independently, so one failure never hides another result.
pub(crate) fn load_repository(repo: &dyn GitRepository) -> Vec<ViewEvent> {
    vec![
        match repo.status() {
            Ok(status) => ViewEvent::StatusUpdated(status),
            Err(error) => ViewEvent::ErrorOccurred(error),
        },
        match repo.log(Some(COMMIT_PAGE_SIZE)) {
            Ok(commits) => ViewEvent::LogUpdated(commits),
            Err(error) => ViewEvent::ErrorOccurred(error),
        },
        match repo.branches() {
            Ok(branches) => ViewEvent::BranchesUpdated(branches),
            Err(error) => ViewEvent::ErrorOccurred(error),
        },
        match repo.stashes() {
            Ok(stashes) => ViewEvent::StashesUpdated(stashes),
            Err(error) => ViewEvent::ErrorOccurred(error),
        },
        match repo.tags() {
            Ok(tags) => ViewEvent::TagsUpdated(tags),
            Err(error) => ViewEvent::ErrorOccurred(error),
        },
    ]
}

/// Run blocking Git work on a dedicated thread so it never stalls the UI or iced's
/// executor. If the thread dies before finishing, `interrupted` supplies the result.
fn in_background<T: Send + 'static>(
    job: impl FnOnce() -> T + Send + 'static,
    interrupted: impl FnOnce() -> T,
) -> impl Future<Output = T> {
    let (sender, receiver) = oneshot::channel();
    thread::spawn(move || {
        let _ = sender.send(job());
    });
    async move { receiver.await.unwrap_or_else(|_| interrupted()) }
}

pub(crate) fn list_models_in_background(
    list: crate::ModelLister,
    token: String,
) -> impl Future<Output = crate::ModelList> {
    in_background(
        move || list(token),
        || crate::ModelList {
            ids: Err(DomainError::Unknown(
                "model list was interrupted".to_string(),
            )),
            exchange: crate::ProviderExchange::kilo("model list was interrupted"),
        },
    )
}

pub(crate) fn suggest_commit_in_background(
    repo: SharedRepository,
    files: Vec<(String, FileDisposition)>,
    token: String,
    model: String,
    suggest: crate::CommitSuggester,
) -> impl Future<Output = crate::CommitSuggestion> {
    in_background(
        move || {
            let mut diff = String::new();
            for (path, place) in files {
                if diff.len() >= SUGGEST_DIFF_LIMIT {
                    break;
                }
                match repo.file_patch(&path, place) {
                    Ok(patch) => {
                        diff.push_str(&format_patch(&patch));
                        diff.push('\n');
                    }
                    Err(error) => {
                        return crate::CommitSuggestion::local_failure(error_message(&error))
                    }
                }
            }
            if diff.trim().is_empty() {
                return crate::CommitSuggestion::local_failure(
                    "Those files have no diff to describe.",
                );
            }
            if diff.len() > SUGGEST_DIFF_LIMIT {
                diff.truncate(SUGGEST_DIFF_LIMIT);
                diff.push_str("\n[diff truncated]\n");
            }
            suggest(token, model, diff)
        },
        || crate::CommitSuggestion::local_failure("commit suggestion was interrupted"),
    )
}

const SUGGEST_DIFF_LIMIT: usize = 12_000;

fn format_patch(patch: &Patch) -> String {
    let mut text = format!("# {}\n", patch.path);
    if patch.binary {
        text.push_str("binary file\n");
        return text;
    }
    for hunk in &patch.hunks {
        text.push_str(&hunk.header);
        text.push('\n');
        for line in &hunk.lines {
            let mark = match line.kind {
                core_domain::DiffLineKind::Added => '+',
                core_domain::DiffLineKind::Removed => '-',
                core_domain::DiffLineKind::Context => ' ',
            };
            text.push(mark);
            text.push_str(&line.text);
            text.push('\n');
        }
    }
    text
}

pub(crate) fn load_repository_in_background(
    repo: SharedRepository,
) -> impl Future<Output = Vec<ViewEvent>> {
    in_background(
        move || load_repository(repo.as_ref()),
        || {
            vec![ViewEvent::ErrorOccurred(DomainError::Unknown(
                "repository load was interrupted".to_string(),
            ))]
        },
    )
}

pub(crate) fn checkout_in_background(
    repo: SharedRepository,
    branch: String,
) -> impl Future<Output = Result<(), DomainError>> {
    in_background(
        move || repo.checkout_branch(&branch),
        || Err(DomainError::Unknown("checkout was interrupted".to_string())),
    )
}

pub(crate) fn fetch_in_background(
    repo: SharedRepository,
) -> impl Future<Output = Result<(), DomainError>> {
    in_background(
        move || repo.fetch(),
        || Err(DomainError::Unknown("fetch was interrupted".to_string())),
    )
}

pub(crate) fn push_in_background(
    repo: SharedRepository,
) -> impl Future<Output = Result<(), DomainError>> {
    in_background(
        move || repo.push(),
        || Err(DomainError::Unknown("push was interrupted".to_string())),
    )
}

pub(crate) fn pull_in_background(
    repo: SharedRepository,
) -> impl Future<Output = Result<(), DomainError>> {
    in_background(
        move || repo.pull(),
        || Err(DomainError::Unknown("pull was interrupted".to_string())),
    )
}

pub(crate) fn edit_in_background(
    repo: SharedRepository,
    edit: WorktreeEdit,
) -> impl Future<Output = Result<(), DomainError>> {
    in_background(
        move || match &edit {
            WorktreeEdit::Stage(paths) => repo.stage_paths(paths),
            WorktreeEdit::Unstage(paths) => repo.unstage_paths(paths),
            WorktreeEdit::Commit(message) => repo.commit(message),
        },
        || {
            Err(DomainError::Unknown(
                "worktree edit was interrupted".to_string(),
            ))
        },
    )
}

/// Native folder picker. Cancel and a crashed dialog both yield `None`.
pub(crate) fn choose_repository_folder(
    title: &'static str,
) -> impl Future<Output = Option<String>> {
    in_background(
        move || {
            rfd::FileDialog::new()
                .set_title(title)
                .pick_folder()
                .map(|path| path.to_string_lossy().into_owned())
        },
        || None,
    )
}

pub(crate) fn open_repository_in_background(
    open: crate::RepositoryOpener,
    path: String,
) -> impl Future<Output = (String, Result<SharedRepository, DomainError>)> {
    in_background(
        move || {
            let result = open(&path);
            (path, result)
        },
        || {
            (
                String::new(),
                Err(DomainError::Unknown(
                    "repository open was interrupted".to_string(),
                )),
            )
        },
    )
}

pub(crate) fn save_settings_in_background(
    save: crate::SettingsSaver,
    settings: core_domain::AppSettings,
) -> impl Future<Output = ()> {
    in_background(
        move || {
            save(settings);
        },
        || {},
    )
}

pub(crate) fn init_in_background(
    initialise: crate::RepositoryInitialiser,
    path: String,
) -> impl Future<Output = Result<(), DomainError>> {
    in_background(
        move || initialise(&path),
        || Err(DomainError::Unknown("init was interrupted".to_string())),
    )
}

pub(crate) fn clone_in_background(
    clone_repository: crate::RepositoryCloner,
    url: String,
    parent: String,
) -> impl Future<Output = Result<String, DomainError>> {
    in_background(
        move || clone_repository(&url, &parent),
        || Err(DomainError::Unknown("clone was interrupted".to_string())),
    )
}

pub(crate) fn load_patch(
    repo: &dyn GitRepository,
    query: PatchQuery,
) -> Result<Vec<Patch>, DomainError> {
    match query {
        PatchQuery::Commit(id) => repo.commit_patch(&id),
        PatchQuery::File { path, place } => repo.file_patch(&path, place).map(|patch| vec![patch]),
    }
}

pub(crate) fn load_patch_in_background(
    repo: SharedRepository,
    query: PatchQuery,
) -> impl Future<Output = Result<Vec<Patch>, DomainError>> {
    in_background(
        move || load_patch(repo.as_ref(), query),
        || {
            Err(DomainError::Unknown(
                "patch load was interrupted".to_string(),
            ))
        },
    )
}

/// Push, apply, pop, or drop. The index is the `stash@{n}` slot from the last list.
#[derive(Debug, Clone, Copy)]
pub(crate) enum StashAction {
    Push,
    Apply(usize),
    Pop(usize),
    Drop(usize),
}

/// Create, rename, or delete one local branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BranchAction {
    Create { name: String, start: String },
    Rename { from: String, to: String },
    Delete { name: String },
}

/// Cherry-pick, reset, merge, or rebase the commit or branch the user confirmed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HistoryAction {
    CherryPick { id: String },
    Reset { id: String, mode: ResetMode },
    Merge { name: String },
    Rebase { name: String },
}

pub(crate) fn history_in_background(
    repo: SharedRepository,
    action: HistoryAction,
) -> impl Future<Output = Result<(), DomainError>> {
    in_background(
        move || match action {
            HistoryAction::CherryPick { id } => repo.cherry_pick(&id),
            HistoryAction::Reset { id, mode } => repo.reset(&id, mode),
            HistoryAction::Merge { name } => repo.merge_branch(&name),
            HistoryAction::Rebase { name } => repo.rebase_onto(&name),
        },
        || {
            Err(DomainError::Unknown(
                "history update was interrupted".to_string(),
            ))
        },
    )
}

pub(crate) fn branch_in_background(
    repo: SharedRepository,
    action: BranchAction,
) -> impl Future<Output = Result<(), DomainError>> {
    in_background(
        move || match action {
            BranchAction::Create { name, start } => repo.create_branch(&name, &start),
            BranchAction::Rename { from, to } => repo.rename_branch(&from, &to),
            BranchAction::Delete { name } => repo.delete_branch(&name),
        },
        || {
            Err(DomainError::Unknown(
                "branch update was interrupted".to_string(),
            ))
        },
    )
}

pub(crate) fn stash_in_background(
    repo: SharedRepository,
    action: StashAction,
) -> impl Future<Output = Result<(), DomainError>> {
    in_background(
        move || match action {
            StashAction::Push => repo.stash_push(""),
            StashAction::Apply(index) => repo.stash_apply(index),
            StashAction::Pop(index) => repo.stash_pop(index),
            StashAction::Drop(index) => repo.stash_drop(index),
        },
        || Err(DomainError::Unknown("stash was interrupted".to_string())),
    )
}

pub(crate) fn discard_in_background(
    repo: SharedRepository,
    path: String,
    place: FileDisposition,
) -> impl Future<Output = Result<(), DomainError>> {
    in_background(
        move || repo.discard_path(&path, place),
        || Err(DomainError::Unknown("discard was interrupted".to_string())),
    )
}

pub(crate) fn stage_hunk_in_background(
    repo: SharedRepository,
    path: String,
    hunk: Hunk,
) -> impl Future<Output = Result<(), DomainError>> {
    in_background(
        move || repo.stage_hunk(&path, &hunk),
        || {
            Err(DomainError::Unknown(
                "stage hunk was interrupted".to_string(),
            ))
        },
    )
}

pub(crate) fn discard_hunk_in_background(
    repo: SharedRepository,
    path: String,
    hunk: Hunk,
    place: FileDisposition,
) -> impl Future<Output = Result<(), DomainError>> {
    in_background(
        move || repo.discard_hunk(&path, &hunk, place),
        || {
            Err(DomainError::Unknown(
                "discard hunk was interrupted".to_string(),
            ))
        },
    )
}

pub(crate) fn error_message(error: &DomainError) -> String {
    match error {
        DomainError::RepositoryNotFound => "Not a Git repository.".to_string(),
        DomainError::AccessDenied => "Access denied.".to_string(),
        DomainError::AuthenticationFailed => "Authentication failed.".to_string(),
        DomainError::Unknown(message) => message.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_domain::{Branch, Commit, DomainError};
    use test_support::FakeGitRepository;

    fn commit(id: &str) -> Commit {
        Commit {
            id: id.to_string(),
            parent_ids: vec![],
            author: "Test Author".to_string(),
            message: format!("commit {id}"),
            timestamp: 0,
        }
    }

    fn branch(name: &str, is_head: bool) -> Branch {
        Branch {
            name: name.to_string(),
            is_remote: false,
            is_head,
        }
    }

    fn populated_repo() -> FakeGitRepository {
        FakeGitRepository {
            log_result: Ok(vec![commit("aaaaaaa1"), commit("bbbbbbb2")]),
            branches_result: Ok(vec![branch("main", true), branch("feature/x", false)]),
            ..FakeGitRepository::ok()
        }
    }

    struct PanickingRepository;

    impl GitRepository for PanickingRepository {
        fn status(&self) -> Result<core_domain::RepoStatus, DomainError> {
            panic!("simulated adapter crash")
        }
        fn log(&self, _max_count: Option<usize>) -> Result<Vec<Commit>, DomainError> {
            unreachable!("status panics first")
        }
        fn branches(&self) -> Result<Vec<Branch>, DomainError> {
            unreachable!("status panics first")
        }
        fn stashes(&self) -> Result<Vec<core_domain::Stash>, DomainError> {
            unreachable!("status panics first")
        }
        fn tags(&self) -> Result<Vec<core_domain::Tag>, DomainError> {
            unreachable!("status panics first")
        }
        fn checkout_branch(&self, _branch_name: &str) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn create_branch(&self, _name: &str, _start_point: &str) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn rename_branch(&self, _from: &str, _to: &str) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn delete_branch(&self, _name: &str) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn cherry_pick(&self, _commit_id: &str) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn reset(
            &self,
            _commit_id: &str,
            _mode: core_domain::ResetMode,
        ) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn merge_branch(&self, _name: &str) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn rebase_onto(&self, _name: &str) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn fetch(&self) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn push(&self) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn pull(&self) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn stage_paths(&self, _paths: &[String]) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn unstage_paths(&self, _paths: &[String]) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn commit(&self, _message: &str) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn commit_patch(&self, _commit_id: &str) -> Result<Vec<core_domain::Patch>, DomainError> {
            unreachable!("not used by load_repository")
        }
        fn file_patch(
            &self,
            _path: &str,
            _place: core_domain::FileDisposition,
        ) -> Result<core_domain::Patch, DomainError> {
            unreachable!("not used by load_repository")
        }
        fn discard_path(
            &self,
            _path: &str,
            _place: core_domain::FileDisposition,
        ) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn stage_hunk(&self, _path: &str, _hunk: &core_domain::Hunk) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn discard_hunk(
            &self,
            _path: &str,
            _hunk: &core_domain::Hunk,
            _place: core_domain::FileDisposition,
        ) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn stash_push(&self, _message: &str) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn stash_apply(&self, _index: usize) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn stash_pop(&self, _index: usize) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn stash_drop(&self, _index: usize) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
    }

    #[test]
    fn load_repository_reports_status_log_and_branches_in_order() {
        let events = load_repository(&populated_repo());
        assert_eq!(events.len(), 5);
        assert!(matches!(&events[0], ViewEvent::StatusUpdated(_)));
        assert!(
            matches!(&events[1], ViewEvent::LogUpdated(commits) if commits.len() == 2 && commits[0].id == "aaaaaaa1")
        );
        assert!(
            matches!(&events[2], ViewEvent::BranchesUpdated(branches) if branches.len() == 2 && branches[0].is_head)
        );
        assert!(matches!(&events[3], ViewEvent::StashesUpdated(stashes) if stashes.is_empty()));
        assert!(matches!(&events[4], ViewEvent::TagsUpdated(tags) if tags.is_empty()));
    }

    #[test]
    fn load_repository_keeps_other_results_when_one_call_fails() {
        let repo = FakeGitRepository {
            branches_result: Err(DomainError::AccessDenied),
            ..populated_repo()
        };
        let events = load_repository(&repo);
        assert!(matches!(&events[0], ViewEvent::StatusUpdated(_)));
        assert!(matches!(&events[1], ViewEvent::LogUpdated(commits) if commits.len() == 2));
        assert!(matches!(
            &events[2],
            ViewEvent::ErrorOccurred(DomainError::AccessDenied)
        ));
    }

    #[test]
    fn load_repository_reports_missing_repository_for_every_call() {
        let repo = FakeGitRepository {
            status_result: Err(DomainError::RepositoryNotFound),
            log_result: Err(DomainError::RepositoryNotFound),
            branches_result: Err(DomainError::RepositoryNotFound),
            stashes_result: Err(DomainError::RepositoryNotFound),
            tags_result: Err(DomainError::RepositoryNotFound),
            ..FakeGitRepository::ok()
        };
        let events = load_repository(&repo);
        assert!(events.iter().all(|event| matches!(
            event,
            ViewEvent::ErrorOccurred(DomainError::RepositoryNotFound)
        )));
    }

    #[test]
    fn background_load_returns_the_same_events() {
        let events = iced::futures::executor::block_on(load_repository_in_background(Arc::new(
            populated_repo(),
        )));
        assert_eq!(events.len(), 5);
        assert!(matches!(&events[1], ViewEvent::LogUpdated(commits) if commits.len() == 2));
        assert!(matches!(&events[2], ViewEvent::BranchesUpdated(branches) if branches.len() == 2));
        assert!(matches!(&events[3], ViewEvent::StashesUpdated(stashes) if stashes.is_empty()));
        assert!(matches!(&events[4], ViewEvent::TagsUpdated(tags) if tags.is_empty()));
    }

    #[test]
    fn background_load_reports_a_crashed_adapter_as_an_error() {
        let events = iced::futures::executor::block_on(load_repository_in_background(Arc::new(
            PanickingRepository,
        )));
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            ViewEvent::ErrorOccurred(DomainError::Unknown(_))
        ));
    }

    #[test]
    fn background_checkout_returns_the_adapter_result() {
        let refusing = FakeGitRepository {
            checkout_result: Err(DomainError::AccessDenied),
            ..FakeGitRepository::ok()
        };
        let ok = iced::futures::executor::block_on(checkout_in_background(
            Arc::new(FakeGitRepository::ok()),
            "feature".to_string(),
        ));
        let refused = iced::futures::executor::block_on(checkout_in_background(
            Arc::new(refusing),
            "feature".to_string(),
        ));
        assert_eq!(ok, Ok(()));
        assert_eq!(refused, Err(DomainError::AccessDenied));
    }

    #[test]
    fn background_fetch_returns_the_adapter_result() {
        let failing = FakeGitRepository {
            fetch_result: Err(DomainError::Unknown("no remote".to_string())),
            ..FakeGitRepository::ok()
        };
        let ok = iced::futures::executor::block_on(fetch_in_background(Arc::new(
            FakeGitRepository::ok(),
        )));
        let failed = iced::futures::executor::block_on(fetch_in_background(Arc::new(failing)));
        assert_eq!(ok, Ok(()));
        assert_eq!(failed, Err(DomainError::Unknown("no remote".to_string())));
    }

    #[test]
    fn background_push_returns_the_adapter_result() {
        let failing = FakeGitRepository {
            push_result: Err(DomainError::Unknown("no upstream".to_string())),
            ..FakeGitRepository::ok()
        };
        let ok = iced::futures::executor::block_on(push_in_background(Arc::new(
            FakeGitRepository::ok(),
        )));
        let failed = iced::futures::executor::block_on(push_in_background(Arc::new(failing)));
        assert_eq!(ok, Ok(()));
        assert_eq!(failed, Err(DomainError::Unknown("no upstream".to_string())));
    }

    #[test]
    fn background_pull_returns_the_adapter_result() {
        let failing = FakeGitRepository {
            pull_result: Err(DomainError::Unknown(
                "not possible to fast-forward".to_string(),
            )),
            ..FakeGitRepository::ok()
        };
        let ok = iced::futures::executor::block_on(pull_in_background(Arc::new(
            FakeGitRepository::ok(),
        )));
        let failed = iced::futures::executor::block_on(pull_in_background(Arc::new(failing)));
        assert_eq!(ok, Ok(()));
        assert_eq!(
            failed,
            Err(DomainError::Unknown(
                "not possible to fast-forward".to_string()
            ))
        );
    }

    #[test]
    fn background_open_returns_the_opener_result() {
        let seen = Arc::new(std::sync::Mutex::new(String::new()));
        let seen_in_opener = Arc::clone(&seen);
        let open: crate::RepositoryOpener = Arc::new(move |path| {
            *seen_in_opener.lock().expect("path lock") = path.to_string();
            Ok(Arc::new(FakeGitRepository::ok()) as SharedRepository)
        });
        let (path, result) = iced::futures::executor::block_on(open_repository_in_background(
            open,
            r"D:\repo".to_string(),
        ));
        assert_eq!(path, r"D:\repo");
        assert!(result.is_ok());
        assert_eq!(seen.lock().expect("path lock").as_str(), r"D:\repo");
    }

    #[test]
    fn background_edit_returns_the_adapter_result() {
        let declining = FakeGitRepository {
            commit_result: Err(DomainError::Unknown("hook declined".to_string())),
            ..FakeGitRepository::ok()
        };
        let ok = iced::futures::executor::block_on(edit_in_background(
            Arc::new(FakeGitRepository::ok()),
            WorktreeEdit::Commit("record the change".to_string()),
        ));
        let declined = iced::futures::executor::block_on(edit_in_background(
            Arc::new(declining),
            WorktreeEdit::Commit("record the change".to_string()),
        ));
        assert_eq!(ok, Ok(()));
        assert_eq!(
            declined,
            Err(DomainError::Unknown("hook declined".to_string()))
        );
    }
}
