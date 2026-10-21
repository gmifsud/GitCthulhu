use std::sync::Arc;

use core_domain::{AppState, DomainError, ViewEvent};
use iced::Task;

use crate::jobs::{
    checkout_in_background, choose_repository_folder, edit_in_background, error_message,
    fetch_in_background, load_repository_in_background, pull_in_background, push_in_background,
    SharedRepository, WorktreeEdit,
};
use crate::theme::DesignTokens;
use crate::RepositoryOpener;

pub(crate) struct GitApp {
    pub(crate) state: AppState,
    pub(crate) adapter: Option<SharedRepository>,
    pub(crate) error: Option<String>,
    pub(crate) commit_message: String,
    pub(crate) tokens: DesignTokens,
    open_repository: RepositoryOpener,
}

#[derive(Debug, Clone)]
pub(crate) enum Message {
    RefreshRepository,
    CheckoutBranch(String),
    CommitMessageChanged(String),
    EditWorktree(WorktreeEdit),
    RepositoryLoaded(Vec<ViewEvent>),
    CheckoutFinished(Result<(), DomainError>),
    Fetch,
    FetchFinished(Result<(), DomainError>),
    Push,
    PushFinished(Result<(), DomainError>),
    Pull,
    PullFinished(Result<(), DomainError>),
    WorktreeEditFinished(WorktreeEdit, Result<(), DomainError>),
    /// Show the native folder dialog.
    ChooseRepositoryFolder,
    /// `None` means the dialog was cancelled.
    RepositoryFolderChosen(Option<String>),
    /// Replace the current adapter with the repository at this path.
    OpenRepository(String),
}

