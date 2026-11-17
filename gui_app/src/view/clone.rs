//! The Clone window: paste an address from GitHub, Azure DevOps or anywhere else, see what it
//! was understood as, choose where it goes, and follow git while it clones.

use iced::widget::text::Wrapping;
use iced::widget::{column, container, progress_bar, row, text, text_input, Space};
use iced::{Alignment, Background, Border, Element, Length, Shadow, Theme};

use super::chrome::{accent_button, command_button, field_style, Reading};
use crate::app::{CloneDialog, Message, CLONE_URL_ID};
use crate::theme::DesignTokens;

const WIDTH: f32 = 600.0;
const SCRIM_ALPHA: f32 = 0.72;

pub(super) fn clone_dialog<'a>(dialog: &'a CloneDialog, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    let small = (reading.size - 1.0).max(11.0);
    let label = |value: &'static str| {
        text(value).font(reading.font).size(small).style(move |_| text::Style { color: Some(tokens.text_secondary) })
    };
    let quiet = move |value: String, colour| {
        text(value).font(reading.font).size(small).wrapping(Wrapping::WordOrGlyph).style(move |_| text::Style { color: Some(colour) })
    };
    let running = dialog.running.is_some();
    let field = |placeholder: &'static str, value: &'a str, on_input: fn(String) -> Message| {
        let input = text_input(placeholder, value).font(reading.font).size(reading.size).padding([5, 8]).style(field_style(tokens));
        if running { input } else { input.on_input(on_input).on_submit(Message::StartClone) }
    };

    let mut card = column![text("Clone a repository")
        .font(reading.font)
        .size(reading.size + 3.0)
        .style(move |_| text::Style { color: Some(tokens.text_primary) })]
    .spacing(10);

    // The address, and what it was understood as.
    card = card.push(label("Address: paste it from GitHub, Azure DevOps, or the browser's address bar"));
    card = card.push(field("https://github.com/owner/repository", &dialog.input, Message::CloneInputChanged).id(text_input::Id::new(CLONE_URL_ID)));
    let found = match &dialog.source {
        Some(source) => quiet(format!("{} · {}", source.provider.name(), source.path), tokens.text_primary),
        None if dialog.input.trim().is_empty() => quiet("HTTPS or SSH, or a folder on this machine.".to_string(), tokens.text_muted),
        None => quiet("Not an address to clone.".to_string(), tokens.removed),
    };
    card = card.push(found);

    // Where it goes.
    card = card.push(label("Into the folder"));
    card = card.push(
        row![
            field(r"D:\Repos", &dialog.parent, Message::CloneParentChanged).width(Length::Fill),
            command_button("Browse…", Message::CloneBrowse, !running, tokens, reading),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    );
    card = card.push(
        row![
            column![label("Folder name"), field("repository", &dialog.name, Message::CloneNameChanged)].spacing(4).width(Length::FillPortion(3)),
            column![label("Branch (blank: the default)"), field("main", &dialog.branch, Message::CloneBranchChanged)].spacing(4).width(Length::FillPortion(2)),
        ]
        .spacing(12),
    );
    let destination = dialog.destination();
    card = card.push(match (&destination, dialog.exists) {
        (Some(path), true) => quiet(format!("{path} already exists: choose another name or folder."), tokens.removed),
        (Some(path), false) => quiet(format!("Creates {path}"), tokens.text_muted),
        (None, _) => quiet(String::new(), tokens.text_muted),
    });

    // Progress, or why it stopped.
    if let Some(run) = &dialog.running {
        let seconds = run.started.elapsed().as_secs();
        let stage = match run.percent {
            Some(percent) => format!("{} · {percent}% · {seconds} s", run.stage),
            None => format!("{} · {seconds} s", run.stage),
        };
        card = card.push(quiet(stage, tokens.text_primary));
        card = card.push(progress_bar(0.0..=100.0, f32::from(run.percent.unwrap_or(0))).height(Length::Fixed(6.0)));
        card = card.push(quiet(
            "If git needs you to sign in, its window may open behind this one.".to_string(),
            tokens.text_muted,
        ));
    } else if let Some(error) = &dialog.error {
        card = card.push(quiet(error.clone(), tokens.removed));
        if let Some(hint) = dialog.source.as_ref().and_then(|source| crate::clone_source::sign_in_hint(error, source.provider)) {
            card = card.push(quiet(hint.to_string(), tokens.text_secondary));
        }
    }

    let actions = if running {
        row![Space::with_width(Length::Fill), command_button("Cancel clone", Message::CancelClone, !dialog.cancelling(), tokens, reading)]
    } else {
        row![
            Space::with_width(Length::Fill),
            command_button("Close", Message::CancelClone, true, tokens, reading),
            accent_button("Clone", Message::StartClone, dialog.can_start(), tokens, reading),
        ]
    };
    card = card.push(actions.spacing(8).align_y(Alignment::Center));

    container(
        container(card.padding(20).width(Length::Fixed(WIDTH))).style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(tokens.bg_sidebar)),
            text_color: Some(tokens.text_primary),
            border: Border { color: tokens.border, width: 1.0, radius: 6.0.into() },
            shadow: Shadow::default(),
        }),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .style(move |_theme: &Theme| container::Style {
        background: Some(Background::Color(iced::Color { a: SCRIM_ALPHA, ..tokens.bg_app })),
        ..container::Style::default()
    })
    .into()
}
