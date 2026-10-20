//! gui_app — pure GUI library.
//! Zero dependency on any adapter crate: Cargo.toml lists none, so the compiler enforces it.
//! Adapters are wired by the `app` crate (the composition root).

use std::future::Future;
use std::sync::Arc;
use std::thread;

use core_domain::{AppSettings, AppState, DomainError, GitRepository, RepoStatus, TransportState, ViewEvent};
use iced::futures::channel::oneshot;
use iced::widget::text::Wrapping;
use iced::widget::{button, canvas, column, container, row, scrollable, text};
use iced::{Alignment, Background, Border, Color, Element, Length, Shadow, Task, Theme};

mod commit_graph;
use commit_graph::{graph_rows, graph_width, GraphCell, ROW_HEIGHT};

/// A repository adapter that can be shared with background loads.
type SharedRepository = Arc<dyn GitRepository + Send + Sync>;

// ── Design tokens ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct DesignTokens {
    pub bg_app: Color,
    pub bg_sidebar: Color,
    pub bg_panel: Color,
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_muted: Color,
    pub border: Color,
    pub accent_primary: Color,
    pub accent_active: Color,
}

impl DesignTokens {
    pub fn deep_focus() -> Self {
        Self {
            bg_app: hex_color(0x0D0D0F),
            bg_sidebar: hex_color(0x111214),
            bg_panel: hex_color(0x151619),
            text_primary: hex_color(0xFFFFFF),
            text_secondary: hex_color(0xD1D1D1),
            text_muted: hex_color(0x5A5A5A),
            border: hex_color(0x2D2D30),
            accent_primary: hex_color(0xF27D26),
            accent_active: hex_color(0x3B82F6),
        }
    }
}

fn hex_color(hex: u32) -> Color {
    Color::from_rgb(
        ((hex >> 16) & 0xFF) as f32 / 255.0,
        ((hex >> 8) & 0xFF) as f32 / 255.0,
        (hex & 0xFF) as f32 / 255.0,
    )
}

/// Number of commits requested from the repository and rendered in the graph pane.
const COMMIT_PAGE_SIZE: usize = 50;

// ── Changes panel ─────────────────────────────────────────────────────────────

/// One group of changed files in the changes panel.
#[derive(Debug, PartialEq)]
struct ChangeSection<'a> {
    title: &'static str,
    files: &'a [String],
}

/// The non-empty groups of `status`, in the order `git status` prints them.
fn change_sections(status: &RepoStatus) -> Vec<ChangeSection<'_>> {
    [
        ("Staged", &status.staged_files),
        ("Unstaged", &status.unstaged_files),
        ("Untracked", &status.untracked_files),
    ]
    .into_iter()
    .filter(|(_, files)| !files.is_empty())
    .map(|(title, files)| ChangeSection { title, files: files.as_slice() })
    .collect()
}

// ── Repository loading ────────────────────────────────────────────────────────

/// Read the repository through the port and translate each result into a [`ViewEvent`].
///
/// Synchronous by design (Candidate B, decision 4): the caller chooses where it runs.
/// Each call is reported independently, so one failure never hides another result.
fn load_repository(repo: &dyn GitRepository) -> Vec<ViewEvent> {
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
    ]
}

/// Run [`load_repository`] on a dedicated thread so blocking Git I/O never stalls
/// the UI or iced's executor. If the thread dies, the load is reported as an error.
fn load_repository_in_background(repo: SharedRepository) -> impl Future<Output = Vec<ViewEvent>> {
    let (sender, receiver) = oneshot::channel();
    thread::spawn(move || {
        // A dropped receiver means the app has gone away; nothing left to notify.
        let _ = sender.send(load_repository(repo.as_ref()));
    });
    async move {
        receiver.await.unwrap_or_else(|_| {
            vec![ViewEvent::ErrorOccurred(DomainError::Unknown(
                "repository load was interrupted".to_string(),
            ))]
        })
    }
}

// ── Public entry points ───────────────────────────────────────────────────────

