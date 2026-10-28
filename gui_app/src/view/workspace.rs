use crate::app::{GitApp, Message};
use crate::layout::{HunkMode, LayoutPreset, WorkspacePane};
use crate::selection::{context_label, Selection};
use crate::theme::DesignTokens;
use iced::widget::pane_grid::{self, Content};
use iced::widget::{button, column, container, row, text};
use iced::{Alignment, Background, Border, Element, Length, Shadow, Theme};

use super::chrome::{command_button, hairline, pane_title, symbol_reading, Reading};
use super::commands::{can_commit, can_stash, labelled_command_buttons, placed_commands, CommandPlace};
use super::filter_bar::filter_bar;
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
const DIFF_TITLE: &str = "Diff";
const PREVIOUS_FILE: &str = "‹";
const NEXT_FILE: &str = "›";
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

    if app.files_rail_open || app.hunk_mode == HunkMode::Focus {
        return grid.into();
    }

    let rail = collapsed_files_rail(app.layout, tokens, reading);
    match app.layout {
        LayoutPreset::Stacked => column![rail, grid]
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
        LayoutPreset::Log | LayoutPreset::Columns | LayoutPreset::InspectorOverDiff => row![rail, grid]
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
    // JetBrains keeps the log head for filters; the selection's commands sit on the
    // Inspector head and on each row's right-click menu.
    column![filter_bar(app, tokens, reading), history_pane(app, selection, tokens, reading)]
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
    let session = app.session();
    let mut head = row![pane_title(&title, tokens, reading), iced::widget::horizontal_space()]
        .spacing(COMMAND_SPACING)
        .align_y(Alignment::Center);
    if session.patches.len() > 1 {
        let position = session
            .focused_diff_path
            .as_deref()
            .and_then(|focused| session.patches.iter().position(|patch| patch.path == focused))
            .map_or_else(
                || format!("All {} files", session.patches.len()),
                |index| format!("{}/{} files", index + 1, session.patches.len()),
            );
        let step = |forward: bool| session.diff_step(forward).map(Message::FocusDiffPath);
        head = head
            .push(step_button(PREVIOUS_FILE, step(false), tokens, reading))
            .push(
                text(position)
                    .font(reading.font)
                    .size(reading.size)
                    .style(move |_| text::Style {
                        color: Some(tokens.text_secondary),
                    }),
            )
            .push(step_button(NEXT_FILE, step(true), tokens, reading));
        if session.focused_diff_path.is_some() {
            head = head.push(command_button("All files", Message::ClearDiffFocus, true, tokens, reading));
        }
    }
    column![
        column![
            container(head)
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
    // The log head holds filters only, so the selection's branch commands join its file commands.
    let items: Vec<_> = [CommandPlace::Graph, CommandPlace::Inspector]
        .into_iter()
        .flat_map(|place| placed_commands(selection, can_commit(app), can_stash(app), place))
        .collect();
    let mut commands = row![text(context_label(selection))
        .font(reading.font)
        .size(reading.size)
        .style(move |_| text::Style {
            color: Some(tokens.accent_primary),
        })]
    .spacing(COMMAND_SPACING)
    .align_y(Alignment::Center);
    for button in labelled_command_buttons(&items, selection, app, tokens, reading) {
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
        LayoutPreset::Log | LayoutPreset::Columns | LayoutPreset::InspectorOverDiff => container(
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

/// "‹" or "›" in the Diff head; disabled at either end of the file list.
fn step_button<'a>(mark: &'static str, message: Option<Message>, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    let enabled = message.is_some();
    button(text(mark).font(reading.font).size(reading.size + 2.0))
        .padding([0, 6])
        .on_press_maybe(message)
        .style(move |_theme, status| {
            let hot = enabled && matches!(status, iced::widget::button::Status::Hovered | iced::widget::button::Status::Pressed);
            iced::widget::button::Style {
                background: Some(Background::Color(if hot { tokens.border } else { iced::Color::TRANSPARENT })),
                text_color: if enabled { tokens.text_primary } else { tokens.text_muted },
                border: Border { radius: 3.0.into(), ..Border::default() },
                shadow: Shadow::default(),
            }
        })
        .into()
}
