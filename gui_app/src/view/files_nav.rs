//! Right inspector: working-tree staging stacked above commit file list.

use crate::app::{GitApp, Message};
use crate::changes::change_sections;
use crate::selection::{short_hash, Selection};
use crate::theme::DesignTokens;
use core_domain::RepoStatus;
use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Alignment, Background, Border, Color, Element, Length, Shadow};
use std::time::{SystemTime, UNIX_EPOCH};

use super::changes::wip_working_tree_block;
use super::chrome::{command_button, Reading};
use super::history::relative_age;
use super::patch::patch_stats;
use iced::widget::button::Status;

pub(crate) fn show_wip_block(status: Option<&RepoStatus>) -> bool {
    status.is_some_and(|status| !change_sections(status).is_empty())
}

pub(crate) fn show_commit_block(selection: &Selection) -> bool {
    matches!(selection, Selection::Commit { .. })
}

fn show_tag_block(selection: &Selection) -> bool {
    matches!(selection, Selection::Tag { .. })
}

pub(crate) fn files_navigator<'a>(
    app: &'a GitApp,
    selection: &Selection,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let status = app.session().state.current_status.as_ref();
    let wip = show_wip_block(status);
    let commit = show_commit_block(selection);
    let tag = show_tag_block(selection);

    if !wip && !commit && !tag {
        return empty_navigator_hint(tokens, reading);
    }

    let mut body = column![].spacing(8).width(Length::Fill);

    if wip {
        body = body.push(wip_working_tree_block(app, selection, tokens, reading));
    }

    if commit {
        if let Some(meta) = commit_meta(app, selection, tokens, reading) {
            body = body.push(meta);
        }
        body = body.push(commit_files_block(app, tokens, reading));
    } else if tag {
        if let Some(meta) = tag_meta(selection, tokens, reading) {
            body = body.push(meta);
        }
        body = body.push(commit_files_block(app, tokens, reading));
    }

    container(scrollable(body))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn empty_navigator_hint<'a>(tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    container(
        text("Select a commit or make a change.")
            .font(reading.font)
            .size(reading.size)
            .style(move |_| text::Style {
                color: Some(tokens.text_muted),
            }),
    )
    .padding([12, 12])
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn commit_meta<'a>(
    app: &'a GitApp,
    selection: &Selection,
    tokens: DesignTokens,
    reading: Reading,
) -> Option<Element<'a, Message>> {
    let Selection::Commit { id } = selection else {
        return None;
    };
    let commit = app.session().state.commits.iter().find(|commit| &commit.id == id)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(commit.timestamp);
    let meta = format!(
        "{} · {} · {}",
        short_hash(&commit.id),
        commit.author,
        relative_age(now, commit.timestamp)
    );
    Some(
        container(
            column![
                text(commit.message.clone())
                    .font(reading.font)
                    .size(reading.size)
                    .style(move |_| text::Style {
                        color: Some(tokens.text_primary),
                    }),
                text(meta)
                    .font(reading.font)
                    .size((reading.size - 2.0).max(11.0))
                    .style(move |_| text::Style {
                        color: Some(tokens.text_muted),
                    }),
            ]
            .spacing(4),
        )
        .padding([0, 12])
        .into(),
    )
}

fn tag_meta<'a>(
    selection: &Selection,
    tokens: DesignTokens,
    reading: Reading,
) -> Option<Element<'a, Message>> {
    let Selection::Tag { name, .. } = selection else {
        return None;
    };
    Some(
        container(
            text(name.clone())
                .font(reading.font)
                .size(reading.size)
                .style(move |_| text::Style {
                    color: Some(tokens.text_primary),
                }),
        )
        .padding([0, 12])
        .into(),
    )
}

