use crate::app::{GitApp, Message};
use crate::theme::DesignTokens;
use crate::palette::repository_name;
use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Background, Border, Color, Element, Length, Shadow, Theme};

const RECENT_WIDTH: f32 = 520.0;
const RECENT_HEIGHT: f32 = 260.0;

use super::chrome::Reading;

pub(super) fn welcome<'a>(
    app: &'a GitApp,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let mut welcome = column![
        text("GitCthulhu")
            .font(reading.font)
            .size(reading.size + 16.0)
            .style(move |_| text::Style {
                color: Some(tokens.accent_primary)
            }),
        text("Open a repository to get started.")
            .font(reading.font)
            .size(reading.size)
            .style(move |_| text::Style {
                color: Some(tokens.text_secondary)
            }),
        button(
            text("Open Repository…")
                .font(reading.font)
                .size(reading.size),
        )
        .on_press_maybe((!app.opening).then_some(Message::ChooseRepositoryFolder)),
        button(
            text("Init Repository…")
                .font(reading.font)
                .size(reading.size),
        )
        .on_press_maybe((!app.opening).then_some(Message::ChooseInitFolder)),
        text_input("https://…", &app.clone_url)
            .on_input(Message::CloneUrlChanged)
            .padding(8)
            .width(Length::Fixed(360.0)),
        button(
            text("Clone into folder…")
                .font(reading.font)
                .size(reading.size),
        )
        .on_press_maybe(
            (!app.opening && !app.clone_url.trim().is_empty())
                .then_some(Message::ChooseCloneFolder),
        ),
    ]
    .spacing(20)
    .padding(40);
    if !app.settings.recent_repositories.is_empty() {
        welcome = welcome.push(recent_list(app, tokens, reading));
    }
    if let Some(error) = &app.error {
        welcome = welcome.push(
            text(error.clone())
                .font(reading.font)
                .size(reading.size)
                .style(move |_| text::Style {
                    color: Some(tokens.accent_primary),
                }),
        );
    }

    container(welcome)
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
        .into()
}

/// Recent repositories, newest first. One click opens through the same path as Open….
fn recent_list<'a>(app: &'a GitApp, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    let mut list = column![text("Recent")
        .font(reading.font)
        .size(reading.size)
        .style(move |_| text::Style {
            color: Some(tokens.text_secondary),
        })]
    .spacing(2);
    for path in &app.settings.recent_repositories {
        let line = row![
            text(repository_name(path).to_string())
                .font(reading.font)
                .size(reading.size)
                .wrapping(Wrapping::None)
                .style(move |_| text::Style {
                    color: Some(tokens.text_primary),
                }),
            container(
                text(path.clone())
                    .font(reading.mono)
                    .size((reading.size - 2.0).max(10.0))
                    .wrapping(Wrapping::None)
                    .style(move |_| text::Style {
                        color: Some(tokens.text_muted),
                    }),
            )
            .width(Length::Fill)
            .clip(true),
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        list = list.push(
            button(line)
                .width(Length::Fill)
                .padding([4, 8])
                .on_press_maybe((!app.opening).then(|| Message::OpenRepository(path.clone())))
                .style(move |_theme, status| {
                    let hot = matches!(
                        status,
                        iced::widget::button::Status::Hovered
                            | iced::widget::button::Status::Pressed
                    );
                    iced::widget::button::Style {
                        background: Some(Background::Color(if hot {
                            tokens.bg_raised
                        } else {
                            Color::TRANSPARENT
                        })),
                        text_color: tokens.text_primary,
                        border: Border {
                            radius: 3.0.into(),
                            ..Border::default()
                        },
                        shadow: Shadow::default(),
                    }
                }),
        );
    }
    container(scrollable(list))
        .width(Length::Fixed(RECENT_WIDTH))
        .max_height(RECENT_HEIGHT)
        .into()
}
