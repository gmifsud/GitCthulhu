//! Left-rail Explorer accordion: LOCAL / REMOTE / WORKTREES / STASHES / TAGS.
//!
//! Each section has a clickable header that collapses its body. The collapsed state
//! lives on `WorktreeSession::explorer_collapsed` (pure, reducer-safe).

use crate::app::{GitApp, Message};
use crate::context_menu::ContextTarget;
use crate::session::{basename, same_path, ExplorerSection};
use crate::theme::DesignTokens;
use core_domain::{Branch, Stash, Tag, Worktree};
use iced::widget::button::Status;
use iced::widget::scrollable::{Direction, Scrollbar};
use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, mouse_area, row, scrollable, text};
use iced::{Alignment, Background, Border, Color, Element, Length, Shadow};

use super::chrome::{
    glyph, hairline, icon_button, symbol_reading, Reading, BRANCH_MARK, FOLD_CLOSED, FOLD_OPEN,
    REMOTE_MARK, STASH_MARK, TAG_MARK,
};

const HEAD_PADDING_Y: f32 = 4.0;
const HEAD_PADDING_X: f32 = 8.0;
const OPEN_MARK: &str = "●";
const DETACHED: &str = "(detached)";

/// The full Explorer accordion pane for the left files rail.
pub(super) fn explorer_pane<'a>(
    app: &'a GitApp,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let session = app.session();
    let collapsed = &session.explorer_collapsed;

    let branches = &session.state.branches;
    let stashes = &session.state.stashes;
    let tags = &session.state.tags;

    let locals: Vec<&Branch> = branches.iter().filter(|b| !b.is_remote).collect();
    let remotes: Vec<&Branch> = branches.iter().filter(|b| b.is_remote).collect();
    let worktrees = &app.worktree_catalogue;

    column![
        section_block(
            "LOCAL",
            locals.len(),
            ExplorerSection::Local,
            collapsed.local,
            (!collapsed.local).then(|| local_rows(&locals, tokens, reading)),
            None,
            tokens,
            reading,
        ),
        hairline(tokens),
        section_block(
            "REMOTE",
            remotes.len(),
            ExplorerSection::Remote,
            collapsed.remote,
            (!collapsed.remote).then(|| remote_rows(&remotes, tokens, reading)),
            None,
            tokens,
            reading,
        ),
        hairline(tokens),
        section_block(
            "WORKTREES",
            worktrees.len(),
            ExplorerSection::Worktrees,
            collapsed.worktrees,
            (!collapsed.worktrees).then(|| worktrees_rows(app, worktrees, tokens, reading)),
            Some(icon_button(
                "+",
                "New worktree",
                "worktree-new",
                app.tip_ready("worktree-new"),
                Message::AskCreateWorktree,
                !app.worktree_busy,
                false,
                tokens,
                reading,
            )),
            tokens,
            reading,
        ),
        hairline(tokens),
        section_block(
            "STASHES",
            stashes.len(),
            ExplorerSection::Stashes,
            collapsed.stashes,
            (!collapsed.stashes).then(|| stash_rows(stashes, tokens, reading)),
            None,
            tokens,
            reading,
        ),
        hairline(tokens),
        section_block(
            "TAGS",
            tags.len(),
            ExplorerSection::Tags,
            collapsed.tags,
            (!collapsed.tags).then(|| tag_rows(tags, tokens, reading)),
            None,
            tokens,
            reading,
        ),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

/// One collapsible accordion block: header + optional body (body omitted when collapsed).
fn section_block<'a>(
    title: &'static str,
    count: usize,
    section: ExplorerSection,
    collapsed: bool,
    body: Option<Element<'a, Message>>,
    trailing: Option<Element<'a, Message>>,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let chevron = if collapsed { FOLD_CLOSED } else { FOLD_OPEN };
    let header_label = format!("{title} · {count}");
    let sym = symbol_reading(&reading);
    let toggle = button(
        row![
            text(chevron).font(sym.font).size(reading.size),
            text(header_label)
                .font(reading.font)
                .size((reading.size - 1.0).max(11.0))
                .style(move |_| iced::widget::text::Style {
                    color: Some(tokens.text_muted),
                }),
        ]
        .spacing(4)
        .align_y(Alignment::Center),
    )
    .padding([HEAD_PADDING_Y, HEAD_PADDING_X])
    .width(Length::Fill)
    .on_press(Message::ToggleExplorerSection(section))
    .style(move |_theme, status| {
        let hot = matches!(status, Status::Hovered | Status::Pressed);
        iced::widget::button::Style {
            background: Some(Background::Color(if hot {
                tokens.border
            } else {
                Color::TRANSPARENT
            })),
            text_color: tokens.text_muted,
            border: Border::default(),
            shadow: Shadow::default(),
        }
    });
    let mut header_row = row![toggle].align_y(Alignment::Center);
    if let Some(action) = trailing {
        header_row = header_row.push(action);
    }
    let header = container(header_row)
        .width(Length::Fill)
        .padding([0, 4, 0, 0]);

    let mut block = column![header].width(Length::Fill);
    if let Some(body) = body {
        debug_assert!(!collapsed);
        block = block.push(body);
    }
    block.into()
}

// ── LOCAL ────────────────────────────────────────────────────────────────────

fn local_rows<'a>(
    locals: &[&Branch],
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let mut rows = column![].spacing(2).padding([4, 8]);
    for branch in locals {
        let selected = branch.is_head;
        rows = rows.push(branch_row(
            BRANCH_MARK,
            branch.name.clone(),
            Message::ChooseBranch(branch.name.clone()),
            selected,
            branch.is_head,
            Some(ContextTarget::LocalBranch {
                name: branch.name.clone(),
                tip: branch.tip.clone(),
            }),
            tokens,
            reading,
        ));
    }
    if locals.is_empty() {
        rows = rows.push(empty_hint("No local branches", tokens, reading));
    }
    rows.into()
}

