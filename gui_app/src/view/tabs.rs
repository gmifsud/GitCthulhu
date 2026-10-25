//! The tab strip: one tab per open worktree session, and nothing else.
//!
//! `×` closes the session only; the worktree on disk is never touched from here.
//! `+` lists the catalogue entries that are not open yet.

use crate::app::{GitApp, Message};
use crate::session::{basename, same_path, WorktreeSession};
use crate::theme::DesignTokens;
use core_domain::Worktree;
use iced::widget::button::Status;
use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, row, scrollable, text, tooltip};
use iced::{Alignment, Background, Border, Color, Element, Length, Shadow, Theme};

use super::chrome::{glyph, hairline, Reading};

const CLOSE_MARK: &str = "×";
const ADD_MARK: &str = "+";
const UNDERLINE: f32 = 2.0;
const PICKER_MAX_HEIGHT: f32 = 200.0;
const DETACHED: &str = "(detached)";

pub(super) fn tab_strip<'a>(
    app: &'a GitApp,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let mut strip = row![].align_y(Alignment::End).spacing(2);
    for (index, session) in app.sessions.iter().enumerate() {
        strip = strip.push(tab(app, index, session, tokens, reading));
    }
    strip = strip.push(add_button(app.tab_picker_open, tokens, reading));

    let mut bar = column![container(strip).padding([0, 8]).width(Length::Fill)];
    if app.tab_picker_open {
        bar = bar.push(hairline(tokens));
        bar = bar.push(picker(app, tokens, reading));
    }
    bar = bar.push(hairline(tokens));

    container(bar)
        .width(Length::Fill)
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(tokens.bg_chrome)),
            text_color: Some(tokens.text_primary),
            border: Border::default(),
            shadow: Shadow::default(),
        })
        .into()
}

fn tab<'a>(
    app: &'a GitApp,
    index: usize,
    session: &'a WorktreeSession,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let active = index == app.active;
    // The last tab stays, and a tab with a job running has nowhere to land its finish.
    let can_close = app.sessions.len() > 1 && !session.state.loading;
    let ink = if active {
        tokens.text_primary
    } else {
        tokens.text_secondary
    };

    let label = button(
        text(basename(&session.path).to_string())
            .font(reading.font)
            .size(reading.size)
            .wrapping(Wrapping::None)
            .style(move |_| text::Style { color: Some(ink) }),
    )
    .padding([4, 8])
    .on_press(Message::SelectWorktreeTab(index))
    .style(move |_theme, _status| flat_button(ink));

    let close = button(glyph(
        CLOSE_MARK,
        reading.size,
        if can_close {
            tokens.text_secondary
        } else {
            tokens.text_muted
        },
        &reading,
    ))
    .padding([2, 6])
    .on_press_maybe(can_close.then_some(Message::CloseWorktreeTab(index)))
    .style(move |_theme, status| {
        let hot = can_close && matches!(status, Status::Hovered | Status::Pressed);
        iced::widget::button::Style {
            background: Some(Background::Color(if hot {
                tokens.border
            } else {
                Color::TRANSPARENT
            })),
            text_color: tokens.text_secondary,
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 3.0.into(),
            },
            shadow: Shadow::default(),
        }
    });

    let underline = container(column![])
        .width(Length::Fill)
        .height(Length::Fixed(UNDERLINE))
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(if active {
                tokens.accent_primary
            } else {
                Color::TRANSPARENT
            })),
            text_color: None,
            border: Border::default(),
            shadow: Shadow::default(),
        });

    let cell = container(column![
        row![label, close].align_y(Alignment::Center),
        underline
    ])
    .style(move |_theme: &Theme| container::Style {
        background: Some(Background::Color(if active {
            tokens.bg_raised
        } else {
            Color::TRANSPARENT
        })),
        text_color: Some(ink),
        border: Border::default(),
        shadow: Shadow::default(),
    });

    tooltip(
        cell,
        container(
            text(tooltip_text(app, session))
                .font(reading.mono)
                .size((reading.size - 1.0).max(10.0)),
        )
        .padding([4, 8])
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(tokens.bg_app)),
            text_color: Some(tokens.text_primary),
            border: Border {
                color: tokens.border,
                width: 1.0,
                radius: 4.0.into(),
            },
            shadow: Shadow::default(),
        }),
        tooltip::Position::Bottom,
    )
    .into()
}

