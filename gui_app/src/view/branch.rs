use crate::app::{BranchPrompt, Message};
use crate::theme::DesignTokens;
use iced::widget::{checkbox, column, container, row, text, text_input};
use iced::{Alignment, Background, Border, Element, Length, Shadow, Theme};

use super::chrome::{accent_button, command_button, field_style, Reading};

pub(super) fn branch_prompt<'a>(
    prompt: &'a BranchPrompt,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let (title, body, confirm_label, confirm_enabled, detail, field) = match prompt {
        BranchPrompt::Create { start, name, .. } => (
            "New branch",
            "Created at this start point.",
            "Create",
            !name.trim().is_empty(),
            start.as_str(),
            Some(name.as_str()),
        ),
        BranchPrompt::Rename { from, to } => (
            "Rename branch",
            "Renaming the current branch moves HEAD with it.",
            "Rename",
            !to.trim().is_empty() && to.trim() != from,
            from.as_str(),
            Some(to.as_str()),
        ),
        BranchPrompt::Delete { name } => (
            "Delete this branch?",
            "A merged branch is deleted. The current branch is left as it is.",
            "Delete",
            true,
            name.as_str(),
            None,
        ),
    };

    let mut card = column![
        text(title)
            .font(reading.font)
            .size(reading.size + 2.0)
            .style(move |_| text::Style {
                color: Some(tokens.text_primary),
            }),
        text(detail.to_string())
            .font(reading.mono)
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
    ]
    .spacing(12)
    .padding(20)
    .width(Length::Fixed(420.0));

    if let Some(field) = field {
        card = card.push(
            text_input("Branch name", field)
                .on_input(Message::BranchNameChanged)
                .on_submit(Message::ConfirmBranch)
                .padding(6)
                .width(Length::Fill)
                .style(field_style(tokens)),
        );
    }
    if let BranchPrompt::Create { checkout, .. } = prompt {
        card = card.push(
            checkbox("Checkout branch", *checkout)
                .on_toggle(Message::BranchCheckoutToggled)
                .size(reading.size)
                .text_size(reading.size)
                .font(reading.font),
        );
    }
    card = card.push(
        row![
            accent_button(
                confirm_label,
                Message::ConfirmBranch,
                confirm_enabled,
                tokens,
                reading,
            ),
            command_button("Cancel", Message::CancelBranch, true, tokens, reading),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    );

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