/// Run with a live repository adapter; the repository is loaded as soon as the window opens.
pub fn run(repo: Box<dyn GitRepository + Send + Sync>) -> iced::Result {
    iced::application("GitCthulhu", GitApp::update, GitApp::view)
        .theme(|_| Theme::Dark)
        .subscription(GitApp::subscription)
        .run_with(move || (GitApp::with_repo(Arc::from(repo)), Task::done(Message::RefreshRepository)))
}

/// Run without a repository (welcome / repo picker screen).
pub fn run_welcome() -> iced::Result {
    iced::application("GitCthulhu", GitApp::update, GitApp::view)
        .theme(|_| Theme::Dark)
        .subscription(GitApp::subscription)
        .run_with(|| (GitApp::welcome(), Task::none()))
}

// ── Application state ─────────────────────────────────────────────────────────

struct GitApp {
    state: AppState,
    /// `None` = welcome / repo picker; `Some` = active repository view.
    adapter: Option<SharedRepository>,
    tokens: DesignTokens,
}

#[derive(Debug, Clone)]
#[allow(dead_code)] // DomainEvent/ConfigReloaded are forward scaffolding — not yet wired
enum Message {
    RefreshRepository,
    CheckoutBranch(String),
    DomainEvent(ViewEvent),
    /// Results of a background [`load_repository`] run.
    RepositoryLoaded(Vec<ViewEvent>),
    ConfigReloaded(AppSettings),
    /// Triggered from the welcome screen. Path will be populated by a file-picker dialog.
    // TODO: [DEBT] Wire a native file-picker and replace this stub
    OpenRepository(String),
}

impl GitApp {
    fn with_repo(repo: SharedRepository) -> Self {
        Self {
            state: AppState::new(),
            adapter: Some(repo),
            tokens: DesignTokens::deep_focus(),
        }
    }

    fn welcome() -> Self {
        Self {
            state: AppState::new(),
            adapter: None,
            tokens: DesignTokens::deep_focus(),
        }
    }
}

// ── Update ────────────────────────────────────────────────────────────────────

impl GitApp {
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::RefreshRepository => {
                // Nothing to load on the welcome screen; one load at a time otherwise.
                let Some(repo) = self.adapter.clone() else {
                    return Task::none();
                };
                if self.state.loading {
                    return Task::none();
                }
                self.state.loading = true;
                Task::perform(load_repository_in_background(repo), Message::RepositoryLoaded)
            }
            Message::RepositoryLoaded(events) => {
                for event in events {
                    self.state.apply_event(event);
                }
                Task::none()
            }
            Message::CheckoutBranch(_branch_name) => {
                // TODO: [DEBT] Wire spawn_blocking + GitRepository::checkout_branch
                Task::none()
            }
            Message::DomainEvent(event) => {
                self.state.apply_event(event);
                Task::none()
            }
            Message::ConfigReloaded(settings) => {
                self.state.settings = settings;
                Task::none()
            }
            Message::OpenRepository(_path) => {
                // TODO: [DEBT] Construct adapter from path, replace self.adapter, trigger refresh
                Task::none()
            }
        }
    }

    fn subscription(&self) -> iced::Subscription<Message> {
        // TODO: [DEBT] Wire config_adapter watcher for live settings reload
        iced::Subscription::none()
    }
}

// ── View ──────────────────────────────────────────────────────────────────────

impl GitApp {
    fn view(&self) -> Element<'_, Message> {
        let tokens = self.tokens;

        // ── Welcome screen ────────────────────────────────────────────────────
        if self.adapter.is_none() {
            let welcome = column![
                text("GitCthulhu")
                    .size(32)
                    .style(move |_| text::Style { color: Some(tokens.accent_primary) }),
                text("Open a repository to get started.")
                    .size(16)
                    .style(move |_| text::Style { color: Some(tokens.text_secondary) }),
                button("Open Repository…").on_press(Message::OpenRepository(String::new())),
            ]
            .spacing(20)
            .padding(40);

            return container(welcome)
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .style(move |_theme: &Theme| container::Style {
                    background: Some(Background::Color(tokens.bg_app)),
                    text_color: Some(tokens.text_primary),
                    border: Border::default(),
                    shadow: Shadow::default(),
                })
                .into();
        }