/// Full path, then the branch checked out there.
fn tooltip_text(app: &GitApp, session: &WorktreeSession) -> String {
    let branch = session
        .state
        .branches
        .iter()
        .find(|branch| !branch.is_remote && branch.is_head)
        .map(|branch| branch.name.clone())
        .or_else(|| {
            app.worktree_catalogue
                .iter()
                .find(|entry| same_path(&entry.path, &session.path))
                .map(|entry| entry.branch.clone())
        })
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| DETACHED.to_string());
    format!("{}\n{branch}", session.path)
}

fn add_button<'a>(open: bool, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    button(glyph(
        ADD_MARK,
        reading.size + 2.0,
        if open {
            tokens.accent_primary
        } else {
            tokens.text_secondary
        },
        &reading,
    ))
    .padding([2, 8])
    .on_press(Message::ToggleTabPicker)
    .style(move |_theme, status| {
        let hot = matches!(status, Status::Hovered | Status::Pressed);
        iced::widget::button::Style {
            background: Some(Background::Color(if hot {
                tokens.bg_raised
            } else {
                Color::TRANSPARENT
            })),
            text_color: tokens.text_secondary,
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 3.0.into(),
            },
            shadow: Shadow::default(),
        }
    })
    .into()
}

fn flat_button(ink: Color) -> iced::widget::button::Style {
    iced::widget::button::Style {
        background: Some(Background::Color(Color::TRANSPARENT)),
        text_color: ink,
        border: Border::default(),
        shadow: Shadow::default(),
    }
}

/// The catalogue entries that have no tab yet.
fn unopened(app: &GitApp) -> Vec<&Worktree> {
    app.worktree_catalogue
        .iter()
        .filter(|entry| {
            !app.sessions
                .iter()
                .any(|session| same_path(&session.path, &entry.path))
        })
        .collect()
}

fn picker<'a>(app: &'a GitApp, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    let mut list = column![].spacing(2).padding([6, 12]);
    let closed = unopened(app);
    if closed.is_empty() {
        let note = match &app.catalogue_error {
            Some(error) => format!("Could not list worktrees: {error}"),
            None => "Every worktree is already open.".to_string(),
        };
        list = list.push(
            text(note)
                .font(reading.font)
                .size(reading.size)
                .style(move |_| text::Style {
                    color: Some(tokens.text_muted),
                }),
        );
    }
    for entry in closed {
        let branch = if entry.branch.is_empty() {
            DETACHED.to_string()
        } else {
            entry.branch.clone()
        };
        list = list.push(
            button(
                row![
                    text(basename(&entry.path).to_string())
                        .font(reading.font)
                        .size(reading.size)
                        .style(move |_| text::Style {
                            color: Some(tokens.text_primary),
                        }),
                    text(branch)
                        .font(reading.font)
                        .size(reading.size)
                        .style(move |_| text::Style {
                            color: Some(tokens.accent_primary),
                        }),
                    text(entry.path.clone())
                        .font(reading.mono)
                        .size((reading.size - 2.0).max(11.0))
                        .wrapping(Wrapping::None)
                        .style(move |_| text::Style {
                            color: Some(tokens.text_muted),
                        }),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            )
            .padding([3, 6])
            .width(Length::Fill)
            .on_press(Message::OpenWorktree(entry.path.clone()))
            .style(move |_theme, status| {
                let hot = matches!(status, Status::Hovered | Status::Pressed);
                iced::widget::button::Style {
                    background: Some(Background::Color(if hot {
                        tokens.bg_raised
                    } else {
                        Color::TRANSPARENT
                    })),
                    text_color: tokens.text_secondary,
                    border: Border::default(),
                    shadow: Shadow::default(),
                }
            }),
        );
    }
    container(scrollable(list))
        .max_height(PICKER_MAX_HEIGHT)
        .width(Length::Fill)
        .into()
}
