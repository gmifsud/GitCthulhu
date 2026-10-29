//! The working tree, after the JetBrains Commit tool window: a toolbar on top, the changed
//! files in the middle (the only part that scrolls), the commit message and Commit at the
//! bottom. File names are coloured by how they changed.

use std::collections::HashSet;

use crate::app::{GitApp, Message};
use crate::changes::{change_rows, change_sections, marked_paths, ChangeRow, ChangeSection};
use crate::icons::Icon;
use crate::jobs::WorktreeEdit;
use crate::selection::Selection;
use crate::theme::DesignTokens;
use core_domain::FileDisposition;
use iced::widget::button::Status;
use iced::widget::{button, column, container, horizontal_space, mouse_area, row, scrollable, text, text_input, Space};
use iced::{Alignment, Background, Border, Color, Element, Length, Shadow};

use super::chrome::{field_style, glyph, hairline, icon_button, labelled_button, one_line, Reading};
use super::commands::can_commit;

/// Rows drawn per section. Each row is several widgets, so a huge untracked set would stall
/// layout. "Select all" still covers every file; only the drawing is bounded.
const FILE_ROWS_SHOWN: usize = 500;
const INDENT: f32 = 14.0;
const CHECK_SIZE: f32 = 14.0;
const ROW_PAD: [f32; 2] = [1.0, 4.0];
const PANEL_PAD: [f32; 2] = [4.0, 8.0];

fn selection_has_suggestable_files(app: &GitApp, selection: &Selection) -> bool {
    if !app.session().marked_files.is_empty() {
        return true;
    }
    if matches!(selection, Selection::File { .. }) {
        return true;
    }
    app.session()
        .state
        .current_status
        .as_ref()
        .is_some_and(|status| {
            !status.staged_files.is_empty()
                || !status.unstaged_files.is_empty()
                || !status.untracked_files.is_empty()
        })
}

/// Toolbar, scrolling file list, commit box. Fills the height it is given.
pub(crate) fn wip_working_tree_block<'a>(
    app: &'a GitApp,
    selection: &Selection,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let status = app.session().state.current_status.as_ref();
    column![
        container(wip_toolbar(app, tokens, reading)).padding(PANEL_PAD),
        hairline(tokens),
        container(scrollable(wip_file_sections(app, selection, status, tokens, reading)).height(Length::Fill))
            .height(Length::Fill)
            .width(Length::Fill),
        hairline(tokens),
        container(commit_box(app, selection, tokens, reading)).padding(PANEL_PAD),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn wip_toolbar<'a>(app: &'a GitApp, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    let session = app.session();
    let busy = session.state.loading;
    let stageable = !marked_paths(&session.marked_files, true).is_empty();
    let unstageable = !marked_paths(&session.marked_files, false).is_empty();
    let chosen = session.marked_files.len();
    let any = session.state.current_status.as_ref().is_some_and(|status| {
        !status.staged_files.is_empty() || !status.unstaged_files.is_empty() || !status.untracked_files.is_empty()
    });
    let tool = |icon: Icon, title: &'static str, key: &'static str, message: Message, enabled: bool, on: bool| {
        icon_button(icon, title, key, app.tip_ready(key), message, enabled, on, tokens, reading)
    };
    row![
        tool(Icon::ListFlat, "Show as list", "changes-list", Message::SetFileTree(false), true, !app.files_as_tree),
        tool(Icon::ListTree, "Group by folder", "changes-tree", Message::SetFileTree(true), true, app.files_as_tree),
        tool(Icon::CheckAll, "Select all", "changes-select-all", Message::SelectAllFiles, any, false),
        tool(Icon::ClearAll, "Clear selection", "changes-clear", Message::ClearFileSelection, chosen > 0, false),
        tool(Icon::Add, "Stage selected", "changes-stage", Message::StageMarked, stageable && !busy, false),
        tool(Icon::Remove, "Unstage selected", "changes-unstage", Message::UnstageMarked, unstageable && !busy, false),
        horizontal_space(),
        label(
            &match chosen {
                0 => String::new(),
                1 => "1 selected".to_string(),
                n => format!("{n} selected"),
            },
            tokens.text_muted,
            reading,
        ),
    ]
    .spacing(4)
    .align_y(Alignment::Center)
    .into()
}

