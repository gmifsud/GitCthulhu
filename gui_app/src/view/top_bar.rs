use crate::app::{GitApp, Message};
use crate::selection::Selection;
use crate::theme::DesignTokens;
use iced::widget::text::Wrapping;
use iced::widget::{container, horizontal_space, row, text, text_input};
use iced::{Alignment, Background, Border, Color, Element, Length, Shadow, Theme};

use super::chrome::{
    branch_chip, embedded_field_style, glyph, quiet_button, Reading, SEARCH_MARK,
};
use super::commands::{can_commit, can_stash, command_buttons, placed_commands, CommandPlace};
use super::refs::current_branch_name;

const TOP_BAR_HEIGHT: f32 = 40.0;
const TOP_BAR_PADDING: f32 = 12.0;
const SEARCH_WIDTH: f32 = 280.0;
const CLUSTER_SPACING: f32 = 8.0;
const COMMAND_SPACING: f32 = 6.0;

const APP_NAME: &str = "GitCthulhu";
const SEARCH_PLACEHOLDER: &str = "Search commits, refs, files…";
const SEARCH_SHORTCUT: &str = "Ctrl K";
const OPEN_REPOSITORY: &str = "Open…";
const SETTINGS: &str = "Settings";
const MARK_HEIGHT: f32 = 11.0;
const MARK_THICKNESS: f32 = 2.0;
const MARK_GAP: f32 = 1.5;
const SEG_RADIUS: f32 = 4.0;

pub(super) fn top_bar<'a>(
    app: &'a GitApp,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let branch_name = current_branch_name(&app.session().state.branches);
    let branch_selection = Selection::Branch {
        name: branch_name.clone(),
        current: true,
    };
    let summary = crate::tracking::tracking_summary(app.session().state.current_status.as_ref());
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
    left = left.push(repository_path(&app.session().path, tokens, reading));
    left = left.push(branch_chip(
        branch_name,
        Message::ToggleBranchList,
        !app.session().state.loading,
        app.branch_list_open,
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
    right = right.push(layout_segment(app.layout, tokens));
    right = right.push(quiet_button(
        OPEN_REPOSITORY,
        Message::ChooseRepositoryFolder,
        !app.session().state.loading,
        tokens,
        reading,
    ));
    right = right.push(quiet_button(
        SETTINGS,
        Message::OpenSettings,
        true,
        tokens,
        reading,
    ));

    let bar = row![left, search_field(tokens, reading), right,]
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

fn search_field<'a>(tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    container(
        row![
            glyph(
                SEARCH_MARK,
                reading.size,
                tokens.text_secondary,
                &reading,
            ),
            text_input(SEARCH_PLACEHOLDER, "")
                .padding(0)
                .width(Length::Fill)
                .style(embedded_field_style(tokens)),
            container(
                text(SEARCH_SHORTCUT)
                    .font(reading.mono)
                    .size((reading.size - 2.0).max(10.0))
                    .style(move |_| text::Style {
                        color: Some(tokens.text_muted),
                    }),
            )
            .padding([1, 5])
            .style(move |_theme: &Theme| container::Style {
                background: Some(Background::Color(Color::TRANSPARENT)),
                text_color: Some(tokens.text_muted),
                border: Border {
                    color: tokens.chip_border,
                    width: 1.0,
                    radius: 3.0.into(),
                },
                shadow: Shadow::default(),
            }),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding([4, 10])
    .width(Length::Fixed(SEARCH_WIDTH))
    .style(move |_theme: &Theme| container::Style {
        background: Some(Background::Color(tokens.bg_app)),
        text_color: Some(tokens.text_secondary),
        border: Border {
            color: tokens.chip_border,
            width: 1.0,
            radius: SEG_RADIUS.into(),
        },
        shadow: Shadow::default(),
    })
    .into()
}

fn layout_segment<'a>(
    current: crate::layout::LayoutPreset,
    tokens: DesignTokens,
) -> Element<'a, Message> {
    container(
        row![
            layout_button(crate::layout::LayoutPreset::Columns, current, tokens),
            layout_button(
                crate::layout::LayoutPreset::InspectorOverDiff,
                current,
                tokens,
            ),
            layout_button(crate::layout::LayoutPreset::Stacked, current, tokens),
        ]
        .spacing(0),
    )
    .style(move |_theme: &Theme| container::Style {
        background: Some(Background::Color(tokens.chip)),
        text_color: Some(tokens.text_primary),
        border: Border {
            color: tokens.chip_border,
            width: 1.0,
            radius: SEG_RADIUS.into(),
        },
        shadow: Shadow::default(),
    })
    .into()
}

fn layout_button<'a>(
    preset: crate::layout::LayoutPreset,
    current: crate::layout::LayoutPreset,
    tokens: DesignTokens,
) -> Element<'a, Message> {
    let active = preset == current;
    let ink = if active {
        Color::BLACK
    } else {
        tokens.text_primary
    };
    iced::widget::button(layout_mark(preset, ink))
        .padding([4, 6])
        .on_press(Message::SetLayout(preset))
        .style(move |_theme, status| {
            let hot = matches!(
                status,
                iced::widget::button::Status::Hovered | iced::widget::button::Status::Pressed
            );
            iced::widget::button::Style {
                background: Some(Background::Color(if active {
                    tokens.accent_primary
                } else if hot {
                    tokens.bg_raised
                } else {
                    tokens.chip
                })),
                text_color: ink,
                border: Border {
                    color: if active {
                        tokens.accent_primary
                    } else {
                        tokens.chip_border
                    },
                    width: 1.0,
                    radius: 0.0.into(),
                },
                shadow: Shadow::default(),
            }
        })
        .into()
}

fn layout_mark(preset: crate::layout::LayoutPreset, ink: Color) -> Element<'static, Message> {
    match preset {
        crate::layout::LayoutPreset::Columns => row![
            mark_bar(MARK_THICKNESS, MARK_HEIGHT, ink),
            mark_bar(MARK_THICKNESS, MARK_HEIGHT, ink),
            mark_bar(MARK_THICKNESS, MARK_HEIGHT, ink),
            mark_bar(MARK_THICKNESS, MARK_HEIGHT, ink),
        ]
        .spacing(MARK_GAP)
        .align_y(Alignment::Center)
        .into(),
        crate::layout::LayoutPreset::InspectorOverDiff => row![
            mark_bar(MARK_THICKNESS, MARK_HEIGHT, ink),
            iced::widget::column![mark_bar(5.0, 4.0, ink), mark_bar(5.0, 5.0, ink),]
                .spacing(MARK_GAP),
            mark_bar(MARK_THICKNESS, MARK_HEIGHT, ink),
        ]
        .spacing(MARK_GAP)
        .align_y(Alignment::Center)
        .into(),
        crate::layout::LayoutPreset::Stacked => iced::widget::column![
            mark_bar(MARK_HEIGHT, MARK_THICKNESS, ink),
            mark_bar(MARK_HEIGHT, MARK_THICKNESS, ink),
            mark_bar(MARK_HEIGHT, MARK_THICKNESS, ink),
            mark_bar(MARK_HEIGHT, MARK_THICKNESS, ink),
        ]
        .spacing(MARK_GAP)
        .into(),
    }
}

fn mark_bar(width: f32, height: f32, ink: Color) -> Element<'static, Message> {
    iced::widget::container(iced::widget::column![])
        .width(Length::Fixed(width))
        .height(Length::Fixed(height))
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(ink)),
            text_color: None,
            border: Border::default(),
            shadow: Shadow::default(),
        })
        .into()
}
