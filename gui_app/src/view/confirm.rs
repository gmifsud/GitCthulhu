use crate::app::{HistoryPrompt, Message, StashPrompt, STASH_MESSAGE_ID};
use crate::theme::DesignTokens;
use core_domain::ResetMode;
use iced::widget::{checkbox, column, container, pick_list, row, text, text_input};
use iced::{Alignment, Background, Border, Element, Length, Shadow, Theme};

use super::chrome::{accent_button, command_button, field_style, Reading};

pub(super) fn confirm_discard<'a>(
    paths: &[String],
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let (title, detail) = match paths {
        [] => ("Discard changes?", "No matching files.".to_string()),
        [path] => ("Discard this file?", path.clone()),
        _ => (
            "Discard these changes?",
            format!("{} files", paths.len()),
        ),
    };
    dialog(
        title,
        &detail,
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

pub(super) fn confirm_detach<'a>(
    commit_id: &str,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let short = crate::selection::short_hash(commit_id);
    dialog(
        "Detach HEAD?",
        short,
        "HEAD will point at this commit. Local changes may block the checkout or carry over.",
        false,
        "Detach",
        Message::ConfirmDetachHead,
        Message::CancelDetach,
        tokens,
        reading,
    )
}

/// `branches` are what can be merged (every branch but the current one); `current` names HEAD.
pub(super) fn history_prompt<'a>(
    prompt: &HistoryPrompt,
    branches: &[String],
    current: &str,
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
        HistoryPrompt::Merge { name, no_ff } => merge_dialog(name, *no_ff, branches, current, tokens, reading),
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
    .style(move |_theme: &Theme| scrim(tokens))
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
    .style(move |_theme: &Theme| scrim(tokens))
    .into()
}

/// The dimmed backdrop behind a dialog: the screen stays visible underneath.
fn scrim(tokens: DesignTokens) -> container::Style {
    container::Style {
        background: Some(Background::Color(iced::Color { a: SCRIM_ALPHA, ..tokens.bg_app })),
        text_color: Some(tokens.text_primary),
        border: Border::default(),
        shadow: Shadow::default(),
    }
}

const SCRIM_ALPHA: f32 = 0.72;
const FORM_WIDTH: f32 = 460.0;

/// A card centred on the scrim.
fn modal<'a>(card: iced::widget::Column<'a, Message>, tokens: DesignTokens) -> Element<'a, Message> {
    container(
        container(card.spacing(12).padding(20).width(Length::Fixed(FORM_WIDTH))).style(move |_theme: &Theme| {
            container::Style {
                background: Some(Background::Color(tokens.bg_sidebar)),
                text_color: Some(tokens.text_primary),
                border: Border { color: tokens.border, width: 1.0, radius: 6.0.into() },
                shadow: Shadow::default(),
            }
        }),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .style(move |_theme: &Theme| scrim(tokens))
    .into()
}

fn heading<'a>(value: String, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    text(value)
        .font(reading.font)
        .size(reading.size + 2.0)
        .style(move |_| text::Style { color: Some(tokens.text_primary) })
        .into()
}

fn note<'a>(value: &'static str, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    text(value)
        .font(reading.font)
        .size(reading.size)
        .style(move |_| text::Style { color: Some(tokens.text_secondary) })
        .into()
}

/// "Merge into <current>": pick the branch, choose whether to always record a merge commit.
fn merge_dialog<'a>(
    name: &str,
    no_ff: bool,
    branches: &[String],
    current: &str,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let chosen = (!name.is_empty()).then(|| name.to_string());
    let card = column![
        heading(format!("Merge into {current}"), tokens, reading),
        pick_list(branches.to_vec(), chosen, Message::MergeTargetPicked)
            .placeholder("Branch to merge")
            .font(reading.font)
            .text_size(reading.size)
            .padding([6, 8])
            .width(Length::Fill)
            .style(move |_theme: &Theme, _status| pick_list::Style {
                text_color: tokens.text_primary,
                placeholder_color: tokens.text_muted,
                handle_color: tokens.text_secondary,
                background: Background::Color(tokens.bg_app),
                border: Border { color: tokens.chip_border, width: 1.0, radius: 4.0.into() },
            }),
        checkbox("Always create a merge commit (--no-ff)", no_ff)
            .on_toggle(Message::MergeNoFfToggled)
            .font(reading.font)
            .text_size(reading.size),
        note(
            "If files conflict, the merge stays open: resolve them in the working tree, then Continue or Abort from the bar at the top.",
            tokens,
            reading,
        ),
        row![
            accent_button("Merge", Message::ConfirmMerge, !name.is_empty(), tokens, reading),
            command_button("Cancel", Message::CancelHistory, true, tokens, reading),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    ];
    modal(card, tokens)
}

/// Message, untracked option, and what is being stashed.
pub(super) fn stash_dialog<'a>(prompt: &StashPrompt, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    let scope = match prompt.paths.as_slice() {
        [] => "Every tracked change is stashed and the working tree is reset to HEAD.".to_string(),
        [path] => format!("Stashes {path} only."),
        paths => format!("Stashes the {} selected files only.", paths.len()),
    };
    let mut card = column![
        heading("Stash changes".to_string(), tokens, reading),
        text_input("Message (optional)", &prompt.message)
            .id(STASH_MESSAGE_ID)
            .on_input(Message::StashMessageChanged)
            .on_submit(Message::ConfirmStash)
            .font(reading.font)
            .size(reading.size)
            .padding(6)
            .style(field_style(tokens)),
        text(scope)
            .font(reading.font)
            .size(reading.size)
            .style(move |_| text::Style { color: Some(tokens.text_secondary) }),
    ];
    if prompt.paths.is_empty() {
        card = card.push(
            checkbox("Include untracked files", prompt.include_untracked)
                .on_toggle(Message::StashUntrackedToggled)
                .font(reading.font)
                .text_size(reading.size),
        );
    }
    card = card.push(
        row![
            accent_button("Stash", Message::ConfirmStash, true, tokens, reading),
            command_button("Cancel", Message::CancelStash, true, tokens, reading),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    );
    modal(card, tokens)
}