/// Commit message, Suggest, and Commit. Enter commits when something is staged.
fn commit_box<'a>(app: &'a GitApp, selection: &Selection, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    let session = app.session();
    let ready = can_commit(app) && !session.state.loading;
    let commit = Message::EditWorktree(WorktreeEdit::Commit(session.commit_message.clone()));
    let mut input = text_input("Commit message", &session.commit_message)
        .on_input(Message::CommitMessageChanged)
        .font(reading.font)
        .size(reading.size)
        .padding(6)
        .width(Length::Fill)
        .style(field_style(tokens));
    if ready {
        input = input.on_submit(commit.clone());
    }
    row![
        input,
        icon_button(
            Icon::Sparkle,
            "Suggest commit message",
            "changes-suggest",
            app.tip_ready("changes-suggest"),
            Message::SuggestCommit,
            !app.agent_busy && !session.state.loading && selection_has_suggestable_files(app, selection),
            false,
            tokens,
            reading,
        ),
        labelled_button(Icon::Check, "Commit".to_string(), commit, ready, true, tokens, reading),
    ]
    .spacing(6)
    .align_y(Alignment::Center)
    .into()
}

fn wip_file_sections<'a>(
    app: &'a GitApp,
    selection: &Selection,
    status: Option<&core_domain::RepoStatus>,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let mut sections_col = column![].spacing(2).padding(PANEL_PAD).width(Length::Fill);
    match status {
        None => sections_col = sections_col.push(label("Not loaded yet", tokens.text_muted, reading)),
        Some(status) => {
            let sections = change_sections(status);
            if sections.is_empty() {
                sections_col = sections_col.push(label("Working tree clean", tokens.text_muted, reading));
            }
            for section in sections {
                sections_col = sections_col.push(section_block(app, selection, status, section, tokens, reading));
            }
        }
    }
    sections_col.into()
}

fn section_block<'a>(
    app: &'a GitApp,
    selection: &Selection,
    status: &core_domain::RepoStatus,
    section: ChangeSection<'_>,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let folded = app.session().collapsed_sections.contains(&section.place);
    let mut files = column![section_header(section.title, section.files.len(), folded, section.place, tokens, reading)]
        .spacing(0)
        .width(Length::Fill);
    if !folded {
        let collapsed = collapsed_prefixes(app, section.place);
        let rows = change_rows(section.files, section.place, app.files_as_tree, &collapsed);
        let hidden = rows.len().saturating_sub(FILE_ROWS_SHOWN);
        for row_item in rows.iter().take(FILE_ROWS_SHOWN) {
            files = files.push(change_line(row_item, section.place, status, app, selection, tokens, reading));
        }
        if hidden > 0 {
            files = files.push(label(&format!("… and {hidden} more"), tokens.text_muted, reading));
        }
    }
    files.into()
}

fn collapsed_prefixes(app: &GitApp, place: FileDisposition) -> HashSet<String> {
    app.session()
        .collapsed_dirs
        .iter()
        .filter(|(entry_place, _)| *entry_place == place)
        .map(|(_, prefix)| prefix.clone())
        .collect()
}

fn count_label(count: usize) -> String {
    if count == 1 {
        "1 file".to_string()
    } else {
        format!("{count} files")
    }
}

/// "▾ Changes  26 files".
fn section_header<'a>(
    title: &'static str,
    count: usize,
    folded: bool,
    place: FileDisposition,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    button(
        row![
            glyph(Icon::fold(!folded), reading.size, tokens.text_muted, &reading),
            label(title, tokens.text_primary, reading),
            label(&count_label(count), tokens.text_muted, reading),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    )
    .padding([3.0, 2.0])
    .width(Length::Fill)
    .on_press(Message::ToggleChangeSection(place))
    .style(move |_theme, status| row_style(tokens, status, false))
    .into()
}

/// One line of text that never wraps, whatever the panel width.
fn label<'a>(value: &str, color: Color, reading: Reading) -> Element<'a, Message> {
    one_line(
        text(value.to_string())
            .font(reading.font)
            .size(reading.size)
            .style(move |_| text::Style { color: Some(color) }),
        reading.size,
        Length::Shrink,
    )
}