impl GitApp {
    fn new(adapter: Option<SharedRepository>, open_repository: RepositoryOpener) -> Self {
        Self {
            state: AppState::new(),
            adapter,
            error: None,
            commit_message: String::new(),
            tokens: DesignTokens::deep_focus(),
            open_repository,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_repo(repo: SharedRepository) -> Self {
        Self::with_repo_opener(repo, unavailable_opener())
    }

    pub(crate) fn with_repo_opener(
        repo: SharedRepository,
        open_repository: RepositoryOpener,
    ) -> Self {
        Self::new(Some(repo), open_repository)
    }

    #[cfg(test)]
    pub(crate) fn welcome() -> Self {
        Self::welcome_with(unavailable_opener())
    }

    pub(crate) fn welcome_with(open_repository: RepositoryOpener) -> Self {
        Self::new(None, open_repository)
    }

    pub(crate) fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::RefreshRepository => {
                let Some(repo) = self.adapter.clone() else {
                    return Task::none();
                };
                if self.state.loading {
                    return Task::none();
                }
                self.state.loading = true;
                self.error = None;
                Task::perform(
                    load_repository_in_background(repo),
                    Message::RepositoryLoaded,
                )
            }
            Message::RepositoryLoaded(events) => {
                for event in events {
                    if let ViewEvent::ErrorOccurred(error) = &event {
                        self.error.get_or_insert_with(|| error_message(error));
                    }
                    self.state.apply_event(event);
                }
                Task::none()
            }
            Message::CheckoutBranch(branch) => {
                let Some(repo) = self.adapter.clone() else {
                    return Task::none();
                };
                if self.state.loading {
                    return Task::none();
                }
                self.state.loading = true;
                self.error = None;
                Task::perform(
                    checkout_in_background(repo, branch),
                    Message::CheckoutFinished,
                )
            }
            Message::CheckoutFinished(result) => {
                self.state.loading = false;
                match result {
                    Ok(()) => Task::done(Message::RefreshRepository),
                    Err(error) => {
                        self.error = Some(error_message(&error));
                        Task::none()
                    }
                }
            }
            Message::Fetch => {
                let Some(repo) = self.adapter.clone() else {
                    return Task::none();
                };
                if self.state.loading {
                    return Task::none();
                }
                self.state.loading = true;
                self.error = None;
                Task::perform(fetch_in_background(repo), Message::FetchFinished)
            }
            Message::FetchFinished(result) => {
                self.state.loading = false;
                match result {
                    Ok(()) => Task::done(Message::RefreshRepository),
                    Err(error) => {
                        self.error = Some(error_message(&error));
                        Task::none()
                    }
                }
            }
            Message::Push => {
                let Some(repo) = self.adapter.clone() else {
                    return Task::none();
                };
                if self.state.loading {
                    return Task::none();
                }
                self.state.loading = true;
                self.error = None;
                Task::perform(push_in_background(repo), Message::PushFinished)
            }
            Message::PushFinished(result) => {
                self.state.loading = false;
                match result {
                    Ok(()) => Task::done(Message::RefreshRepository),
                    Err(error) => {
                        self.error = Some(error_message(&error));
                        Task::none()
                    }
                }
            }
            Message::Pull => {
                let Some(repo) = self.adapter.clone() else {
                    return Task::none();
                };
                if self.state.loading {
                    return Task::none();
                }
                self.state.loading = true;
                self.error = None;
                Task::perform(pull_in_background(repo), Message::PullFinished)
            }
            Message::PullFinished(result) => {
                self.state.loading = false;
                match result {
                    Ok(()) => Task::done(Message::RefreshRepository),
                    Err(error) => {
                        self.error = Some(error_message(&error));
                        Task::none()
                    }
                }
            }
            Message::CommitMessageChanged(message) => {
                self.commit_message = message;
                Task::none()
            }
            Message::EditWorktree(edit) => {
                let Some(repo) = self.adapter.clone() else {
                    return Task::none();
                };
                if self.state.loading {
                    return Task::none();
                }
                if let WorktreeEdit::Commit(message) = &edit {
                    if message.trim().is_empty() {
                        return Task::none();
                    }
                }
                self.state.loading = true;
                self.error = None;
                let report = edit.clone();
                Task::perform(edit_in_background(repo, edit), move |result| {
                    Message::WorktreeEditFinished(report.clone(), result)
                })
            }
            Message::WorktreeEditFinished(edit, result) => {
                self.state.loading = false;
                match result {
                    Ok(()) => {
                        if matches!(edit, WorktreeEdit::Commit(_)) {
                            self.commit_message.clear();
                        }
                        Task::done(Message::RefreshRepository)
                    }
                    Err(error) => {
                        self.error = Some(error_message(&error));
                        Task::none()
                    }
                }
            }
            Message::ChooseRepositoryFolder => {
                if self.state.loading {
                    return Task::none();
                }
                Task::perform(choose_repository_folder(), Message::RepositoryFolderChosen)
            }
            Message::RepositoryFolderChosen(None) => Task::none(),
            Message::RepositoryFolderChosen(Some(path)) => {
                self.update(Message::OpenRepository(path))
            }
            Message::OpenRepository(path) => {
                if self.state.loading {
                    return Task::none();
                }
                if path.trim().is_empty() {
                    self.error = Some("Choose a repository folder.".to_string());
                    return Task::none();
                }
                match (self.open_repository)(&path) {
                    Ok(repo) => {
                        let open_repository = Arc::clone(&self.open_repository);
                        *self = Self::new(Some(repo), open_repository);
                        Task::done(Message::RefreshRepository)
                    }
                    Err(error) => {
                        self.error = Some(error_message(&error));
                        Task::none()
                    }
                }
            }
        }
    }

    pub(crate) fn subscription(&self) -> iced::Subscription<Message> {
        iced::Subscription::none()
    }
}

