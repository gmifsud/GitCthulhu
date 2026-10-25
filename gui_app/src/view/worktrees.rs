//! Worktrees in the left explorer, and the create / remove dialogs.
//!
//! Remove is only offered here, never from a tab's `×`, and never for the main worktree.

use crate::app::{GitApp, Message, WorktreePrompt};
use crate::session::{basename, same_path};
use crate::theme::DesignTokens;
use core_domain::Worktree;
use iced::widget::button::Status;
use iced::widget::text::Wrapping;
use iced::widget::scrollable::{Direction, Scrollbar};
use iced::widget::{
    button, checkbox, column, container, horizontal_space, row, scrollable, text, text_input,
};
use iced::{Alignment, Background, Border, Color, Element, Length, Shadow, Theme};

use super::chrome::{
    accent_button, command_button, field_style, glyph, hairline, icon_button, pane_title, Reading,
};

const WORKTREES_TITLE: &str = "Worktrees";
const LIST_MAX_HEIGHT: f32 = 160.0;
const OPEN_MARK: &str = "●";
const DETACHED: &str = "(detached)";

pub(super) fn worktrees_section<'a>(
    app: &'a GitApp,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let title = format!("{WORKTREES_TITLE} · {}", app.worktree_catalogue.len());
    let header = row![
        pane_title(title, tokens, reading),
        horizontal_space(),
        icon_button(
            "+",
            "New worktree",
            "worktree-new",
            app.tip_ready("worktree-new"),
            Message::AskCreateWorktree,
            !app.worktree_busy,
            false,
            tokens,
            reading,
        ),
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    let mut rows = column![].spacing(2);
    if app.worktree_catalogue.is_empty() && app.catalogue_error.is_none() {
        rows = rows.push(
            text("No worktrees loaded.")
                .font(reading.font)
                .size(reading.size)
                .style(move |_| text::Style {
                    color: Some(tokens.text_muted),
                }),
        );
    }
    for entry in &app.worktree_catalogue {
        rows = rows.push(worktree_row(app, entry, tokens, reading));
    }

    let mut section = column![header].spacing(6).padding([8, 12]);
    if let Some(error) = &app.catalogue_error {
        section = section.push(
            text(format!("Could not list worktrees: {error}"))
                .font(reading.font)
                .size(reading.size)
                .style(move |_| text::Style {
                    color: Some(tokens.accent_primary),
                }),
        );
    }
    // Embed the scrollbar so it takes layout space and does not cover the × buttons.
    let list = scrollable(rows).direction(Direction::Vertical(
        Scrollbar::new()
            .width(8)
            .scroller_width(8)
            .spacing(6),
    ));
    section = section.push(container(list).max_height(LIST_MAX_HEIGHT));

    column![section, hairline(tokens)].into()
}

fn worktree_row<'a>(
    app: &'a GitApp,
    entry: &'a Worktree,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let open = app
        .sessions
        .iter()
        .any(|session| same_path(&session.path, &entry.path));
    let active = app
        .sessions
        .get(app.active)
        .is_some_and(|session| same_path(&session.path, &entry.path));
    let branch = if entry.branch.is_empty() {
        DETACHED.to_string()
    } else {
        entry.branch.clone()
    };

    let mut name = row![text(basename(&entry.path).to_string())
        .font(reading.font)
        .size(reading.size)
        .wrapping(Wrapping::None)
        .style(move |_| text::Style {
            color: Some(if open {
                tokens.text_primary
            } else {
                tokens.text_secondary
            }),
        })]
    .spacing(6)
    .align_y(Alignment::Center);
    if entry.is_main {
        name = name.push(tag("main", tokens, reading));
    }
    if entry.locked {
        name = name.push(tag("locked", tokens, reading));
    }

    let marker: Element<'a, Message> = glyph(
        OPEN_MARK,
        (reading.size - 4.0).max(7.0),
        if open {
            tokens.accent_primary
        } else {
            Color::TRANSPARENT
        },
        &reading,
    );

    let body = button(
        row![
            marker,
            column![
                name,
                text(branch)
                    .font(reading.font)
                    .size((reading.size - 2.0).max(11.0))
                    .wrapping(Wrapping::None)
                    .style(move |_| text::Style {
                        color: Some(tokens.accent_primary),
                    }),
            ]
            .spacing(1)
            .width(Length::Fill),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding([3, 6])
    .width(Length::Fill)
    .on_press(Message::OpenWorktree(entry.path.clone()))
    .style(move |_theme, status| {
        let hot = matches!(status, Status::Hovered | Status::Pressed);
        iced::widget::button::Style {
            background: Some(Background::Color(if active {
                tokens.bg_raised
            } else if hot {
                tokens.border
            } else {
                Color::TRANSPARENT
            })),
            text_color: tokens.text_secondary,
            border: Border {
                color: if active {
                    tokens.accent_primary
                } else {
                    Color::TRANSPARENT
                },
                width: if active { 1.0 } else { 0.0 },
                radius: 4.0.into(),
            },
            shadow: Shadow::default(),
        }
    });

    let remove_key = format!("worktree-remove:{}", entry.path);
    let tip_ready = app.tip_ready(&remove_key);
    row![
        body,
        icon_button(
            "×",
            "Remove worktree",
            remove_key,
            tip_ready,
            Message::AskRemoveWorktree {
                path: entry.path.clone(),
            },
            // The main worktree is never offered, and a job in flight owns the dialog.
            !entry.is_main && !app.worktree_busy,
            false,
            tokens,
            reading,
        ),
    ]
    .spacing(4)
    .align_y(Alignment::Center)
    .into()
}

fn tag<'a>(label: &'static str, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    container(
        text(label)
            .font(reading.font)
            .size((reading.size - 3.0).max(10.0))
            .style(move |_| text::Style {
                color: Some(tokens.text_muted),
            }),
    )
    .padding([0, 4])
    .style(move |_theme: &Theme| container::Style {
        background: Some(Background::Color(Color::TRANSPARENT)),
        text_color: Some(tokens.text_muted),
        border: Border {
            color: tokens.chip_border,
            width: 1.0,
            radius: 3.0.into(),
        },
        shadow: Shadow::default(),
    })
    .into()
}

