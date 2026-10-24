use crate::app::{GitApp, Message};
use crate::changes::{change_rows, change_sections, marked_paths, ChangeRow};
use crate::selection::Selection;
use crate::theme::DesignTokens;
use core_domain::FileDisposition;
use iced::widget::button::Status;
use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Background, Border, Color, Element, Length, Shadow, Theme};

use super::chrome::{accent_button, command_button, panel_style, section_label, Reading};

pub(super) fn changes_pane<'a>(
    app: &'a GitApp,
    selection: &Selection,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let stageable = !marked_paths(&app.marked_files, true).is_empty();
    let unstageable = !marked_paths(&app.marked_files, false).is_empty();
    let chosen = app.marked_files.len();
    let mut changes = column![
        section_label(tokens, reading, "Changes"),
        row![
            text_input("Commit message", &app.commit_message)
                .on_input(Message::CommitMessageChanged)
                .padding(8)
                .width(Length::Fill),
            command_button(
                "Suggest",
                Message::SuggestCommit,
                !app.agent_busy
                    && !app.state.loading
                    && (!app.marked_files.is_empty()
                        || matches!(selection, Selection::File { .. })),
                tokens,
                reading,
            ),
        ]
        .spacing(6)
        .align_y(iced::Alignment::Center),
        row![
            if app.files_as_tree {
                command_button("List", Message::SetFileTree(false), true, tokens, reading)
            } else {
                accent_button("List", Message::SetFileTree(false), true, tokens, reading)
            },
            if app.files_as_tree {
                accent_button("Tree", Message::SetFileTree(true), true, tokens, reading)
            } else {
                command_button("Tree", Message::SetFileTree(true), true, tokens, reading)
            },
            command_button(
                "Select all",
                Message::SelectAllFiles,
                app.state.current_status.as_ref().is_some_and(|status| {
                    !status.staged_files.is_empty()
                        || !status.unstaged_files.is_empty()
                        || !status.untracked_files.is_empty()
                }),
                tokens,
                reading,
            ),
            command_button(
                "Clear",
                Message::ClearFileSelection,
                chosen > 0,
                tokens,
                reading,
            ),
            command_button(
                "Stage",
                Message::StageMarked,
                stageable && !app.state.loading,
                tokens,
                reading
            ),
            command_button(
                "Unstage",
                Message::UnstageMarked,
                unstageable && !app.state.loading,
                tokens,
                reading,
            ),
        ]
        .spacing(6),
        text(if chosen == 0 {
            "Nothing selected".to_string()
        } else if chosen == 1 {
            "1 selected".to_string()
        } else {
            format!("{chosen} selected")
        })
        .font(reading.font)
        .size((reading.size - 2.0).max(11.0))
        .style(move |_| text::Style {
            color: Some(tokens.text_muted),
        }),
    ]
    .spacing(8)
    .padding(12);

    match &app.state.current_status {
        None => {
            changes = changes.push(
                text("Not loaded yet")
                    .font(reading.font)
                    .size(reading.size)
                    .style(move |_| text::Style {
                        color: Some(tokens.text_muted),
                    }),
            )
        }
        Some(status) => {
            let sections = change_sections(status);
            if sections.is_empty() {
                changes = changes.push(
                    text("Working tree clean")
                        .font(reading.font)
                        .size(reading.size)
                        .style(move |_| text::Style {
                            color: Some(tokens.text_muted),
                        }),
                );
            }
            for section in sections {
                let mut files =
                    column![text(format!("{} ({})", section.title, section.files.len()))
                        .font(reading.font)
                        .size((reading.size - 2.0).max(11.0))
                        .style(move |_| text::Style {
                            color: Some(tokens.text_muted),
                        })]
                    .spacing(0);
                for row_item in change_rows(section.files, section.place, app.files_as_tree) {
                    files = files.push(change_line(&row_item, app, selection, tokens, reading));
                }
                changes = changes.push(files);
            }
        }
    }

    container(scrollable(changes))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_theme: &Theme| panel_style(tokens, true))
        .into()
}

fn change_line<'a>(
    row_item: &ChangeRow,
    app: &'a GitApp,
    selection: &Selection,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let depth = match row_item {
        ChangeRow::Directory { depth, .. } | ChangeRow::File { depth, .. } => *depth,
    };
    let row_size = (reading.size - 3.0).max(11.0);
    let indent = container(column![])
        .width(Length::Fixed(depth as f32 * 10.0))
        .height(Length::Fixed(1.0));
    match row_item {
        ChangeRow::Directory { name, .. } => row![
            indent,
            text(name.clone())
                .font(reading.font)
                .size(row_size)
                .line_height(1.0)
                .style(move |_| text::Style {
                    color: Some(tokens.text_muted),
                }),
        ]
        .into(),
        ChangeRow::File {
            name, path, place, ..
        } => {
            let marked = app
                .marked_files
                .iter()
                .any(|mark| &mark.path == path && mark.place == *place);
            let focused = matches!(
                selection,
                Selection::File { path: selected, place: selected_place }
                    if selected == path && selected_place == place
            );
            let mark = Message::ToggleFileMark {
                path: path.clone(),
                place: *place,
            };
            row![
                indent,
                compact_mark(
                    if marked { "–" } else { "+" },
                    mark,
                    marked,
                    tokens,
                    row_size
                ),
                compact_name(
                    name.clone(),
                    Message::FocusFile {
                        path: path.clone(),
                        place: *place,
                    },
                    focused || marked,
                    *place == FileDisposition::Staged,
                    tokens,
                    reading,
                    row_size,
                ),
            ]
            .spacing(2)
            .into()
        }
    }
}

fn compact_mark<'a>(
    label: &'a str,
    message: Message,
    marked: bool,
    tokens: DesignTokens,
    size: f32,
) -> Element<'a, Message> {
    button(text(label).size(size).line_height(1.0))
        .padding([1, 4])
        .on_press(message)
        .style(move |_theme, status| {
            let hot = matches!(status, Status::Hovered | Status::Pressed);
            iced::widget::button::Style {
                background: Some(Background::Color(if marked || hot {
                    tokens.border
                } else {
                    Color::TRANSPARENT
                })),
                text_color: if marked {
                    tokens.accent_primary
                } else {
                    tokens.text_muted
                },
                border: Border {
                    color: Color::TRANSPARENT,
                    width: 0.0,
                    radius: 2.0.into(),
                },
                shadow: Shadow::default(),
            }
        })
        .into()
}

fn compact_name<'a>(
    label: String,
    message: Message,
    selected: bool,
    staged: bool,
    tokens: DesignTokens,
    reading: Reading,
    size: f32,
) -> Element<'a, Message> {
    let color = if staged {
        tokens.accent_primary
    } else if selected {
        tokens.text_primary
    } else {
        tokens.text_secondary
    };
    button(
        text(label)
            .font(reading.mono)
            .size(size)
            .line_height(1.0)
            .wrapping(Wrapping::None),
    )
    .width(Length::Fill)
    .padding([1, 4])
    .on_press(message)
    .style(move |_theme, status| {
        let hot = matches!(status, Status::Hovered | Status::Pressed);
        iced::widget::button::Style {
            background: Some(Background::Color(if selected || hot {
                tokens.border
            } else {
                Color::TRANSPARENT
            })),
            text_color: color,
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 2.0.into(),
            },
            shadow: Shadow::default(),
        }
    })
    .into()
}
