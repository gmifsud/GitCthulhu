use std::collections::HashSet;

use crate::app::{GitApp, Message};
use crate::changes::{change_rows, change_sections, marked_paths, ChangeRow};
use crate::selection::Selection;
use crate::theme::DesignTokens;
use core_domain::FileDisposition;
use iced::widget::button::Status;
use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Background, Border, Color, Element, Length, Shadow, Theme};

use super::chrome::{
    accent_button, command_button, field_style, fold_mark, glyph, panel_style, section_label,
    Reading,
};

/// Rows drawn per section. Each row is two buttons, so a huge untracked set would stall layout.
/// "Select all" still covers every file; only the drawing is bounded.
const FILE_ROWS_SHOWN: usize = 500;

pub(super) fn changes_pane<'a>(
    app: &'a GitApp,
    selection: &Selection,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let stageable = !marked_paths(&app.session().marked_files, true).is_empty();
    let unstageable = !marked_paths(&app.session().marked_files, false).is_empty();
    let chosen = app.session().marked_files.len();
    let mut changes = column![
        section_label(tokens, reading, "Changes"),
        row![
            text_input("Commit message", &app.session().commit_message)
                .on_input(Message::CommitMessageChanged)
                .padding(6)
                .width(Length::Fill)
                .style(field_style(tokens)),
            command_button(
                "Suggest",
                Message::SuggestCommit,
                !app.agent_busy
                    && !app.session().state.loading
                    && (!app.session().marked_files.is_empty()
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
                app.session().state.current_status.as_ref().is_some_and(|status| {
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
                stageable && !app.session().state.loading,
                tokens,
                reading
            ),
            command_button(
                "Unstage",
                Message::UnstageMarked,
                unstageable && !app.session().state.loading,
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
    .spacing(4)
    .padding(6);

    match &app.session().state.current_status {
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
                let folded = app.session().collapsed_sections.contains(&section.place);
                let mut files = column![section_header(
                    section.title,
                    section.files.len(),
                    folded,
                    section.place,
                    tokens,
                    reading,
                )]
                .spacing(0);
                if !folded {
                    let collapsed = collapsed_prefixes(app, section.place);
                    let rows =
                        change_rows(section.files, section.place, app.files_as_tree, &collapsed);
                    let hidden = rows.len().saturating_sub(FILE_ROWS_SHOWN);
                    for row_item in rows.iter().take(FILE_ROWS_SHOWN) {
                        files = files.push(change_line(
                            row_item,
                            section.place,
                            app,
                            selection,
                            tokens,
                            reading,
                        ));
                    }
                    if hidden > 0 {
                        files = files.push(
                            text(format!("… and {hidden} more"))
                                .font(reading.font)
                                .size((reading.size - 2.0).max(11.0))
                                .style(move |_| text::Style {
                                    color: Some(tokens.text_muted),
                                }),
                        );
                    }
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

fn collapsed_prefixes(app: &GitApp, place: FileDisposition) -> HashSet<String> {
    app.session().collapsed_dirs
        .iter()
        .filter(|(entry_place, _)| *entry_place == place)
        .map(|(_, prefix)| prefix.clone())
        .collect()
}

fn section_header<'a>(
    title: &'static str,
    count: usize,
    folded: bool,
    place: FileDisposition,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let size = (reading.size - 2.0).max(11.0);
    button(
        row![
            glyph(fold_mark(folded), size, tokens.text_muted, &reading),
            text(format!("{title} ({count})"))
                .font(reading.font)
                .size(size)
                .style(move |_| text::Style {
                    color: Some(tokens.text_muted),
                }),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    )
    .padding([2, 0])
    .width(Length::Fill)
    .on_press(Message::ToggleChangeSection(place))
    .style(move |_theme, status| fold_button_style(tokens, status))
    .into()
}

fn change_line<'a>(
    row_item: &ChangeRow,
    place: FileDisposition,
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
        ChangeRow::Directory {
            name,
            prefix,
            hidden,
            ..
        } => {
            let folded = *hidden > 0;
            let label = if folded {
                format!("{name} ({hidden})")
            } else {
                name.clone()
            };
            row![
                indent,
                button(
                    row![
                        glyph(fold_mark(folded), row_size, tokens.text_muted, &reading),
                        text(label)
                            .font(reading.font)
                            .size(row_size)
                            .line_height(1.0)
                            .style(move |_| text::Style {
                                color: Some(tokens.text_secondary),
                            }),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                )
                .padding([1, 4])
                .on_press(Message::ToggleChangeDir {
                    prefix: prefix.clone(),
                    place,
                })
                .style(move |_theme, status| fold_button_style(tokens, status)),
            ]
            .into()
        }
        ChangeRow::File {
            name, path, place, ..
        } => {
            let marked = app
                .session()
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

fn fold_button_style(
    tokens: DesignTokens,
    status: Status,
) -> iced::widget::button::Style {
    let hot = matches!(status, Status::Hovered | Status::Pressed);
    iced::widget::button::Style {
        background: Some(Background::Color(if hot {
            tokens.border
        } else {
            Color::TRANSPARENT
        })),
        text_color: tokens.text_muted,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 2.0.into(),
        },
        shadow: Shadow::default(),
    }
}

fn compact_mark<'a>(
    label: &'a str,
    message: Message,
    marked: bool,
    tokens: DesignTokens,
    size: f32,
) -> Element<'a, Message> {
    button(
        text(label)
            .size(size)
            .line_height(1.0)
            .style(move |_| text::Style {
                color: Some(if marked {
                    tokens.accent_primary
                } else {
                    tokens.text_muted
                }),
            }),
    )
    .padding([1, 5])
    .on_press(message)
    .style(move |_theme, status| {
        let hot = matches!(status, Status::Hovered | Status::Pressed);
        iced::widget::button::Style {
            background: Some(Background::Color(if marked || hot {
                tokens.chip
            } else {
                Color::TRANSPARENT
            })),
            text_color: if marked {
                tokens.accent_primary
            } else {
                tokens.text_muted
            },
            border: Border {
                color: if marked || hot {
                    tokens.chip_border
                } else {
                    Color::TRANSPARENT
                },
                width: 1.0,
                radius: 3.0.into(),
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
