use crate::app::{GitApp, Message};
use crate::selection::Selection;
use crate::theme::DesignTokens;
use iced::widget::text::Wrapping;
use iced::widget::{container, horizontal_space, row, text, text_input};
use iced::{Alignment, Background, Border, Element, Length, Shadow, Theme};

use super::chrome::{accent_button, command_button, symbol_reading, Reading};
use super::commands::{can_commit, can_stash, command_buttons, placed_commands, CommandPlace};
use super::refs::current_branch_name;

const TOP_BAR_HEIGHT: f32 = 40.0;
const TOP_BAR_PADDING: f32 = 12.0;
const SEARCH_WIDTH: f32 = 280.0;
const CLUSTER_SPACING: f32 = 8.0;
const COMMAND_SPACING: f32 = 6.0;

const APP_NAME: &str = "GitCthulhu";
const SEARCH_PLACEHOLDER: &str = "Search commits, refs, files…";
const OPEN_REPOSITORY: &str = "Open…";
const SETTINGS: &str = "Settings";
const BRANCH_MENU_MARK: &str = "▾";
const LAYOUT_COLUMNS: &str = "▯▯▯";
const LAYOUT_INSPECTOR_OVER_DIFF: &str = "▯⊟▯";
const LAYOUT_STACKED: &str = "⊟";

pub(super) fn top_bar<'a>(
    app: &'a GitApp,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let branch_name = current_branch_name(&app.state.branches);
    let branch_selection = Selection::Branch {
        name: branch_name.clone(),
        current: true,
    };
    let summary = crate::tracking::tracking_summary(app.state.current_status.as_ref());
    let top_commands = placed_commands(
        &branch_selection,
        can_commit(app),
        can_stash(app),
        CommandPlace::TopBar,
    );

    let mut left = row![mark(tokens, reading)]
        .spacing(CLUSTER_SPACING)
        .align_y(Alignment::Center)
        .width(Length::Fill);
    if let Some(path) = &app.repository_path {
        left = left.push(repository_path(path, tokens, reading));
    }
    left = left.push(branch_chip(
        &branch_name,
        app.state.loading,
        tokens,
        reading,
    ));
    left = left.push(tracking(summary, tokens, reading));

    let mut right = row![horizontal_space()]
        .spacing(COMMAND_SPACING)
        .align_y(Alignment::Center)
        .width(Length::Fill);
    for button in command_buttons(&top_commands, &branch_selection, app, tokens, reading) {
        right = right.push(button);
    }
    right = right.push(layout_segment(app.layout, tokens, reading));
    right = right.push(command_button(
        OPEN_REPOSITORY,
        Message::ChooseRepositoryFolder,
        !app.state.loading,
        tokens,
        reading,
    ));
    right = right.push(command_button(
        SETTINGS,
        Message::OpenSettings,
        true,
        tokens,
        reading,
    ));

    let bar = row![
        left,
        text_input(SEARCH_PLACEHOLDER, "")
            .padding(4)
            .width(Length::Fixed(SEARCH_WIDTH)),
        right,
    ]
    .spacing(CLUSTER_SPACING)
    .align_y(Alignment::Center)
    .width(Length::Fill)
    .height(Length::Fixed(TOP_BAR_HEIGHT));

    container(bar)
        .width(Length::Fill)
        .height(Length::Fixed(TOP_BAR_HEIGHT))
        .padding([0.0, TOP_BAR_PADDING])
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(tokens.bg_chrome)),
            text_color: Some(tokens.text_primary),
            border: Border::default(),
            shadow: Shadow::default(),
        })
        .into()
}

fn mark<'a>(tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    text(APP_NAME)
        .font(reading.font)
        .size(reading.size)
        .style(move |_| text::Style {
            color: Some(tokens.text_primary),
        })
        .into()
}

fn repository_path<'a>(path: &str, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    container(
        text(path.to_string())
            .font(reading.mono)
            .size(reading.size)
            .wrapping(Wrapping::None)
            .style(move |_| text::Style {
                color: Some(tokens.text_secondary),
            }),
    )
    .width(Length::Fill)
    .clip(true)
    .into()
}

fn branch_chip<'a>(
    name: &str,
    busy: bool,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    accent_button(
        format!("{name} {BRANCH_MENU_MARK}"),
        Message::ToggleBranchList,
        !busy,
        tokens,
        reading,
    )
}

fn tracking<'a>(summary: String, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    text(summary)
        .font(reading.font)
        .size(reading.size)
        .wrapping(Wrapping::None)
        .style(move |_| text::Style {
            color: Some(tokens.text_secondary),
        })
        .into()
}

fn layout_segment<'a>(
    current: crate::layout::LayoutPreset,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    row![
        layout_button(
            LAYOUT_COLUMNS,
            crate::layout::LayoutPreset::Columns,
            current,
            tokens,
            reading,
        ),
        layout_button(
            LAYOUT_INSPECTOR_OVER_DIFF,
            crate::layout::LayoutPreset::InspectorOverDiff,
            current,
            tokens,
            reading,
        ),
        layout_button(
            LAYOUT_STACKED,
            crate::layout::LayoutPreset::Stacked,
            current,
            tokens,
            reading,
        ),
    ]
    .spacing(0)
    .into()
}

fn layout_button<'a>(
    label: &'static str,
    preset: crate::layout::LayoutPreset,
    current: crate::layout::LayoutPreset,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let reading = symbol_reading(&reading);
    if preset == current {
        accent_button(label, Message::SetLayout(preset), true, tokens, reading)
    } else {
        command_button(label, Message::SetLayout(preset), true, tokens, reading)
    }
}
