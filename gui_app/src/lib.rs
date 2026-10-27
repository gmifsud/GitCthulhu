//! gui_app — iced interface.
//! Adapters are wired by the `app` crate. This crate does not name them.

mod app;
mod appearance;
mod changes;
mod commit_graph;
mod context_menu;
mod jobs;
mod layout;
mod palette;
mod reachability;
mod ref_pills;
mod selection;
mod session;
mod terminal;
mod theme;
mod tracking;
mod view;

pub use terminal::{CommitSuggestion, ModelList, ProviderExchange};

use std::sync::Arc;

use app::{GitApp, Message};
use core_domain::{AppSettings, DomainError, GitRepository};
use iced::{Task, Theme};

/// Opens a repository path into the port the window already speaks.
/// Implemented by the composition root so this crate never names an adapter.
pub type RepositoryOpener =
    Arc<dyn Fn(&str) -> Result<Arc<dyn GitRepository + Send + Sync>, DomainError> + Send + Sync>;

/// Opens a linked worktree into a tab. Unlike [`RepositoryOpener`] it must not remember the path:
/// `last_repo_path` stays the repository the user chose with Open….
pub type WorktreeOpener =
    Arc<dyn Fn(&str) -> Result<Arc<dyn GitRepository + Send + Sync>, DomainError> + Send + Sync>;

/// `git init` in a folder. The window opens the path afterwards through [`RepositoryOpener`].
pub type RepositoryInitialiser = Arc<dyn Fn(&str) -> Result<(), DomainError> + Send + Sync>;

/// `git clone` of a URL into a new directory under the parent. Returns that directory.
pub type RepositoryCloner = Arc<dyn Fn(&str, &str) -> Result<String, DomainError> + Send + Sync>;

/// Persists Appearance and the Kilo token and model. The window never sees the config store.
/// The composition root merges those fields into the settings already on disk.
pub type SettingsSaver = Arc<dyn Fn(AppSettings) + Send + Sync>;

/// Model ids from the gateway, with the reply that produced them.
/// The argument is the saved token, which may be empty.
pub type ModelLister = Arc<dyn Fn(String) -> ModelList + Send + Sync>;

/// One commit subject for a diff, with the provider reply.
/// Arguments are the token, the model id, and the diff.
pub type CommitSuggester = Arc<dyn Fn(String, String, String) -> CommitSuggestion + Send + Sync>;

/// Run with a live repository adapter. The repository loads as soon as the window opens.
#[allow(clippy::too_many_arguments)]
pub fn run(
    repo: Box<dyn GitRepository + Send + Sync>,
    repository_path: String,
    open: RepositoryOpener,
    open_worktree: WorktreeOpener,
    initialise: RepositoryInitialiser,
    clone_repository: RepositoryCloner,
    settings: AppSettings,
    save_settings: SettingsSaver,
    list_models: ModelLister,
    suggest_commit: CommitSuggester,
) -> iced::Result {
    iced::application("GitCthulhu", GitApp::update, GitApp::view)
        .theme(|_| Theme::Dark)
        .window(repository_window())
        .subscription(GitApp::subscription)
        .run_with(move || {
            (
                {
                    let mut app = GitApp::with_repo_opener(
                        Arc::from(repo),
                        Some(repository_path),
                        Arc::clone(&open),
                        Arc::clone(&open_worktree),
                        Arc::clone(&initialise),
                        Arc::clone(&clone_repository),
                        settings.clone(),
                        Arc::clone(&save_settings),
                    );
                    app.list_models = Arc::clone(&list_models);
                    app.suggest_commit = Arc::clone(&suggest_commit);
                    app
                },
                Task::done(Message::RefreshRepository),
            )
        })
}

/// Run on the welcome screen. Open Repository uses `open` after the folder dialog.
#[allow(clippy::too_many_arguments)]
pub fn run_welcome(
    open: RepositoryOpener,
    open_worktree: WorktreeOpener,
    initialise: RepositoryInitialiser,
    clone_repository: RepositoryCloner,
    settings: AppSettings,
    save_settings: SettingsSaver,
    list_models: ModelLister,
    suggest_commit: CommitSuggester,
) -> iced::Result {
    iced::application("GitCthulhu", GitApp::update, GitApp::view)
        .theme(|_| Theme::Dark)
        .window(repository_window())
        .subscription(GitApp::subscription)
        .run_with(move || {
            (
                {
                    let mut app = GitApp::welcome_with(
                        open,
                        open_worktree,
                        initialise,
                        clone_repository,
                        settings,
                        save_settings,
                    );
                    app.list_models = list_models;
                    app.suggest_commit = suggest_commit;
                    app
                },
                Task::none(),
            )
        })
}

fn repository_window() -> iced::window::Settings {
    iced::window::Settings {
        size: iced::Size::new(1280.0, 800.0),
        icon: window_icon(),
        ..Default::default()
    }
}

fn window_icon() -> Option<iced::window::Icon> {
    let rgba = include_bytes!("../assets/icon-64.rgba");
    iced::window::icon::from_rgba(rgba.to_vec(), 64, 64).ok()
}
