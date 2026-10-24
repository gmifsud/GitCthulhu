use crate::app::{GitApp, Message};
use crate::theme::DesignTokens;
use iced::widget::{column, container, row, scrollable, text};
use iced::{Background, Border, Element, Length, Shadow, Theme};

use super::chrome::{command_button, Reading};

pub(super) fn terminal_pane<'a>(
    app: &'a GitApp,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let bar = row![
        text("Terminal")
            .font(reading.font)
            .size(reading.size)
            .style(move |_| text::Style {
                color: Some(tokens.accent_primary),
            }),
        command_button("Clear", Message::ClearTerminal, true, tokens, reading),
        command_button(
            if app.terminal_open { "Hide" } else { "Show" },
            Message::ToggleTerminal,
            true,
            tokens,
            reading,
        ),
    ]
    .spacing(8)
    .padding([6, 12]);

    let mut pane = column![bar].width(Length::Fill);
    if app.terminal_open {
        let mut lines = column![].spacing(8).padding([8, 12]);
        if app.terminal.entries().is_empty() {
            lines = lines.push(note("No provider output yet.", tokens, reading));
        }
        for entry in app.terminal.entries() {
            lines = lines.push(entry_view(entry, tokens, reading));
        }
        pane = pane.push(
            scrollable(lines)
                .anchor_bottom()
                .width(Length::Fill)
                .height(Length::Fill),
        );
    }

    container(pane)
        .width(Length::Fill)
        .height(if app.terminal_open {
            Length::Fixed(180.0)
        } else {
            Length::Shrink
        })
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(tokens.bg_sidebar)),
            text_color: Some(tokens.text_secondary),
            border: Border {
                color: tokens.border,
                width: 1.0,
                radius: 0.0.into(),
            },
            shadow: Shadow::default(),
        })
        .into()
}

fn entry_view<'a>(
    entry: &'a crate::terminal::TranscriptEntry,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    column![
        text(entry.source.clone())
            .font(reading.font)
            .size((reading.size - 2.0).max(11.0))
            .style(move |_| text::Style {
                color: Some(tokens.accent_primary),
            }),
        text(entry.text.clone())
            .font(reading.mono)
            .size((reading.size - 2.0).max(11.0))
            .style(move |_| text::Style {
                color: Some(tokens.text_secondary),
            }),
    ]
    .spacing(2)
    .into()
}

fn note<'a>(value: &'static str, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    text(value)
        .font(reading.font)
        .size(reading.size)
        .style(move |_| text::Style {
            color: Some(tokens.text_muted),
        })
        .into()
}
