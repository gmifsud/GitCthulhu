use crate::app::{GitApp, Message};
use crate::layout::{LayoutPreset, WorkspacePane};
use crate::selection::{context_label, Selection};
use crate::theme::DesignTokens;
use iced::widget::pane_grid::{self, Content};
use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Alignment, Background, Border, Element, Length, Shadow, Theme};

use super::changes::changes_pane;
use super::chrome::{command_button, hairline, panel_style, symbol_reading, Reading};
use super::commands::{can_commit, can_stash, command_buttons, placed_commands, CommandPlace};
use super::history::history_pane;
use super::patch::patch_pane;
use super::refs::branch_switcher;

const PANE_SPACING: f32 = 4.0;
const SPLIT_HIT: f32 = 8.0;
const HEAD_PADDING_Y: f32 = 4.0;
const HEAD_PADDING_X: f32 = 8.0;
const COMMAND_SPACING: f32 = 6.0;
const FILES_RAIL_WIDTH: f32 = 36.0;
const CHIP_PADDING_Y: f32 = 3.0;
const CHIP_PADDING_X: f32 = 9.0;
const CHIP_RADIUS: f32 = 4.0;
const FILES_TITLE: &str = "Files";
const COLLAPSE_FILES: &str = "⟩";
const EXPAND_FILES: &str = "⟨";

pub(super) fn workspace<'a>(
    app: &'a GitApp,
    selection: &Selection,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let grid = pane_grid::PaneGrid::new(&app.panes, move |_pane, kind, _maximised| {
        let body = match kind {
            WorkspacePane::Graph => graph(app, selection, app.layout, tokens, reading),
            WorkspacePane::Inspector => inspector(app, selection, app.layout, tokens, reading),
            WorkspacePane::Diff => diff(app, selection, app.layout, tokens, reading),
            WorkspacePane::Files => files_rail(app, app.layout, tokens, reading),
        };
        let ruled = matches!(kind, WorkspacePane::Inspector | WorkspacePane::Files);
        Content::new(body).style(move |_theme| panel_style(tokens, ruled))
    })
    .spacing(PANE_SPACING)
    .on_resize(SPLIT_HIT, Message::PaneResized)
    .width(Length::Fill)
    .height(Length::Fill);

    if app.files_rail_open {
        return grid.into();
    }

    let rail = collapsed_files_rail(app.layout, tokens, reading);
    match app.layout {
        LayoutPreset::Stacked => column![grid, rail]
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
        LayoutPreset::Columns | LayoutPreset::InspectorOverDiff => row![grid, rail]
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
    }
}

fn graph<'a>(
    app: &'a GitApp,
    selection: &Selection,
    layout: LayoutPreset,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let _ = layout;
    let mut body = column![].width(Length::Fill).height(Length::Fill);
    let commands = placed_commands(
        selection,
        can_commit(app),
        can_stash(app),
        CommandPlace::Graph,
    );
    if !commands.is_empty() {
        body = body.push(command_head(&commands, selection, app, tokens, reading));
    }
    body = body.push(branch_switcher(app, selection, tokens, reading));
    body = body.push(history_pane(app, selection, tokens, reading));
    body.into()
}

fn inspector<'a>(
    app: &'a GitApp,
    selection: &Selection,
    layout: LayoutPreset,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let _ = layout;
    column![
        inspector_head(app, selection, tokens, reading),
        detail_body(app, selection, tokens, reading),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn diff<'a>(
    app: &'a GitApp,
    _selection: &Selection,
    layout: LayoutPreset,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let _ = layout;
    patch_pane(app, tokens, reading)
}

fn detail_body<'a>(
    app: &'a GitApp,
    selection: &Selection,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    if matches!(selection, Selection::File { .. }) {
        return column![
            changes_pane(app, selection, tokens, reading),
            patch_pane(app, tokens, reading),
        ]
        .height(Length::Fill)
        .into();
    }
    let show_patch = matches!(selection, Selection::Commit { .. } | Selection::Tag { .. });
    if !show_patch {
        return changes_pane(app, selection, tokens, reading);
    }

    let mut body = column![row![command_button(
        "Working tree",
        Message::ShowWorkingTree,
        true,
        tokens,
        reading,
    )]
    .padding([8, 12])]
    .spacing(4)
    .width(Length::Fill)
    .height(Length::Fill);

    if let Selection::Commit { id } = selection {
        if let Some(commit) = app.state.commits.iter().find(|commit| &commit.id == id) {
            body = body.push(
                container(
                    text(commit.message.clone())
                        .font(reading.font)
                        .size(reading.size)
                        .style(move |_| text::Style {
                            color: Some(tokens.text_primary),
                        }),
                )
                .padding([0, 12]),
            );
        }
    } else if let Selection::Tag { name, .. } = selection {
        body = body.push(
            text(name.clone())
                .font(reading.font)
                .size(reading.size)
                .style(move |_| text::Style {
                    color: Some(tokens.text_primary),
                }),
        );
    }

    body = body.push(patch_pane(app, tokens, reading));
    container(body)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn inspector_head<'a>(
    app: &'a GitApp,
    selection: &Selection,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let items = placed_commands(
        selection,
        can_commit(app),
        can_stash(app),
        CommandPlace::Inspector,
    );
    let mut commands = row![text(context_label(selection))
        .font(reading.font)
        .size(reading.size)
        .style(move |_| text::Style {
            color: Some(tokens.accent_primary),
        })]
    .spacing(COMMAND_SPACING)
    .align_y(Alignment::Center);
    for button in command_buttons(&items, selection, app, tokens, reading) {
        commands = commands.push(button);
    }
    column![
        container(commands.wrap())
            .padding([HEAD_PADDING_Y, HEAD_PADDING_X])
            .width(Length::Fill),
        hairline(tokens),
    ]
    .into()
}

