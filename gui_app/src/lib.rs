//! gui_app — iced interface.
//! Adapters are wired by the `app` crate. This crate does not name them.

mod app;
mod changes;
mod commit_graph;
mod jobs;
mod theme;
mod tracking;
mod view;

use std::sync::Arc;

use app::{GitApp, Message};
use core_domain::{DomainError, GitRepository};
use iced::{Task, Theme};

/// Opens a repository path into the port the window already speaks.
/// Implemented by the composition root so this crate never names an adapter.
pub type RepositoryOpener =
    Arc<dyn Fn(&str) -> Result<Arc<dyn GitRepository + Send + Sync>, DomainError> + Send + Sync>;

/// Run with a live repository adapter. The repository loads as soon as the window opens.
pub fn run(repo: Box<dyn GitRepository + Send + Sync>, open: RepositoryOpener) -> iced::Result {
    iced::application("GitCthulhu", GitApp::update, GitApp::view)
        .theme(|_| Theme::Dark)
        .window(repository_window())
        .subscription(GitApp::subscription)
        .run_with(move || {
            (
                GitApp::with_repo_opener(Arc::from(repo), Arc::clone(&open)),
                Task::done(Message::RefreshRepository),
            )
        })
}

/// Run on the welcome screen. Open Repository uses `open` after the folder dialog.
pub fn run_welcome(open: RepositoryOpener) -> iced::Result {
    iced::application("GitCthulhu", GitApp::update, GitApp::view)
        .theme(|_| Theme::Dark)
        .window(repository_window())
        .subscription(GitApp::subscription)
        .run_with(move || (GitApp::welcome_with(open), Task::none()))
}

fn repository_window() -> iced::window::Settings {
    iced::window::Settings {
        size: iced::Size::new(1280.0, 800.0),
        ..Default::default()
    }
}
