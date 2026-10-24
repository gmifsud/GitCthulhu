use crate::app::{GitApp, Message};
use crate::theme::DesignTokens;
use iced::widget::{button, column, container, text, text_input};
use iced::{Background, Border, Element, Length, Shadow, Theme};

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
        .on_press_maybe((!app.state.loading).then_some(Message::ChooseRepositoryFolder)),
        button(
            text("Init Repository…")
                .font(reading.font)
                .size(reading.size),
        )
        .on_press_maybe((!app.state.loading).then_some(Message::ChooseInitFolder)),
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
            (!app.state.loading && !app.clone_url.trim().is_empty())
                .then_some(Message::ChooseCloneFolder),
        ),
    ]
    .spacing(20)
    .padding(40);
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
