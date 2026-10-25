use crate::app::{GitApp, Message};
use crate::layout::{LayoutPreset, WorkspacePane};
use crate::selection::{context_label, Selection};
use crate::theme::DesignTokens;
use iced::widget::pane_grid::{self, Content};
use iced::widget::{button, column, container, row, text};
use iced::{Alignment, Background, Border, Element, Length, Shadow, Theme};

use super::chrome::{command_button, hairline, pane_title, symbol_reading, Reading};
use super::commands::{can_commit, can_stash, command_buttons, placed_commands, CommandPlace};
use super::explorer::explorer_pane;
use super::files_nav::files_navigator;
use super::history::history_pane;
use super::patch::patch_pane;

const PANE_SPACING: f32 = 4.0;
const SPLIT_HIT: f32 = 8.0;
const HEAD_PADDING_Y: f32 = 4.0;
const HEAD_PADDING_X: f32 = 8.0;
const COMMAND_SPACING: f32 = 6.0;
const FILES_RAIL_WIDTH: f32 = 36.0;
const FILES_TITLE: &str = "Explorer";
const GRAPH_TITLE: &str = "History";
const DIFF_TITLE: &str = "Diff";
const COLLAPSE_FILES: &str = "⟨";
const EXPAND_FILES: &str = "⟩";

pub(super) fn workspace<'a>(
    app: &'a GitApp,
    selection: &Selection,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let grid = pane_grid::PaneGrid::new(&app.panes, move |_pane, kind, _maximised| {
        let surface = kind;
        let body = match kind {
            WorkspacePane::Graph => graph(app, selection, app.layout, tokens, reading),
            WorkspacePane::Inspector => inspector(app, selection, app.layout, tokens, reading),
            WorkspacePane::Diff => diff(app, selection, app.layout, tokens, reading),
            WorkspacePane::Files => files_rail(app, selection, app.layout, tokens, reading),
        };
        Content::new(body).style(move |_theme| pane_surface(tokens, *surface))
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
        LayoutPreset::Stacked => column![rail, grid]
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
        LayoutPreset::Columns | LayoutPreset::InspectorOverDiff => row![rail, grid]
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
    }
}

fn pane_surface(tokens: DesignTokens, kind: WorkspacePane) -> container::Style {
    let background = match kind {
        WorkspacePane::Graph => tokens.bg_app,
        WorkspacePane::Inspector => tokens.bg_sidebar,
        WorkspacePane::Diff => tokens.bg_raised,
        WorkspacePane::Files => tokens.bg_sidebar,
    };
    container::Style {
        background: Some(Background::Color(background)),
        text_color: Some(tokens.text_secondary),
        border: Border {
            color: tokens.border,
            width: if matches!(kind, WorkspacePane::Graph) {
                0.0
            } else {
                1.0
            },
            radius: 0.0.into(),
        },
        shadow: Shadow::default(),
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
    let commands = placed_commands(
        selection,
        can_commit(app),
        can_stash(app),
        CommandPlace::Graph,
    );
    column![
        titled_command_head(GRAPH_TITLE, &commands, selection, app, tokens, reading),
        history_pane(app, selection, tokens, reading),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
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
        files_navigator(app, selection, tokens, reading),
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
    let title = if app.session().patches.is_empty() {
        DIFF_TITLE.to_string()
    } else {
        format!("{DIFF_TITLE} · {}", app.session().patches.len())
    };
    column![
        column![
            container(pane_title(&title, tokens, reading))
                .padding([HEAD_PADDING_Y, HEAD_PADDING_X])
                .width(Length::Fill),
            hairline(tokens),
        ],
        patch_pane(app, tokens, reading),
    ]
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

fn titled_command_head<'a>(
    title: &'a str,
    items: &[crate::selection::RibbonItem],
    selection: &Selection,
    app: &'a GitApp,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let mut commands = row![pane_title(title, tokens, reading)]
        .spacing(COMMAND_SPACING)
        .align_y(Alignment::Center);
    commands = commands.push(iced::widget::horizontal_space());
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
    selection: &Selection,
    layout: LayoutPreset,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let _ = (layout, selection);
    column![
        files_head(tokens, reading),
        explorer_pane(app, tokens, reading),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn files_head<'a>(tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    let title = FILES_TITLE;
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
            radius: 4.0.into(),
        },
        shadow: Shadow::default(),
    }
}