fn change_line<'a>(
    row_item: &ChangeRow,
    place: FileDisposition,
    status: &core_domain::RepoStatus,
    app: &'a GitApp,
    selection: &Selection,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let depth = match row_item {
        ChangeRow::Directory { depth, .. } | ChangeRow::File { depth, .. } => *depth,
    };
    let indent = Space::with_width(Length::Fixed(INDENT * (depth as f32 + 1.0)));
    match row_item {
        ChangeRow::Directory { name, prefix, hidden, files, .. } => {
            let folded = *hidden > 0;
            let target = crate::context_menu::ContextTarget::ChangePath { path: prefix.clone(), place, is_dir: true };
            mouse_area(
                button(
                    row![
                        indent,
                        glyph(Icon::fold(!folded), reading.size, tokens.text_muted, &reading),
                        glyph(if folded { Icon::Folder } else { Icon::FolderOpened }, reading.size, tokens.text_muted, &reading),
                        label(name, tokens.text_secondary, reading),
                        label(&count_label(*files), tokens.text_muted, reading),
                    ]
                    .spacing(5)
                    .align_y(Alignment::Center),
                )
                .padding(ROW_PAD)
                .width(Length::Fill)
                .on_press(Message::ToggleChangeDir { prefix: prefix.clone(), place })
                .style(move |_theme, status| row_style(tokens, status, false)),
            )
            .on_right_press(Message::OpenContextMenu { target })
            .into()
        }
        ChangeRow::File { name, path, place, .. } => {
            let session = app.session();
            let marked = session.marked_files.iter().any(|mark| &mark.path == path && mark.place == *place);
            let focused = matches!(
                selection,
                Selection::File { path: selected, place: selected_place }
                    if selected == path && selected_place == place
            );
            let kind = status.kinds.of(*place, path);
            // A list shows "name  folder"; a tree already shows the folder above.
            let (file_name, folder) = if app.files_as_tree {
                (name.as_str(), "")
            } else {
                name.rsplit_once('/').map_or((name.as_str(), ""), |(folder, file)| (file, folder))
            };
            let mut line = row![
                indent,
                check_box(marked, Message::ToggleFileMark { path: path.clone(), place: *place }, tokens, &reading),
                glyph(Icon::File, reading.size, tokens.text_muted, &reading),
                label(file_name, tokens.change_ink(kind), reading),
            ]
            .spacing(5)
            .align_y(Alignment::Center);
            if !folder.is_empty() {
                line = line.push(label(folder, tokens.text_muted, reading));
            }
            let target = crate::context_menu::ContextTarget::ChangePath { path: path.clone(), place: *place, is_dir: false };
            mouse_area(
                button(line)
                    .padding(ROW_PAD)
                    .width(Length::Fill)
                    .on_press(Message::FocusFile { path: path.clone(), place: *place })
                    .style(move |_theme, status| row_style(tokens, status, focused)),
            )
            .on_right_press(Message::OpenContextMenu { target })
            .into()
        }
    }
}

/// The include-in-commit box at the start of a file row.
fn check_box<'a>(marked: bool, message: Message, tokens: DesignTokens, reading: &Reading) -> Element<'a, Message> {
    let mark: Element<'a, Message> = if marked {
        glyph(Icon::Check, CHECK_SIZE - 3.0, tokens.bg_app, reading)
    } else {
        Space::new(Length::Fixed(CHECK_SIZE - 3.0), Length::Fixed(CHECK_SIZE - 3.0)).into()
    };
    button(container(mark).center(Length::Fixed(CHECK_SIZE)))
        .padding(0)
        .on_press(message)
        .style(move |_theme, status| {
            let hot = matches!(status, Status::Hovered | Status::Pressed);
            iced::widget::button::Style {
                background: Some(Background::Color(if marked { tokens.accent_primary } else { Color::TRANSPARENT })),
                text_color: tokens.bg_app,
                border: Border {
                    color: if marked {
                        tokens.accent_primary
                    } else if hot {
                        tokens.text_secondary
                    } else {
                        tokens.chip_border
                    },
                    width: 1.0,
                    radius: 3.0.into(),
                },
                shadow: Shadow::default(),
            }
        })
        .into()
}

fn row_style(tokens: DesignTokens, status: Status, selected: bool) -> iced::widget::button::Style {
    let hot = matches!(status, Status::Hovered | Status::Pressed);
    iced::widget::button::Style {
        background: Some(Background::Color(if selected {
            tokens.border
        } else if hot {
            tokens.bg_raised
        } else {
            Color::TRANSPARENT
        })),
        text_color: tokens.text_secondary,
        border: Border { radius: 3.0.into(), ..Border::default() },
        shadow: Shadow::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_read_as_files() {
        assert_eq!(count_label(1), "1 file");
        assert_eq!(count_label(26), "26 files");
    }

    #[test]
    fn untracked_files_take_the_unversioned_colour() {
        let tokens = DesignTokens::deep_focus();
        let kinds = core_domain::ChangeKinds::default();
        assert_eq!(tokens.change_ink(kinds.of(FileDisposition::Untracked, "a")), tokens.vcs_untracked);
        assert_eq!(tokens.change_ink(kinds.of(FileDisposition::Unstaged, "a")), tokens.vcs_modified);
    }
}