/// The create or remove dialog. `error` is git's last message, shown here because the status
/// bar sits behind the dialog.
pub(super) fn worktree_prompt<'a>(
    prompt: &'a WorktreePrompt,
    error: Option<&'a str>,
    busy: bool,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let mut card = match prompt {
        WorktreePrompt::Create {
            path,
            branch,
            create_branch,
            start_point,
        } => create_card(
            path,
            branch,
            *create_branch,
            start_point.as_deref(),
            busy,
            tokens,
            reading,
        ),
        WorktreePrompt::Remove {
            path,
            branch,
            force,
        } => remove_card(path, branch, *force, busy, tokens, reading),
    };
    if let Some(error) = error {
        card = card.push(
            text(error.to_string())
                .font(reading.font)
                .size(reading.size)
                .style(move |_| text::Style {
                    color: Some(tokens.accent_primary),
                }),
        );
    }
    modal(card, tokens)
}

fn heading<'a>(
    label: &'static str,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    text(label)
        .font(reading.font)
        .size(reading.size + 2.0)
        .style(move |_| text::Style {
            color: Some(tokens.text_primary),
        })
        .into()
}

fn body_text<'a>(
    label: impl Into<String>,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    text(label.into())
        .font(reading.font)
        .size(reading.size)
        .style(move |_| text::Style {
            color: Some(tokens.text_secondary),
        })
        .into()
}

fn create_card<'a>(
    path: &'a str,
    branch: &'a str,
    create_branch: bool,
    start_point: Option<&'a str>,
    busy: bool,
    tokens: DesignTokens,
    reading: Reading,
) -> iced::widget::Column<'a, Message> {
    let absolute = std::path::Path::new(path.trim()).is_absolute();
    let ready = absolute && !branch.trim().is_empty() && !busy;
    let mut card = column![
        heading("New worktree", tokens, reading),
        body_text(
            "Adds a linked checkout of this repository. Its tab opens when git finishes.",
            tokens,
            reading,
        ),
        row![
            text_input("Absolute folder path", path)
                .on_input(Message::WorktreePathChanged)
                .padding(6)
                .width(Length::Fill)
                .style(field_style(tokens)),
            command_button(
                "Browse…",
                Message::ChooseWorktreeFolder,
                !busy,
                tokens,
                reading,
            ),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    ]
    .spacing(12)
    .padding(20)
    .width(Length::Fixed(460.0));
    if !path.trim().is_empty() && !absolute {
        card = card.push(body_text(
            "Use an absolute path, such as D:\\Repos\\feature.",
            tokens,
            reading,
        ));
    }
    card = card.push(
        text_input(
            if create_branch {
                "New branch name"
            } else {
                "Existing local branch"
            },
            branch,
        )
        .on_input(Message::WorktreeBranchChanged)
        .padding(6)
        .width(Length::Fill)
        .style(field_style(tokens)),
    );
    let create_label = match start_point {
        Some(tip) => format!(
            "Create this branch at {}",
            crate::selection::short_hash(tip)
        ),
        None => "Create this branch at HEAD".to_string(),
    };
    card = card.push(
        checkbox(create_label, create_branch)
            .on_toggle(Message::WorktreeCreateBranchToggled)
            .text_size(reading.size)
            .font(reading.font),
    );
    if let Some(tip) = start_point {
        card = card.push(body_text(
            format!("Start tip: {tip}"),
            tokens,
            reading,
        ));
    }
    card.push(
        row![
            accent_button(
                "Create",
                Message::ConfirmCreateWorktree,
                ready,
                tokens,
                reading,
            ),
            command_button("Cancel", Message::CancelWorktree, true, tokens, reading),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
}

fn remove_card<'a>(
    path: &'a str,
    branch: &'a str,
    force: bool,
    busy: bool,
    tokens: DesignTokens,
    reading: Reading,
) -> iced::widget::Column<'a, Message> {
    let branch = if branch.is_empty() { DETACHED } else { branch };
    column![
        heading("Remove this worktree?", tokens, reading),
        text(path.to_string())
            .font(reading.mono)
            .size(reading.size)
            .style(move |_| text::Style {
                color: Some(tokens.accent_primary),
            }),
        body_text(format!("Branch: {branch}"), tokens, reading),
        body_text(
            "The folder is deleted from disk. Closing its tab does not do this. A worktree with uncommitted changes needs Force.",
            tokens,
            reading,
        ),
        checkbox("Force remove (discards uncommitted changes)", force)
            .on_toggle(Message::WorktreeForceToggled)
            .text_size(reading.size)
            .font(reading.font),
        row![
            accent_button(
                "Remove",
                Message::ConfirmRemoveWorktree { force },
                !busy,
                tokens,
                reading,
            ),
            command_button("Cancel", Message::CancelWorktree, true, tokens, reading),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    ]
    .spacing(12)
    .padding(20)
    .width(Length::Fixed(460.0))
}

fn modal<'a>(
    card: iced::widget::Column<'a, Message>,
    tokens: DesignTokens,
) -> Element<'a, Message> {
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