pub(crate) fn commit_files_block<'a>(
    app: &'a GitApp,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let mut block = column![].spacing(2).padding([4, 12]);
    if app.session().patch_pending {
        return block
            .push(
                text("Loading files…")
                    .font(reading.font)
                    .size(reading.size)
                    .style(move |_| text::Style {
                        color: Some(tokens.text_muted),
                    }),
            )
            .into();
    }
    if let Some(error) = &app.session().patch_error {
        return block
            .push(
                text(error.clone())
                    .font(reading.font)
                    .size(reading.size)
                    .style(move |_| text::Style {
                        color: Some(tokens.accent_primary),
                    }),
            )
            .into();
    }
    if app.session().patches.is_empty() {
        return block
            .push(
                text("No files in this commit")
                    .font(reading.font)
                    .size(reading.size)
                    .style(move |_| text::Style {
                        color: Some(tokens.text_muted),
                    }),
            )
            .into();
    }

    let (total_added, total_removed) = app.session().patches.iter().fold((0, 0), |(a, r), patch| {
        let (added, removed) = patch_stats(patch);
        (a + added, r + removed)
    });
    let title = format!(
        "Files changed ({})  +{total_added} −{total_removed}",
        app.session().patches.len()
    );
    let mut heading = row![text(title)
        .font(reading.font)
        .size((reading.size - 2.0).max(11.0))
        .style(move |_| text::Style {
            color: Some(tokens.text_muted),
        })]
    .spacing(8)
    .align_y(Alignment::Center);
    if app.session().focused_diff_path.is_some() {
        heading = heading.push(iced::widget::horizontal_space()).push(
            command_button("All files", Message::ClearDiffFocus, true, tokens, reading),
        );
    }
    block = block.push(heading);

    for patch in &app.session().patches {
        let (added, removed) = patch_stats(patch);
        let focused = app.session().focused_diff_path.as_deref() == Some(patch.path.as_str());
        let stats = if patch.binary {
            "binary".to_string()
        } else {
            format!("+{added} −{removed}")
        };
        let path = patch.path.clone();
        block = block.push(
            button(
                row![
                    text(patch.path.clone())
                        .font(reading.mono)
                        .size((reading.size - 2.0).max(11.0))
                        .wrapping(Wrapping::None)
                        .width(Length::Fill),
                    text(stats)
                        .font(reading.mono)
                        .size((reading.size - 2.0).max(11.0))
                        .style(move |_| text::Style {
                            color: Some(if added > 0 && removed == 0 {
                                tokens.added
                            } else if removed > 0 && added == 0 {
                                tokens.removed
                            } else {
                                tokens.text_muted
                            }),
                        }),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .padding([2, 4])
            .width(Length::Fill)
            .on_press(Message::FocusDiffPath(path))
            .style(move |_theme, status| {
                let hot = matches!(status, Status::Hovered | Status::Pressed);
                iced::widget::button::Style {
                    background: Some(Background::Color(if focused || hot {
                        tokens.border
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

    block.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_domain::RepoStatus;

    fn clean_status() -> RepoStatus {
        RepoStatus {
            staged_files: vec![],
            unstaged_files: vec![],
            untracked_files: vec![],
            tracking: None,
            head_commit_id: None,
        }
    }

    fn dirty_status() -> RepoStatus {
        RepoStatus {
            staged_files: vec![],
            unstaged_files: vec!["a.rs".into()],
            untracked_files: vec![],
            tracking: None,
            head_commit_id: None,
        }
    }

    #[test]
    fn wip_block_hidden_when_clean() {
        assert!(!show_wip_block(None));
        assert!(!show_wip_block(Some(&clean_status())));
    }

    #[test]
    fn wip_block_shown_when_dirty() {
        assert!(show_wip_block(Some(&dirty_status())));
    }

    #[test]
    fn commit_block_for_commit_selection_only() {
        assert!(show_commit_block(&Selection::Commit {
            id: "abc".into()
        }));
        assert!(!show_commit_block(&Selection::Branch {
            name: "main".into(),
            current: true,
        }));
    }
}