fn command_head<'a>(
    items: &[crate::selection::RibbonItem],
    selection: &Selection,
    app: &'a GitApp,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let mut commands = row![].spacing(COMMAND_SPACING).align_y(Alignment::Center);
    for button in command_buttons(items, selection, app, tokens, reading) {
        commands = commands.push(button);
    }
    column![
        container(commands.wrap())
            .padding([HEAD_PADDING_Y, HEAD_PADDING_X])
            .width(Length::Fill),
        hairline(tokens),
    ]
    .into()
}

fn files_rail<'a>(
    app: &'a GitApp,
    layout: LayoutPreset,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    column![
        files_head(app.patches.len(), tokens, reading),
        files_body(app, layout, tokens, reading),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn files_head<'a>(count: usize, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    let title = format!("{FILES_TITLE} · {count}");
    column![
        row![
            text(title)
                .font(reading.font)
                .size((reading.size - 2.0).max(11.0))
                .style(move |_| text::Style {
                    color: Some(tokens.text_muted),
                }),
            iced::widget::horizontal_space(),
            command_button(
                COLLAPSE_FILES,
                Message::ToggleFilesRail,
                true,
                tokens,
                symbol_reading(&reading)
            ),
        ]
        .spacing(COMMAND_SPACING)
        .align_y(Alignment::Center)
        .padding([HEAD_PADDING_Y, HEAD_PADDING_X]),
        hairline(tokens),
    ]
    .into()
}

fn files_body<'a>(
    app: &'a GitApp,
    layout: LayoutPreset,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    if layout == LayoutPreset::Stacked {
        let mut chips = row![]
            .spacing(COMMAND_SPACING)
            .padding([HEAD_PADDING_Y, HEAD_PADDING_X])
            .width(Length::Fill);
        for patch in &app.patches {
            chips = chips.push(path_chip(&patch.path, tokens, reading));
        }
        return scrollable(chips.wrap())
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
    }

    let mut list = column![].spacing(2);
    for patch in &app.patches {
        list = list.push(path_line(&patch.path, tokens, reading));
    }
    scrollable(list)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn path_line<'a>(path: &str, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    container(
        text(path.to_string())
            .font(reading.mono)
            .size((reading.size - 2.0).max(11.0))
            .wrapping(Wrapping::None)
            .style(move |_| text::Style {
                color: Some(tokens.text_secondary),
            }),
    )
    .width(Length::Fill)
    .clip(true)
    .padding([2.0, HEAD_PADDING_X])
    .into()
}

fn path_chip<'a>(path: &str, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    container(
        text(path.to_string())
            .font(reading.mono)
            .size((reading.size - 2.0).max(11.0))
            .wrapping(Wrapping::None)
            .style(move |_| text::Style {
                color: Some(tokens.text_secondary),
            }),
    )
    .padding([CHIP_PADDING_Y, CHIP_PADDING_X])
    .style(move |_theme: &Theme| container::Style {
        background: Some(Background::Color(tokens.chip)),
        text_color: Some(tokens.text_secondary),
        border: Border {
            color: tokens.chip_border,
            width: 1.0,
            radius: CHIP_RADIUS.into(),
        },
        shadow: Shadow::default(),
    })
    .into()
}

fn collapsed_files_rail<'a>(
    layout: LayoutPreset,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    match layout {
        LayoutPreset::Stacked => container(
            row![rail_toggle(tokens, reading)]
                .align_y(Alignment::Center)
                .padding([2, 8]),
        )
        .width(Length::Fill)
        .style(move |_theme: &Theme| rail_container_style(tokens))
        .into(),
        LayoutPreset::Columns | LayoutPreset::InspectorOverDiff => container(
            column![rail_toggle(tokens, reading)]
                .align_x(Alignment::Center)
                .padding(2)
                .width(Length::Fill),
        )
        .width(Length::Fixed(FILES_RAIL_WIDTH))
        .height(Length::Fill)
        .style(move |_theme: &Theme| rail_container_style(tokens))
        .into(),
    }
}

fn rail_toggle<'a>(tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    button(
        text(EXPAND_FILES)
            .font(symbol_reading(&reading).font)
            .size(reading.size),
    )
    .padding(4)
    .on_press(Message::ToggleFilesRail)
    .style(move |_theme, status| rail_button_style(tokens, status))
    .into()
}

fn rail_container_style(tokens: DesignTokens) -> container::Style {
    container::Style {
        background: Some(Background::Color(tokens.bg_sidebar)),
        text_color: Some(tokens.text_secondary),
        border: Border {
            color: tokens.border,
            width: 1.0,
            radius: 0.0.into(),
        },
        shadow: Shadow::default(),
    }
}

fn rail_button_style(
    tokens: DesignTokens,
    status: iced::widget::button::Status,
) -> iced::widget::button::Style {
    let hot = matches!(
        status,
        iced::widget::button::Status::Hovered | iced::widget::button::Status::Pressed
    );
    iced::widget::button::Style {
        background: Some(Background::Color(if hot {
            tokens.border
        } else {
            tokens.bg_sidebar
        })),
        text_color: tokens.text_primary,
        border: Border {
            color: tokens.chip_border,
            width: 1.0,
            radius: CHIP_RADIUS.into(),
        },
        shadow: Shadow::default(),
    }
}
