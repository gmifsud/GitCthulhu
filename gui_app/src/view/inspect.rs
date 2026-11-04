//! The Inspector while another worktree is inspected (a single click on its WIP row, its
//! Explorer row or its Quick open entry). It is read-only: stage, unstage and commit live
//! only on the active tab, so a glance at another worktree can never change it.

use crate::app::{GitApp, Message};
use crate::changes::change_sections;
use crate::selection::Selection;
use crate::theme::DesignTokens;
use crate::wip::is_dirty;
use core_domain::RepoStatus;
use iced::widget::{column, container, horizontal_space, row, scrollable, text};
use iced::{Alignment, Element, Length};

use super::chrome::{command_button, glyph, hairline, one_line, Reading};
use super::explorer::label_text;
use super::files_nav::files_navigator;
use crate::icons::Icon;

const HEAD_PADDING: [u16; 2] = [4, 8];
const BODY_PADDING: [u16; 2] = [6, 12];

pub(super) fn inspected_worktree<'a>(
    app: &'a GitApp,
    path: &'a str,
    selection: &Selection,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let title = one_line(
        text(format!("Inspecting {}", app.worktree_label(path)))
            .font(reading.font)
            .size(reading.size)
            .style(move |_| text::Style { color: Some(tokens.accent_primary) }),
        reading.size,
        Length::Shrink,
    );
    let head = row![
        glyph(Icon::Worktree, reading.size, tokens.accent_primary, &reading),
        container(title).width(Length::Fill).clip(true),
        horizontal_space().width(Length::Shrink),
        command_button("Open in tab", Message::OpenWorktree(path.to_string()), true, tokens, reading),
        command_button("Back", Message::ClearInspect, true, tokens, reading),
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    let status = app.worktree_status(path);
    let note = if status.is_some_and(is_dirty) {
        "Read-only. Open it in a tab to stage or commit here."
    } else {
        "Clean. Showing its branch tip; open it in a tab to work here."
    };
    let body: Element<'a, Message> = match status {
        Some(status) if is_dirty(status) => changes_list(status, tokens, reading),
        _ if matches!(selection, Selection::Commit { .. }) => files_navigator(app, selection, tokens, reading),
        _ => container(label_text(
            "Its tip is not in the loaded history.",
            tokens.text_muted,
            reading,
        ))
        .padding(BODY_PADDING)
        .into(),
    };
    column![
        container(head).padding(HEAD_PADDING).width(Length::Fill),
        container(label_text(note, tokens.text_muted, reading)).padding([0, 8]),
        hairline(tokens),
        body,
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

/// Staged, Changes, Unversioned Files and Merge Conflicts as plain lists, with no actions.
fn changes_list<'a>(status: &'a RepoStatus, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    let mut items: Vec<Element<'a, Message>> = Vec::new();
    let mut section = |title: &str, files: &'a [String], ink: iced::Color| {
        items.push(label_text(&format!("{title} · {}", files.len()), tokens.text_secondary, reading));
        for file in files {
            items.push(container(label_text(file, ink, reading)).padding([0, 12]).into());
        }
    };
    if !status.conflicted_files.is_empty() {
        section("Merge Conflicts", &status.conflicted_files, tokens.removed);
    }
    for change in change_sections(status) {
        section(change.title, change.files, tokens.text_primary);
    }
    let lines = iced::widget::Column::with_children(items).spacing(2).padding(BODY_PADDING);
    container(scrollable(lines).width(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
