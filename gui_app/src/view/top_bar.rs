use crate::app::{GitApp, Message};
use crate::palette::REPOSITORY_SCOPE;
use crate::theme::DesignTokens;
use iced::widget::{column, container, row, text};
use iced::{Alignment, Background, Border, Color, Element, Length, Shadow, Theme};

use super::chrome::{branch_chip, quiet_button, Reading};
use super::refs::current_branch_name;

/// Identity row: app mark, repository name and folder, then app-level commands.
const TITLE_ROW_HEIGHT: f32 = 36.0;
/// Branch row: the branch chip and tracking line, then branch and view commands.
/// Its own row so a long repository path can never push the chip around.
const BRANCH_ROW_HEIGHT: f32 = 34.0;
/// Both rows plus the hairline under them: where the area below the header starts.
pub(super) const HEADER_HEIGHT: f32 = TITLE_ROW_HEIGHT + BRANCH_ROW_HEIGHT + 1.0;
const TOP_BAR_PADDING: f32 = 12.0;
const CLUSTER_SPACING: f32 = 8.0;
const COMMAND_SPACING: f32 = 6.0;

const OPEN_REPOSITORY: &str = "Open…";
const CLONE_REPOSITORY: &str = "Clone…";
const RECENT_REPOSITORIES: &str = "Recent";
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
    column![
        chrome_row(title_row(app, tokens, reading), TITLE_ROW_HEIGHT, tokens),
        chrome_row(branch_row(app, tokens, reading), BRANCH_ROW_HEIGHT, tokens),
    ]
    .into()
}

fn chrome_row<'a>(
    content: Element<'a, Message>,
    height: f32,
    tokens: DesignTokens,
) -> Element<'a, Message> {
    // At least `height` tall, taller when a long name wraps: names are never clipped.
    let pad = ((height - 22.0) / 2.0).max(4.0);
    container(content)
        .width(Length::Fill)
        .padding([pad, TOP_BAR_PADDING])
        .align_y(Alignment::Center)
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(tokens.bg_chrome)),
            text_color: Some(tokens.text_primary),
            border: Border::default(),
            shadow: Shadow::default(),
        })
        .into()
}

fn title_row<'a>(app: &'a GitApp, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    let path = &app.session().path;
    let mut bar = row![
        super::wordmark::wordmark(app.settings.header_mark, tokens, reading),
        separator(tokens),
        repository_name(path, tokens, reading),
        repository_folder(path, tokens, reading),
    ]
    .spacing(CLUSTER_SPACING)
    .align_y(Alignment::Center)
    .width(Length::Fill);
    bar = bar.push(quiet_button(
        RECENT_REPOSITORIES,
        Message::TogglePalette(format!("{REPOSITORY_SCOPE} ")),
        !app.session().state.loading && app.settings.recent_repositories.len() > 1,
        tokens,
        reading,
    ));
    bar = bar.push(quiet_button(
        CLONE_REPOSITORY,
        Message::OpenClone,
        !app.session().state.loading,
        tokens,
        reading,
    ));
    bar = bar.push(quiet_button(
        OPEN_REPOSITORY,
        Message::ChooseRepositoryFolder,
        !app.session().state.loading,
        tokens,
        reading,
    ));
    bar.push(quiet_button(
        SETTINGS,
        Message::OpenSettings,
        true,
        tokens,
        reading,
    ))
    .into()
}

fn branch_row<'a>(app: &'a GitApp, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    let branch_name = current_branch_name(&app.session().state.branches);
    let summary = crate::tracking::tracking_summary(app.session().state.current_status.as_ref(), app.session().state.loading);

    // Three slots: the two sides share the spare width equally, so the sync group sits on the
    // window's centre line whatever the branch name or path. Each side clips rather than push.
    let left = container(
        row![
            branch_chip(
                branch_name,
                Message::ToggleBranchList,
                !app.session().state.loading,
                app.files_rail_open && !app.session().explorer_collapsed.local,
                tokens,
                reading,
            ),
            tracking(summary, tokens, reading),
        ]
        .spacing(COMMAND_SPACING)
        .align_y(Alignment::Center)
        // A long branch pushes the status line onto the next line instead of cutting either.
        .wrap(),
    )
    .width(Length::Fill);
    // The compare / conflict window: the selected change against HEAD, or yours against
    // theirs when the file is conflicted.
    let right = container(
        row![
            super::chrome::labelled_button(
                crate::icons::Icon::Compare,
                "Compare".to_string(),
                Message::OpenCompare { path: None, against: None },
                !app.session().state.loading,
                false,
                tokens,
                reading,
            ),
            hunk_segment(app.hunk_mode, tokens),
            layout_segment(app.layout, tokens),
        ]
        .spacing(COMMAND_SPACING)
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .align_x(iced::alignment::Horizontal::Right)
    .clip(true);

    row![left, sync_group(app, tokens, reading), right]
        .spacing(COMMAND_SPACING)
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .into()
}

