//! The open-repository banner: progress while a repository opens, and a failure with Retry.
//! Shown on the welcome screen and above the workspace, which stays as it was.

use iced::widget::text::Wrapping;
use iced::widget::{button, container, horizontal_space, row, text, Space};
use iced::{Alignment, Background, Border, Element, Length, Theme};

use super::chrome::Reading;
use crate::app::{GitApp, Message};
use crate::session::basename;
use crate::theme::DesignTokens;

/// "Opening name… 3 s", or `None` when nothing is opening.
pub(crate) fn progress_label(app: &GitApp) -> Option<String> {
    let progress = app.open_progress.as_ref()?;
    let seconds = progress.started.elapsed().as_secs();
    Some(format!(
        "Opening {}… {seconds} s ({})",
        basename(&progress.path),
        progress.path
    ))
}

/// The loading card over a dimmed window: what is running, and for how long. It is in the
/// middle of the window and large on purpose: the window takes no input while it shows.
pub(super) fn loading_cover<'a>(label: String, seconds: u64, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    let card = container(
        iced::widget::column![
            row![
                super::chrome::glyph(crate::icons::Icon::Refresh, reading.size + 8.0, tokens.accent_primary, &reading),
                text(label)
                    .font(reading.font)
                    .size(reading.size + 4.0)
                    .wrapping(Wrapping::WordOrGlyph)
                    .style(move |_| text::Style { color: Some(tokens.text_primary) }),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
            text(format!("{seconds} s · the window is paused until this finishes"))
                .font(reading.font)
                .size(reading.size)
                .style(move |_| text::Style { color: Some(tokens.text_secondary) }),
        ]
        .spacing(10),
    )
    .padding([20, 28])
    .max_width(560)
    .style(move |_theme: &Theme| container::Style {
        background: Some(Background::Color(tokens.bg_raised)),
        border: Border {
            color: tokens.accent_primary,
            width: 2.0,
            radius: 8.0.into(),
        },
        ..container::Style::default()
    });
    container(card)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(iced::Color { a: 0.55, ..tokens.bg_app })),
            ..container::Style::default()
        })
        .into()
}

pub(super) fn open_banner<'a>(app: &'a GitApp, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    let line = |label: String, color| {
        text(label)
            .font(reading.font)
            .size(reading.size)
            .wrapping(Wrapping::WordOrGlyph)
            .style(move |_| text::Style { color: Some(color) })
    };
    let action = |label: &'static str, message: Message| {
        button(text(label).font(reading.font).size(reading.size)).on_press(message)
    };
    let content: Element<'a, Message> = if let Some(label) = progress_label(app) {
        row![line(label, tokens.text_primary)].into()
    } else if let Some(failure) = &app.open_failure {
        row![
            line(failure.summary(), tokens.removed),
            horizontal_space(),
            action("Retry", Message::RetryOpen),
            action("Copy diagnostics", Message::CopyOpenDiagnostics),
            action("Dismiss", Message::DismissOpenFailure),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
    } else {
        return Space::new(Length::Shrink, Length::Shrink).into();
    };
    container(content)
        .width(Length::Fill)
        .padding([6, 12])
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(tokens.bg_raised)),
            border: Border {
                color: tokens.border,
                width: 1.0,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}