#[cfg(test)]
fn unavailable_opener() -> RepositoryOpener {
    Arc::new(|_| Err(DomainError::Unknown("no repository opener".into())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::load_repository;
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

    #[test]
    fn repository_loaded_populates_app_state() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(populated_repo()));
        let _ = app.update(Message::RefreshRepository);
        let _ = app.update(Message::RepositoryLoaded(
            load_repository(&populated_repo()),
        ));
        assert!(!app.state.loading);
        assert!(app.state.current_status.is_some());
        assert_eq!(app.state.commits.len(), 2);
        assert_eq!(app.state.dag_nodes.len(), 2);
        assert_eq!(
            app.state.branches,
            vec![branch("main", true), branch("feature/x", false)]
        );
    }

    #[test]
    fn with_repo_opens_the_repository_view() {
        let app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        assert!(app.adapter.is_some());
    }

    #[test]
    fn welcome_has_no_repository() {
        assert!(GitApp::welcome().adapter.is_none());
    }

    #[test]
    fn open_repository_replaces_the_welcome_screen() {
        let seen = Arc::new(std::sync::Mutex::new(String::new()));
        let seen_in_opener = Arc::clone(&seen);
        let open: RepositoryOpener = Arc::new(move |path| {
            *seen_in_opener.lock().expect("path lock") = path.to_string();
            Ok(Arc::new(FakeGitRepository::ok()))
        });
        let mut app = GitApp::welcome_with(open);
        app.error = Some("previous failure".to_string());

        let _ = app.update(Message::OpenRepository(r"D:\repo".to_string()));

        assert!(app.adapter.is_some());
        assert_eq!(app.error, None);
        assert_eq!(seen.lock().expect("path lock").as_str(), r"D:\repo");
    }

    #[test]
    fn open_repository_error_stays_on_the_welcome_screen() {
        let open: RepositoryOpener =
            Arc::new(|_| Err(DomainError::Unknown("cannot open that folder".into())));
        let mut app = GitApp::welcome_with(open);

        let _ = app.update(Message::OpenRepository(r"D:\repo".to_string()));

        assert!(app.adapter.is_none());
        assert_eq!(app.error.as_deref(), Some("cannot open that folder"));
    }

    #[test]
    fn blank_repository_path_does_not_call_the_opener() {
        let open: RepositoryOpener =
            Arc::new(|_| Err(DomainError::Unknown("opener was called".into())));
        let mut app = GitApp::welcome_with(open);

        let _ = app.update(Message::OpenRepository("  ".to_string()));

        assert!(app.adapter.is_none());
        assert_eq!(app.error.as_deref(), Some("Choose a repository folder."));
    }

    #[test]
    fn cancelled_folder_dialog_leaves_the_welcome_screen() {
        let open: RepositoryOpener =
            Arc::new(|_| Err(DomainError::Unknown("opener was called".into())));
        let mut app = GitApp::welcome_with(open);

        let _ = app.update(Message::RepositoryFolderChosen(None));

        assert!(app.adapter.is_none());
        assert_eq!(app.error, None);
    }

    #[test]
    fn open_repository_is_ignored_while_another_operation_runs() {
        let open: RepositoryOpener =
            Arc::new(|_| Err(DomainError::Unknown("opener was called".into())));
        let mut app = GitApp::welcome_with(open);
        app.state.loading = true;
        app.error = Some("still showing".to_string());

        let _ = app.update(Message::OpenRepository(r"D:\repo".to_string()));

        assert!(app.adapter.is_none());
        assert_eq!(app.error.as_deref(), Some("still showing"));
    }

    #[test]
    fn refresh_marks_state_as_loading() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        let _ = app.update(Message::RefreshRepository);
        assert!(app.state.loading);
    }

    #[test]
    fn refresh_on_the_welcome_screen_does_nothing() {
        let mut app = GitApp::welcome();
        let _ = app.update(Message::RefreshRepository);
        assert!(!app.state.loading);
    }

    #[test]
    fn refresh_errors_are_shown() {
        let missing = FakeGitRepository {
            status_result: Err(DomainError::RepositoryNotFound),
            ..FakeGitRepository::ok()
        };
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        let _ = app.update(Message::RepositoryLoaded(load_repository(&missing)));
        assert_eq!(app.error.as_deref(), Some("Not a Git repository."));
    }

    #[test]
    fn checkout_on_the_welcome_screen_does_nothing() {
        let mut app = GitApp::welcome();
        let _ = app.update(Message::CheckoutBranch("main".to_string()));
        assert!(!app.state.loading);
    }

    #[test]
    fn checkout_starts_loading_and_clears_the_last_error() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.error = Some("previous failure".to_string());
        let _ = app.update(Message::CheckoutBranch("feature".to_string()));
        assert!(app.state.loading);
        assert_eq!(app.error, None);
    }

    #[test]
    fn checkout_is_ignored_while_another_operation_runs() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.state.loading = true;
        app.error = Some("still showing".to_string());
        let _ = app.update(Message::CheckoutBranch("feature".to_string()));
        assert_eq!(app.error.as_deref(), Some("still showing"));
    }

    #[test]
    fn successful_checkout_stops_loading_without_an_error() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.state.loading = true;
        let _ = app.update(Message::CheckoutFinished(Ok(())));
        assert!(!app.state.loading);
        assert_eq!(app.error, None);
    }

    #[test]
    fn failed_checkout_shows_why() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.state.loading = true;
        let reason = "error: Your local changes would be overwritten by checkout".to_string();
        let _ = app.update(Message::CheckoutFinished(Err(DomainError::Unknown(
            reason.clone(),
        ))));
        assert!(!app.state.loading);
        assert_eq!(app.error, Some(reason));
    }

    #[test]
    fn fetch_on_the_welcome_screen_does_nothing() {
        let mut app = GitApp::welcome();
        let _ = app.update(Message::Fetch);
        assert!(!app.state.loading);
    }

    #[test]
    fn fetch_starts_loading_and_clears_the_last_error() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.error = Some("previous failure".to_string());
        let _ = app.update(Message::Fetch);
        assert!(app.state.loading);
        assert_eq!(app.error, None);
    }

    #[test]
    fn fetch_is_ignored_while_another_operation_runs() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.state.loading = true;
        app.error = Some("still showing".to_string());
        let _ = app.update(Message::Fetch);
        assert_eq!(app.error.as_deref(), Some("still showing"));
    }

    #[test]
    fn successful_fetch_stops_loading_without_an_error() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.state.loading = true;
        let _ = app.update(Message::FetchFinished(Ok(())));
        assert!(!app.state.loading);
        assert_eq!(app.error, None);
    }

    #[test]
    fn failed_fetch_shows_git_message() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.state.loading = true;
        let reason = "fatal: No remote repository specified.".to_string();
        let _ = app.update(Message::FetchFinished(Err(DomainError::Unknown(
            reason.clone(),
        ))));
        assert!(!app.state.loading);
        assert_eq!(app.error, Some(reason));
    }

    #[test]
    fn push_on_the_welcome_screen_does_nothing() {
        let mut app = GitApp::welcome();
        let _ = app.update(Message::Push);
        assert!(!app.state.loading);
    }

    #[test]
    fn push_starts_loading_and_clears_the_last_error() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.error = Some("previous failure".to_string());
        let _ = app.update(Message::Push);
        assert!(app.state.loading);
        assert_eq!(app.error, None);
    }

    #[test]
    fn push_is_ignored_while_another_operation_runs() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.state.loading = true;
        app.error = Some("still showing".to_string());
        let _ = app.update(Message::Push);
        assert_eq!(app.error.as_deref(), Some("still showing"));
    }

    #[test]
    fn successful_push_stops_loading_without_an_error() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.state.loading = true;
        let _ = app.update(Message::PushFinished(Ok(())));
        assert!(!app.state.loading);
        assert_eq!(app.error, None);
    }

    #[test]
    fn failed_push_shows_git_message() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.state.loading = true;
        let reason = "fatal: The current branch main has no upstream branch.".to_string();
        let _ = app.update(Message::PushFinished(Err(DomainError::Unknown(
            reason.clone(),
        ))));
        assert!(!app.state.loading);
        assert_eq!(app.error, Some(reason));
    }

    #[test]
    fn pull_on_the_welcome_screen_does_nothing() {
        let mut app = GitApp::welcome();
        let _ = app.update(Message::Pull);
        assert!(!app.state.loading);
    }

    #[test]
    fn pull_starts_loading_and_clears_the_last_error() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.error = Some("previous failure".to_string());
        let _ = app.update(Message::Pull);
        assert!(app.state.loading);
        assert_eq!(app.error, None);
    }

    #[test]
    fn pull_is_ignored_while_another_operation_runs() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.state.loading = true;
        app.error = Some("still showing".to_string());
        let _ = app.update(Message::Pull);
        assert_eq!(app.error.as_deref(), Some("still showing"));
    }

    #[test]
    fn successful_pull_stops_loading_without_an_error() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.state.loading = true;
        let _ = app.update(Message::PullFinished(Ok(())));
        assert!(!app.state.loading);
        assert_eq!(app.error, None);
    }

    #[test]
    fn failed_pull_shows_git_message() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.state.loading = true;
        let reason = "fatal: Not possible to fast-forward, aborting.".to_string();
        let _ = app.update(Message::PullFinished(Err(DomainError::Unknown(
            reason.clone(),
        ))));
        assert!(!app.state.loading);
        assert_eq!(app.error, Some(reason));
    }

    #[test]
    fn edit_on_the_welcome_screen_does_nothing() {
        let mut app = GitApp::welcome();
        let _ = app.update(Message::EditWorktree(WorktreeEdit::Stage(vec![
            "a.txt".to_string()
        ])));
        assert!(!app.state.loading);
    }

    #[test]
    fn edit_starts_loading_and_clears_the_last_error() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.error = Some("previous failure".to_string());
        let _ = app.update(Message::EditWorktree(WorktreeEdit::Stage(vec![
            "a.txt".to_string()
        ])));
        assert!(app.state.loading);
        assert_eq!(app.error, None);
    }

    #[test]
    fn edit_is_ignored_while_another_operation_runs() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.state.loading = true;
        app.error = Some("still showing".to_string());
        let _ = app.update(Message::EditWorktree(WorktreeEdit::Unstage(vec![
            "a.txt".to_string()
        ])));
        assert_eq!(app.error.as_deref(), Some("still showing"));
    }

    #[test]
    fn whitespace_commit_message_does_not_start_an_edit() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.error = Some("previous failure".to_string());
        let _ = app.update(Message::EditWorktree(WorktreeEdit::Commit(
            "  \t".to_string(),
        )));
        assert!(!app.state.loading);
        assert_eq!(app.error.as_deref(), Some("previous failure"));
    }

    #[test]
    fn successful_commit_clears_the_message_and_stops_loading() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.state.loading = true;
        app.commit_message = "record the change".to_string();
        let _ = app.update(Message::WorktreeEditFinished(
            WorktreeEdit::Commit("record the change".to_string()),
            Ok(()),
        ));
        assert!(!app.state.loading);
        assert_eq!(app.error, None);
        assert_eq!(app.commit_message, "");
    }

    #[test]
    fn successful_stage_keeps_the_commit_message() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.state.loading = true;
        app.commit_message = "still drafting".to_string();
        let _ = app.update(Message::WorktreeEditFinished(
            WorktreeEdit::Stage(vec!["a.txt".to_string()]),
            Ok(()),
        ));
        assert!(!app.state.loading);
        assert_eq!(app.commit_message, "still drafting");
    }

    #[test]
    fn failed_edit_shows_why() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.state.loading = true;
        let reason = "error: hook declined to update".to_string();
        let _ = app.update(Message::WorktreeEditFinished(
            WorktreeEdit::Commit("record the change".to_string()),
            Err(DomainError::Unknown(reason.clone())),
        ));
        assert!(!app.state.loading);
        assert_eq!(app.error, Some(reason));
    }

    #[test]
    fn commit_message_changes_are_kept() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        let _ = app.update(Message::CommitMessageChanged(
            "record the change".to_string(),
        ));
        assert_eq!(app.commit_message, "record the change");
        assert!(!app.state.loading);
    }
}