/// Fetch, Pull ↓n, Push ↑n (or Publish to …, or Force push ↑n) and Stash (n files), fed by
/// the same tracking state as the branch line. A disabled button explains itself on hover.
fn sync_group<'a>(app: &'a GitApp, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    let session = app.session();
    let status = session.state.current_status.as_ref();
    let running = running_in(app).or(session.state.loading.then_some("Refresh"));
    let [fetch, pull, push] = crate::tracking::sync_buttons(status, &session.state.remotes, running);
    let stash = crate::tracking::stash_button(status, running);
    row![
        sync_button(crate::icons::Icon::Fetch, fetch, tokens, reading),
        sync_button(crate::icons::Icon::Pull, pull, tokens, reading),
        sync_button(crate::icons::Icon::Push, push, tokens, reading),
        sync_button(crate::icons::Icon::Stash, stash, tokens, reading),
    ]
    .spacing(COMMAND_SPACING)
    .align_y(Alignment::Center)
    .into()
}

/// The git action running in the active tab's worktree, by title ("Fetch", "Pull"…).
fn running_in(app: &GitApp) -> Option<&str> {
    let entry = app.activity.running()?;
    match &entry.running {
        Some((crate::activity::Lane::Worktree(path), _)) if *path == app.session().path => {
            Some(entry.record.title.as_str())
        }
        _ => None,
    }
}

/// What a sync button sends. Force push only ever opens its confirm.
fn sync_message(action: crate::tracking::SyncAction) -> Message {
    use crate::tracking::SyncAction;
    match action {
        SyncAction::Fetch => Message::Fetch,
        SyncAction::Pull => Message::Pull,
        SyncAction::Push => Message::Push,
        SyncAction::Publish(remote) => Message::Publish(remote),
        SyncAction::ForcePush => Message::AskForcePush,
        SyncAction::Stash => Message::StashPush,
    }
}

fn sync_button<'a>(
    icon: crate::icons::Icon,
    button: crate::tracking::SyncButton,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let message = sync_message(button.action);
    let drawn = super::chrome::labelled_button(icon, button.label, message, button.enabled, false, tokens, reading);
    match button.reason {
        Some(reason) if !button.enabled => iced::widget::tooltip(
            drawn,
            container(text(reason).font(reading.font).size((reading.size - 1.0).max(11.0)))
                .padding([4, 8])
                .style(move |_theme: &Theme| container::Style {
                    background: Some(Background::Color(tokens.bg_raised)),
                    text_color: Some(tokens.text_primary),
                    border: Border {
                        color: tokens.border,
                        width: 1.0,
                        radius: 4.0.into(),
                    },
                    shadow: Shadow::default(),
                }),
            iced::widget::tooltip::Position::Bottom,
        )
        .into(),
        _ => drawn,
    }
}

/// A thin upright rule between the mark and the repository.
fn separator<'a>(tokens: DesignTokens) -> Element<'a, Message> {
    container(column![])
        .width(Length::Fixed(1.0))
        .height(Length::Fixed(16.0))
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(tokens.border)),
            ..container::Style::default()
        })
        .into()
}

/// The folder's own name, which is what people call the repository.
fn repository_name<'a>(path: &str, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    super::chrome::full_text(
        crate::session::basename(path).to_string(),
        iced::Font {
            weight: iced::font::Weight::Semibold,
            ..reading.font
        },
        reading.size,
        tokens.text_primary,
    )
}

/// The full folder, muted, in the room that is left. It says which worktree this window is
/// working in, so it wraps rather than being cut.
fn repository_folder<'a>(path: &str, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    container(super::chrome::full_text(path.to_string(), reading.mono, (reading.size - 1.0).max(10.0), tokens.text_muted))
        .width(Length::Fill)
        .into()
}

fn tracking<'a>(summary: String, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    super::chrome::full_text(summary, reading.font, reading.size, tokens.text_secondary)
}

fn hunk_segment<'a>(current: crate::layout::HunkMode, tokens: DesignTokens) -> Element<'a, Message> {
    container(
        row![
            hunk_button(crate::layout::HunkMode::Beside, current, tokens),
            hunk_button(crate::layout::HunkMode::Wide, current, tokens),
            hunk_button(crate::layout::HunkMode::Focus, current, tokens),
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

fn hunk_button<'a>(
    mode: crate::layout::HunkMode,
    current: crate::layout::HunkMode,
    tokens: DesignTokens,
) -> Element<'a, Message> {
    let active = mode == current;
    let ink = if active {
        Color::BLACK
    } else {
        tokens.text_primary
    };
    let label = match mode {
        crate::layout::HunkMode::Beside => "B",
        crate::layout::HunkMode::Wide => "W",
        crate::layout::HunkMode::Focus => "F",
    };
    iced::widget::button(
        text(label)
            .size(11.0)
            .style(move |_| text::Style { color: Some(ink) }),
    )
    .padding([4, 6])
    .on_press(Message::SetHunkMode(mode))
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

fn layout_segment<'a>(
    current: crate::layout::LayoutPreset,
    tokens: DesignTokens,
) -> Element<'a, Message> {
    container(
        row![
            layout_button(crate::layout::LayoutPreset::Log, current, tokens),
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
        // History | inspector above a full-width diff.
        crate::layout::LayoutPreset::Log => iced::widget::column![
            row![mark_bar(7.0, 5.0, ink), mark_bar(3.0, 5.0, ink)].spacing(MARK_GAP),
            mark_bar(MARK_HEIGHT, 4.0, ink),
        ]
        .spacing(MARK_GAP)
        .into(),
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