        // ── Degraded-transport modal ──────────────────────────────────────────
        if let TransportState::Degraded(ref reason) = self.state.transport_state {
            let modal_content = column![
                text("DIAGNOSTIC PRE-FLIGHT FAILED")
                    .size(24)
                    .style(move |_| text::Style { color: Some(tokens.accent_primary) }),
                text("SSH Capabilities Missing or Frozen.")
                    .size(16)
                    .style(move |_| text::Style { color: Some(tokens.text_primary) }),
                text(reason.as_str())
                    .size(14)
                    .style(move |_| text::Style { color: Some(tokens.text_secondary) }),
                button("Copy Remediation Command").on_press(Message::RefreshRepository),
            ]
            .spacing(20)
            .padding(40);

            return container(modal_content)
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .style(move |_theme: &Theme| container::Style {
                    background: Some(Background::Color(tokens.bg_app)),
                    text_color: Some(tokens.text_primary),
                    border: Border::default(),
                    shadow: Shadow::default(),
                })
                .into();
        }

        // ── Shared style helpers ──────────────────────────────────────────────
        let sidebar_style = move |_theme: &Theme| container::Style {
            background: Some(Background::Color(tokens.bg_sidebar)),
            text_color: Some(tokens.text_secondary),
            border: Border {
                color: tokens.border,
                width: 1.0,
                radius: 0.0.into(),
            },
            shadow: Shadow::default(),
        };

        let app_style = move |_theme: &Theme| container::Style {
            background: Some(Background::Color(tokens.bg_app)),
            text_color: Some(tokens.text_secondary),
            border: Border::default(),
            shadow: Shadow::default(),
        };

        // ── Left sidebar: dynamic branch list ────────────────────────────────
        let mut branch_column = column![
            text("Branches").size(16).style(move |_| text::Style { color: Some(tokens.accent_primary) }),
        ]
        .spacing(10)
        .padding(10);

        if self.state.branches.is_empty() {
            branch_column = branch_column.push(
                text("No branches").style(move |_| text::Style { color: Some(tokens.text_muted) }),
            );
        } else {
            for branch in &self.state.branches {
                branch_column = branch_column.push(
                    button(branch.name.as_str())
                        .on_press(Message::CheckoutBranch(branch.name.clone())),
                );
            }
        }

        let left_sidebar = container(scrollable(branch_column))
            .style(sidebar_style.clone())
            .width(Length::Fixed(200.0))
            .height(Length::Fill);

        // ── Center pane: commit graph ─────────────────────────────────────────
        // Rows have a fixed height and single-line, clipped text so graph segments join up.
        let single_line = |value: &str, width: Length| {
            container(text(value.to_string()).wrapping(Wrapping::None)).width(width).clip(true)
        };
        let shown_nodes = &self.state.dag_nodes[..self.state.dag_nodes.len().min(COMMIT_PAGE_SIZE)];
        let rows = graph_rows(shown_nodes);
        let lanes_width = Length::Fixed(graph_width(&rows));

        let mut commit_rows = column![];
        for (node, graph_row) in shown_nodes.iter().zip(rows) {
            let Some(commit) = self.state.commits.iter().find(|c| c.id == node.commit_id) else {
                continue;
            };
            commit_rows = commit_rows.push(
                row![
                    canvas(GraphCell(graph_row)).width(lanes_width).height(Length::Fixed(ROW_HEIGHT)),
                    single_line(&commit.id[..7], Length::Fixed(80.0)),
                    single_line(&commit.message, Length::Fill),
                    single_line(&commit.author, Length::Fixed(140.0)),
                ]
                .spacing(10)
                .height(Length::Fixed(ROW_HEIGHT))
                .align_y(Alignment::Center),
            );
        }