// ── REMOTE ───────────────────────────────────────────────────────────────────

fn remote_rows<'a>(
    remotes: &[&Branch],
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let mut rows = column![].spacing(2).padding([4, 8]);
    for branch in remotes {
        let selected = false;
        rows = rows.push(branch_row(
            REMOTE_MARK,
            branch.name.clone(),
            Message::Select(crate::selection::Selection::Remote {
                name: branch.name.clone(),
            }),
            selected,
            false,
            None,
            tokens,
            reading,
        ));
    }
    if remotes.is_empty() {
        rows = rows.push(empty_hint("No remote branches", tokens, reading));
    }
    rows.into()
}

// ── WORKTREES ────────────────────────────────────────────────────────────────

fn worktrees_rows<'a>(
    app: &'a GitApp,
    worktrees: &'a [Worktree],
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let mut rows = column![].spacing(2);
    if worktrees.is_empty() {
        rows = rows.push(
            container(empty_hint("No worktrees loaded.", tokens, reading)).padding([4, 8]),
        );
    }
    for entry in worktrees {
        rows = rows.push(worktree_row(app, entry, tokens, reading));
    }
    // Embedded scrollbar so it takes layout space and does not cover buttons.
    let list = scrollable(rows).direction(Direction::Vertical(
        Scrollbar::new().width(8).scroller_width(8).spacing(6),
    ));
    container(list).max_height(160.0).width(Length::Fill).into()
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

    let mut name_row = row![text(basename(&entry.path).to_string())
        .font(reading.font)
        .size(reading.size)
        .wrapping(Wrapping::None)
        .style(move |_| iced::widget::text::Style {
            color: Some(if open {
                tokens.text_primary
            } else {
                tokens.text_secondary
            }),
        })]
    .spacing(6)
    .align_y(Alignment::Center);

    if entry.is_main {
        name_row = name_row.push(wt_tag("main", tokens, reading));
    }
    if entry.locked {
        name_row = name_row.push(wt_tag("locked", tokens, reading));
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
                name_row,
                text(branch)
                    .font(reading.font)
                    .size((reading.size - 2.0).max(11.0))
                    .wrapping(Wrapping::None)
                    .style(move |_| iced::widget::text::Style {
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

fn wt_tag<'a>(label: &'static str, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    use iced::widget::Theme;
    container(
        text(label)
            .font(reading.font)
            .size((reading.size - 3.0).max(10.0))
            .style(move |_| iced::widget::text::Style {
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

// ── STASHES ──────────────────────────────────────────────────────────────────

fn stash_rows<'a>(stashes: &'a [Stash], tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    let mut rows = column![].spacing(2).padding([4, 8]);
    for stash in stashes {
        let msg = Message::Select(crate::selection::Selection::Stash {
            index: stash.index,
            message: stash.message.clone(),
        });
        rows = rows.push(branch_row(
            STASH_MARK,
            stash.message.clone(),
            msg,
            false,
            false,
            None,
            tokens,
            reading,
        ));
    }
    if stashes.is_empty() {
        rows = rows.push(empty_hint("No stashes", tokens, reading));
    }
    rows.into()
}

// ── TAGS ─────────────────────────────────────────────────────────────────────

fn tag_rows<'a>(tags: &'a [Tag], tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    let mut rows = column![].spacing(2).padding([4, 8]);
    for tag in tags {
        let msg = Message::Select(crate::selection::Selection::Tag {
            name: tag.name.clone(),
            commit_id: tag.commit_id.clone(),
        });
        rows = rows.push(branch_row(
            TAG_MARK,
            tag.name.clone(),
            msg,
            false,
            false,
            None,
            tokens,
            reading,
        ));
    }
    if tags.is_empty() {
        rows = rows.push(empty_hint("No tags", tokens, reading));
    }
    rows.into()
}

// ── Shared helpers ────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn branch_row<'a>(
    mark: &'static str,
    label: String,
    message: Message,
    selected: bool,
    accent: bool,
    context: Option<ContextTarget>,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let mark_color = if accent {
        tokens.accent_primary
    } else if selected {
        tokens.text_primary
    } else {
        tokens.text_muted
    };
    let label_color = if accent {
        tokens.accent_primary
    } else if selected {
        tokens.text_primary
    } else {
        tokens.text_secondary
    };
    let row = button(
        row![
            glyph(mark, reading.size, mark_color, &reading),
            text(label)
                .font(reading.font)
                .size(reading.size)
                .wrapping(Wrapping::None)
                .style(move |_| iced::widget::text::Style {
                    color: Some(label_color),
                }),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding([4, 8])
    .on_press(message)
    .style(move |_theme, status| {
        let hot = matches!(status, Status::Hovered | Status::Pressed);
        iced::widget::button::Style {
            background: Some(Background::Color(if selected || hot {
                tokens.border
            } else {
                Color::TRANSPARENT
            })),
            text_color: label_color,
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 4.0.into(),
            },
            shadow: Shadow::default(),
        }
    });
    if let Some(target) = context {
        mouse_area(row)
            .on_right_press(Message::OpenContextMenu { target })
            .into()
    } else {
        row.into()
    }
}

fn empty_hint<'a>(label: &'static str, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    text(label)
        .font(reading.font)
        .size(reading.size)
        .style(move |_| iced::widget::text::Style {
            color: Some(tokens.text_muted),
        })
        .into()
}
