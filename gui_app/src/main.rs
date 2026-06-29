//! gui_app
//! Main entry point for the iced desktop application.

use core_domain::{AppState, UserCommand, ViewEvent};
use git_adapter::GitoxideAdapter;
use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Color, Element, Length, Task, Theme, Border, Background, Shadow};

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

pub fn main() -> iced::Result {
    iced::application("Rust Git GUI", GitApp::update, GitApp::view)
        .theme(|_| Theme::Dark)
        .subscription(GitApp::subscription)
        .run()
}

struct GitApp {
    state: AppState,
    adapter: GitoxideAdapter,
    tokens: DesignTokens,
}

#[derive(Debug, Clone)]
enum Message {
    RefreshRepository,
    CheckoutBranch(String),
    DomainEvent(ViewEvent),
    ConfigReloaded(core_domain::AppSettings),
}

impl Default for GitApp {
    fn default() -> Self {
        Self {
            state: AppState::new(),
            adapter: GitoxideAdapter::new("."),
            tokens: DesignTokens::deep_focus(),
        }
    }
}

impl GitApp {
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::RefreshRepository => {
                self.state.loading = true;
                Task::none()
            }
            Message::CheckoutBranch(_branch_name) => {
                Task::none()
            }
            Message::DomainEvent(event) => {
                self.state.apply_event(event);
                Task::none()
            }
            Message::ConfigReloaded(settings) => {
                self.state.settings = settings;
                // e.g. update self.tokens based on settings.theme
                Task::none()
            }
        }
    }

    fn subscription(&self) -> iced::Subscription<Message> {
        // Implement the Subscription API to listen to file system events
        // via the config_adapter watcher logic.
        iced::Subscription::none() // Mocked for simplicity
    }

    fn view(&self) -> Element<Message> {
        let tokens = self.tokens;

        if let core_domain::TransportState::Degraded(ref reason) = self.state.transport_state {
            let modal_content = column![
                text("DIAGNOSTIC PRE-FLIGHT FAILED")
                    .size(24)
                    .style(move |_| text::Style { color: Some(tokens.accent_primary) }),
                text("SSH Capabilities Missing or Frozen.")
                    .size(16)
                    .style(move |_| text::Style { color: Some(tokens.text_primary) }),
                text(reason)
                    .size(14)
                    .style(move |_| text::Style { color: Some(tokens.text_secondary) }),
                button("Copy Remediation Command")
                    .on_press(Message::RefreshRepository) // Mock action
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

        // Custom style helpers replacing generic themes with strict tokens
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

        // Left Sidebar: Branches & Remotes
        let left_sidebar = container(
            column![
                text("Branches").size(16).style(move |_| text::Style { color: Some(tokens.accent_primary) }),
                button("main").on_press(Message::CheckoutBranch("main".to_string())),
                button("feature/iced-ui").on_press(Message::CheckoutBranch("feature/iced-ui".to_string())),
            ]
            .spacing(10)
            .padding(10)
        )
        .style(sidebar_style.clone())
        .width(Length::Fixed(200.0))
        .height(Length::Fill);

        // Center Pane: Commit Graph
        let mut commit_list = column![
            text("Commit Graph (Virtualized)").size(20).style(move |_| text::Style { color: Some(tokens.text_primary) }),
            button("Refresh").on_press(Message::RefreshRepository),
            text(if self.state.loading { "Loading..." } else { "Up to date." }).style(move |_| text::Style { color: Some(tokens.text_muted) }),
        ].spacing(15);
        
        let visible_nodes = self.state.dag_nodes.iter().take(50);
        for node in visible_nodes {
            let commit_info = self.state.commits.iter().find(|c| c.id == node.commit_id);
            if let Some(commit) = commit_info {
                commit_list = commit_list.push(
                    row![
                        text(format!("Lane {}", node.lane)).width(Length::Fixed(60.0)),
                        text(&commit.id[..7]).width(Length::Fixed(80.0)),
                        text(&commit.message).width(Length::Fill),
                        text(&commit.author).width(Length::Fixed(120.0)),
                    ].spacing(10)
                );
            }
        }

        let center_pane = container(
            scrollable(commit_list.padding(20))
        )
        .style(app_style)
        .width(Length::Fill)
        .height(Length::Fill);

        // Right Sidebar: Staged changes & Diffs
        let right_sidebar = container(
            column![
                text("Staged Changes").size(16).style(move |_| text::Style { color: Some(tokens.accent_primary) }),
                text("0 files ready to commit").style(move |_| text::Style { color: Some(tokens.text_muted) }),
            ]
            .spacing(10)
            .padding(10)
        )
        .style(sidebar_style)
        .width(Length::Fixed(250.0))
        .height(Length::Fill);

        let main_layout = row![
            left_sidebar,
            center_pane,
            right_sidebar,
        ];

        container(main_layout)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}
