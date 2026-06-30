//! gui_app — pure GUI library.
//! Zero dependency on any adapter crate at the source level.
//! Adapters are wired by the [[bin]] compositor in `main.rs`.

use core_domain::{AppSettings, AppState, GitRepository, TransportState, ViewEvent};
use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Background, Border, Color, Element, Length, Shadow, Task, Theme};

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

// ── Public entry points ───────────────────────────────────────────────────────

/// Run with a live repository adapter.
pub fn run(repo: Box<dyn GitRepository + Send>) -> iced::Result {
    iced::application("GitCthulhu", GitApp::update, GitApp::view)
        .theme(|_| Theme::Dark)
        .subscription(GitApp::subscription)
        .run_with(move || (GitApp::with_repo(repo), Task::none()))
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
    adapter: Option<Box<dyn GitRepository + Send>>,
    tokens: DesignTokens,
}

#[derive(Debug, Clone)]
#[allow(dead_code)] // DomainEvent/ConfigReloaded are forward scaffolding — not yet wired
enum Message {
    RefreshRepository,
    CheckoutBranch(String),
    DomainEvent(ViewEvent),
    ConfigReloaded(AppSettings),
    /// Triggered from the welcome screen. Path will be populated by a file-picker dialog.
    // TODO: [DEBT] Wire a native file-picker and replace this stub
    OpenRepository(String),
}

impl GitApp {
    fn with_repo(repo: Box<dyn GitRepository + Send>) -> Self {
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
                self.state.loading = true;
                // TODO: [DEBT] Wire spawn_blocking + GitRepository::status / log / branches
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

        let left_sidebar = container(branch_column)
            .style(sidebar_style.clone())
            .width(Length::Fixed(200.0))
            .height(Length::Fill);

        // ── Center pane: commit graph ─────────────────────────────────────────
        let mut commit_list = column![
            text("Commit Graph (Virtualized)")
                .size(20)
                .style(move |_| text::Style { color: Some(tokens.text_primary) }),
            button("Refresh").on_press(Message::RefreshRepository),
            text(if self.state.loading { "Loading..." } else { "Up to date." })
                .style(move |_| text::Style { color: Some(tokens.text_muted) }),
        ]
        .spacing(15);

        for node in self.state.dag_nodes.iter().take(50) {
            if let Some(commit) = self.state.commits.iter().find(|c| c.id == node.commit_id) {
                commit_list = commit_list.push(
                    row![
                        text(format!("Lane {}", node.lane)).width(Length::Fixed(60.0)),
                        text(&commit.id[..7]).width(Length::Fixed(80.0)),
                        text(&commit.message).width(Length::Fill),
                        text(&commit.author).width(Length::Fixed(120.0)),
                    ]
                    .spacing(10),
                );
            }
        }

        let center_pane = container(scrollable(commit_list.padding(20)))
            .style(app_style)
            .width(Length::Fill)
            .height(Length::Fill);

        // ── Right sidebar: staged changes ─────────────────────────────────────
        let right_sidebar = container(
            column![
                text("Staged Changes")
                    .size(16)
                    .style(move |_| text::Style { color: Some(tokens.accent_primary) }),
                text("0 files ready to commit")
                    .style(move |_| text::Style { color: Some(tokens.text_muted) }),
            ]
            .spacing(10)
            .padding(10),
        )
        .style(sidebar_style)
        .width(Length::Fixed(250.0))
        .height(Length::Fill);

        container(row![left_sidebar, center_pane, right_sidebar])
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}