        let commit_list = column![
            text("Commit Graph (Virtualized)")
                .size(20)
                .style(move |_| text::Style { color: Some(tokens.text_primary) }),
            button("Refresh").on_press(Message::RefreshRepository),
            text(if self.state.loading { "Loading..." } else { "Up to date." })
                .style(move |_| text::Style { color: Some(tokens.text_muted) }),
            commit_rows,
        ]
        .spacing(15);

        let center_pane = container(scrollable(commit_list.padding(20)))
            .style(app_style)
            .width(Length::Fill)
            .height(Length::Fill);

        // ── Right sidebar: working-tree changes ───────────────────────────────
        let muted_note = move |note: &'static str| {
            text(note).style(move |_| text::Style { color: Some(tokens.text_muted) })
        };
        let mut changes_column = column![
            text("Changes").size(16).style(move |_| text::Style { color: Some(tokens.accent_primary) }),
        ]
        .spacing(10)
        .padding(10);

        match &self.state.current_status {
            None => changes_column = changes_column.push(muted_note("Not loaded yet")),
            Some(status) => {
                let sections = change_sections(status);
                if sections.is_empty() {
                    changes_column = changes_column.push(muted_note("Working tree clean"));
                }
                for section in sections {
                    let mut files = column![text(format!("{} ({})", section.title, section.files.len()))
                        .size(14)
                        .style(move |_| text::Style { color: Some(tokens.text_primary) })]
                    .spacing(4);
                    for file in section.files {
                        files = files.push(
                            text(file.as_str())
                                .size(13)
                                .style(move |_| text::Style { color: Some(tokens.text_secondary) }),
                        );
                    }
                    changes_column = changes_column.push(files);
                }
            }
        }

        let right_sidebar = container(scrollable(changes_column))
            .style(sidebar_style)
            .width(Length::Fixed(250.0))
            .height(Length::Fill);

        container(row![left_sidebar, center_pane, right_sidebar])
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
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
        Branch { name: name.to_string(), is_remote: false, is_head }
    }

    fn populated_repo() -> FakeGitRepository {
        FakeGitRepository {
            log_result: Ok(vec![commit("aaaaaaa1"), commit("bbbbbbb2")]),
            branches_result: Ok(vec![branch("main", true), branch("feature/x", false)]),
            ..FakeGitRepository::ok()
        }
    }

    // ── load_repository ───────────────────────────────────────────────────────

    #[test]
    fn load_repository_reports_status_log_and_branches_in_order() {
        let events = load_repository(&populated_repo());

        assert_eq!(events.len(), 3);
        assert!(matches!(&events[0], ViewEvent::StatusUpdated(_)));
        assert!(matches!(&events[1], ViewEvent::LogUpdated(c) if c.len() == 2 && c[0].id == "aaaaaaa1"));
        assert!(matches!(&events[2], ViewEvent::BranchesUpdated(b) if b.len() == 2 && b[0].is_head));
    }

    #[test]
    fn load_repository_keeps_other_results_when_one_call_fails() {
        let repo = FakeGitRepository {
            branches_result: Err(DomainError::AccessDenied),
            ..populated_repo()
        };

        let events = load_repository(&repo);

        assert!(matches!(&events[0], ViewEvent::StatusUpdated(_)));
        assert!(matches!(&events[1], ViewEvent::LogUpdated(c) if c.len() == 2));
        assert!(matches!(&events[2], ViewEvent::ErrorOccurred(DomainError::AccessDenied)));
    }

    #[test]
    fn load_repository_reports_missing_repository_for_every_call() {
        let repo = FakeGitRepository {
            status_result: Err(DomainError::RepositoryNotFound),
            log_result: Err(DomainError::RepositoryNotFound),
            branches_result: Err(DomainError::RepositoryNotFound),
            ..FakeGitRepository::ok()
        };

        let events = load_repository(&repo);

        assert_eq!(events.len(), 3);
        assert!(events
            .iter()
            .all(|e| matches!(e, ViewEvent::ErrorOccurred(DomainError::RepositoryNotFound))));
    }

    // ── change_sections ───────────────────────────────────────────────────────

    fn repo_status(staged: &[&str], unstaged: &[&str], untracked: &[&str]) -> RepoStatus {
        let owned = |files: &[&str]| files.iter().map(|f| f.to_string()).collect();
        RepoStatus { staged_files: owned(staged), unstaged_files: owned(unstaged), untracked_files: owned(untracked) }
    }

    #[test]
    fn clean_status_has_no_sections() {
        assert!(change_sections(&repo_status(&[], &[], &[])).is_empty());
    }

    #[test]
    fn sections_follow_git_status_order() {
        let status = repo_status(&["a.rs"], &["b.rs"], &["c.rs"]);

        let titles: Vec<_> = change_sections(&status).iter().map(|s| s.title).collect();

        assert_eq!(titles, vec!["Staged", "Unstaged", "Untracked"]);
    }

    #[test]
    fn empty_groups_are_left_out() {
        let status = repo_status(&["a.rs"], &[], &["c.rs", "d.rs"]);

        let sections = change_sections(&status);

        assert_eq!(
            sections,
            vec![
                ChangeSection { title: "Staged", files: &status.staged_files },
                ChangeSection { title: "Untracked", files: &status.untracked_files },
            ]
        );
    }

    // ── Background load ───────────────────────────────────────────────────────

    /// An adapter whose first call panics, standing in for a crashed Git backend.
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
        fn checkout_branch(&self, _branch_name: &str) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn fetch(&self, _store: &dyn core_domain::CredentialStore) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn fetch_ssh(&self, _ssh_manager: &dyn core_domain::SshManager) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
    }

    #[test]
    fn background_load_returns_the_same_events() {
        let events = iced::futures::executor::block_on(load_repository_in_background(Arc::new(
            populated_repo(),
        )));

        assert_eq!(events.len(), 3);
        assert!(matches!(&events[1], ViewEvent::LogUpdated(c) if c.len() == 2));
        assert!(matches!(&events[2], ViewEvent::BranchesUpdated(b) if b.len() == 2));
    }

    #[test]
    fn background_load_reports_a_crashed_adapter_as_an_error() {
        let events =
            iced::futures::executor::block_on(load_repository_in_background(Arc::new(PanickingRepository)));

        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], ViewEvent::ErrorOccurred(DomainError::Unknown(_))));
    }

    // ── GitApp ────────────────────────────────────────────────────────────────

    #[test]
    fn repository_loaded_populates_app_state() {
        let mut app = GitApp::with_repo(Arc::new(populated_repo()));
        let _ = app.update(Message::RefreshRepository);

        let _ = app.update(Message::RepositoryLoaded(load_repository(&populated_repo())));

        assert!(!app.state.loading);
        assert!(app.state.current_status.is_some());
        assert_eq!(app.state.commits.len(), 2);
        assert_eq!(app.state.dag_nodes.len(), 2);
        assert_eq!(app.state.branches, vec![branch("main", true), branch("feature/x", false)]);
    }

    #[test]
    fn with_repo_opens_the_repository_view() {
        let app = GitApp::with_repo(Arc::new(FakeGitRepository::ok()));
        assert!(app.adapter.is_some());
    }

    #[test]
    fn welcome_has_no_repository() {
        let app = GitApp::welcome();
        assert!(app.adapter.is_none());
    }

    #[test]
    fn refresh_marks_state_as_loading() {
        let mut app = GitApp::with_repo(Arc::new(FakeGitRepository::ok()));
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
    fn config_reload_replaces_settings() {
        let mut app = GitApp::welcome();
        let settings = AppSettings {
            theme: "Light".to_string(),
            repository_shortcuts: vec!["D:/Repos/GitCthulhu".to_string()],
        };

        let _ = app.update(Message::ConfigReloaded(settings.clone()));

        assert_eq!(app.state.settings, settings);
    }
}
