use crate::app::{HistoryPrompt, Message};
use crate::theme::DesignTokens;
use core_domain::ResetMode;
use iced::widget::{column, container, row, text};
use iced::{Alignment, Background, Border, Element, Length, Shadow, Theme};

use super::chrome::{accent_button, command_button, Reading};

pub(super) fn confirm_discard<'a>(
    path: &str,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    dialog(
        "Discard this file?",
        path,
        "A tracked file is restored to HEAD. An untracked file is deleted.",
        true,
        "Discard",
        Message::ConfirmDiscard,
        Message::CancelDiscard,
        tokens,
        reading,
    )
}

pub(super) fn confirm_drop_stash<'a>(
    message: &str,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    dialog(
        "Drop this stash?",
        message,
        "The saved change is discarded. The worktree is left as it is.",
        false,
        "Drop",
        Message::ConfirmDropStash,
        Message::CancelDropStash,
        tokens,
        reading,
    )
}

pub(super) fn history_prompt<'a>(
    prompt: &HistoryPrompt,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    match prompt {
        HistoryPrompt::CherryPick { id } => dialog(
            "Cherry-pick this commit?",
            id,
            "The commit is applied onto the current branch.",
            true,
            "Cherry-pick",
            Message::ConfirmCherryPick,
            Message::CancelHistory,
            tokens,
            reading,
        ),
        HistoryPrompt::Reset { id } => reset_dialog(id, tokens, reading),
        HistoryPrompt::Merge { name } => dialog(
            "Merge this branch?",
            name,
            "Records a merge commit, even when a fast-forward is possible.",
            false,
            "Merge",
            Message::ConfirmMerge,
            Message::CancelHistory,
            tokens,
            reading,
        ),
        HistoryPrompt::Rebase { name } => dialog(
            "Rebase onto this branch?",
            name,
            "The current branch is rewritten onto this tip. Commits that are not already on it are replayed.",
            false,
            "Rebase",
            Message::ConfirmRebase,
            Message::CancelHistory,
            tokens,
            reading,
        ),
    }
}

fn reset_dialog<'a>(id: &str, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    let card = column![
        text("Reset to this commit?")
            .font(reading.font)
            .size(reading.size + 2.0)
            .style(move |_| text::Style {
                color: Some(tokens.text_primary),
            }),
        text(id.to_string())
            .font(reading.mono)
            .size(reading.size)
            .style(move |_| text::Style {
                color: Some(tokens.accent_primary),
            }),
        text("Soft keeps the index and the worktree. Mixed keeps the worktree and clears the index. Hard discards the worktree.")
            .font(reading.font)
            .size(reading.size)
            .style(move |_| text::Style {
                color: Some(tokens.text_secondary),
            }),
        row![
            command_button(
                "Soft",
                Message::ConfirmReset(ResetMode::Soft),
                true,
                tokens,
                reading,
            ),
            command_button(
                "Mixed",
                Message::ConfirmReset(ResetMode::Mixed),
                true,
                tokens,
                reading,
            ),
            accent_button(
                "Hard",
                Message::ConfirmReset(ResetMode::Hard),
                true,
                tokens,
                reading,
            ),
            command_button("Cancel", Message::CancelHistory, true, tokens, reading),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    ]
    .spacing(12)
    .padding(20)
    .width(Length::Fixed(520.0));

    container(
        container(card).style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(tokens.bg_sidebar)),
            text_color: Some(tokens.text_primary),
            border: Border {
                color: tokens.border,
                width: 1.0,
                radius: 6.0.into(),
            },
            shadow: Shadow::default(),
        }),
    )
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

#[allow(clippy::too_many_arguments)]
fn dialog<'a>(
    title: &'static str,
    detail: &str,
    body: &'static str,
    detail_mono: bool,
    confirm_label: &'static str,
    confirm: Message,
    cancel: Message,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let detail_font = if detail_mono {
        reading.mono
    } else {
        reading.font
    };
    let card = column![
        text(title)
            .font(reading.font)
            .size(reading.size + 2.0)
            .style(move |_| text::Style {
                color: Some(tokens.text_primary),
            }),
        text(detail.to_string())
            .font(detail_font)
            .size(reading.size)
            .style(move |_| text::Style {
                color: Some(tokens.accent_primary),
            }),
        text(body)
            .font(reading.font)
            .size(reading.size)
            .style(move |_| text::Style {
                color: Some(tokens.text_secondary),
            }),
        row![
            accent_button(confirm_label, confirm, true, tokens, reading),
            command_button("Cancel", cancel, true, tokens, reading),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    ]
    .spacing(12)
    .padding(20)
    .width(Length::Fixed(420.0));

    container(
        container(card).style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(tokens.bg_sidebar)),
            text_color: Some(tokens.text_primary),
            border: Border {
                color: tokens.border,
                width: 1.0,
                radius: 6.0.into(),
            },
            shadow: Shadow::default(),
        }),
    )
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
