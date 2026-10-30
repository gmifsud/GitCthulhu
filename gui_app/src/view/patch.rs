//! The Diff pane. Side by side by default, after the JetBrains diff viewer: line numbers in
//! each gutter, modified pairs on a blue wash with the changed words marked, added lines on
//! green, removed on red, and an empty filler opposite an unpaired line. Unified is one
//! column with both line numbers. The rows come from [`crate::diff_view::layout`].

use crate::app::{GitApp, Message, DIFF_SCROLL_ID};
use crate::diff_view::{self, Cell, Line, Side, FILE_HEIGHT, HUNK_HEIGHT, NOTE_HEIGHT, ROW_HEIGHT};
use crate::icons::Icon;
use crate::theme::DesignTokens;
use core_domain::{DiffLineKind, FileDisposition, Patch};
use iced::widget::{column, container, horizontal_space, row, scrollable, text, Space};
use iced::{Alignment, Background, Border, Color, Element, Length, Shadow, Theme};

use super::chrome::{command_button, glyph, Reading};

const SHOW_MORE_LABEL: &str = "Show more lines";
const LINE_NO_WIDTH: f32 = 44.0;
const MARK_WIDTH: f32 = 14.0;
const DIVIDER_WIDTH: f32 = 1.0;
const TEXT_PAD_X: f32 = 6.0;

pub(super) fn patch_pane<'a>(
    app: &'a GitApp,
    layout: &diff_view::Layout,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let session = app.session();
    if session.patch_pending {
        return note_pane("Loading…", tokens, reading);
    }
    if let Some(error) = &session.patch_error {
        return note_pane(error, tokens, reading);
    }
    if session.patches.is_empty() {
        return note_pane("Select a commit or a changed file to see its diff.", tokens, reading);
    }
    let busy = session.state.loading;
    // Stage and discard act on the working tree, so they belong to working-tree files only.
    let hunk_place = match session.selection.as_ref() {
        Some(crate::selection::Selection::File { place, .. }) => Some(*place),
        _ => None,
    };
    let mut body = diff_rows(layout, hunk_place, busy, tokens, reading);
    if layout.hidden > 0 {
        body = body.push(
            container(
                row![
                    command_button(SHOW_MORE_LABEL, Message::ShowMoreDiff, true, tokens, reading),
                    text(format!("{} more lines", layout.hidden))
                        .font(reading.font)
                        .size(reading.size)
                        .style(move |_| text::Style { color: Some(tokens.text_muted) }),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .padding([6, 8]),
        );
    }
    // Vertical only: iced panics if content fills the horizontal axis of a horizontally
    // scrolling scrollable. Long lines clip at the edge of their column.
    scrollable(body)
        .id(scrollable::Id::new(DIFF_SCROLL_ID))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// The rows of a laid-out diff. `hunk_place` adds Stage / Discard hunk buttons for a
/// working-tree file; `None` draws a read-only diff (commits, compare).
pub(super) fn diff_rows<'a>(
    layout: &diff_view::Layout,
    hunk_place: Option<FileDisposition>,
    busy: bool,
    tokens: DesignTokens,
    reading: Reading,
) -> iced::widget::Column<'a, Message> {
    let mut body = column![].width(Length::Fill);
    for line in &layout.lines {
        body = body.push(match line {
            Line::File { path } => file_row(path, tokens, reading),
            Line::Note { text } => note_row(text, NOTE_HEIGHT, tokens, reading),
            Line::Hunk { path, index, header } => hunk_row(path, *index, header, hunk_place, busy, tokens, reading),
            Line::Split { left, right } => split_row(left, right, tokens, reading),
            Line::Unified { old, new, kind, text } => unified_row(*old, *new, *kind, text, tokens, reading),
        });
    }
    body
}

fn note_pane<'a>(value: &str, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    container(note_row(value, NOTE_HEIGHT, tokens, reading)).padding([4, 8]).into()
}

fn note_row<'a>(value: &str, height: f32, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    container(
        text(value.to_string())
            .font(reading.font)
            .size(reading.size)
            .style(move |_| text::Style { color: Some(tokens.text_muted) }),
    )
    .height(Length::Fixed(height))
    .align_y(Alignment::Center)
    .padding([0.0, TEXT_PAD_X])
    .clip(true)
    .into()
}

fn wash(color: Color) -> impl Fn(&Theme) -> container::Style {
    move |_theme: &Theme| container::Style {
        background: Some(Background::Color(color)),
        text_color: None,
        border: Border::default(),
        shadow: Shadow::default(),
    }
}

fn file_row<'a>(path: &str, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    container(
        row![
            glyph(Icon::File, reading.size, tokens.text_muted, &reading),
            text(path.to_string())
                .font(reading.mono)
                .size(reading.size)
                .style(move |_| text::Style { color: Some(tokens.accent_primary) }),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    )
    .height(Length::Fixed(FILE_HEIGHT))
    .width(Length::Fill)
    .align_y(Alignment::Center)
    .padding([0.0, TEXT_PAD_X])
    .clip(true)
    .style(wash(tokens.bg_raised))
    .into()
}

#[allow(clippy::too_many_arguments)]
fn hunk_row<'a>(
    path: &str,
    index: usize,
    header: &str,
    place: Option<FileDisposition>,
    busy: bool,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let mut line = row![
        text(header.to_string())
            .font(reading.mono)
            .size((reading.size - 1.0).max(11.0))
            .style(move |_| text::Style { color: Some(tokens.text_muted) }),
        horizontal_space(),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    if let Some(place) = place {
        if place != FileDisposition::Staged {
            line = line.push(command_button(
                "Stage hunk",
                Message::StageHunk { path: path.to_string(), index },
                !busy,
                tokens,
                reading,
            ));
        }
        line = line.push(command_button(
            "Discard hunk",
            Message::DiscardHunk { path: path.to_string(), index },
            !busy,
            tokens,
            reading,
        ));
    }
    container(line)
        .height(Length::Fixed(HUNK_HEIGHT))
        .width(Length::Fill)
        .align_y(Alignment::Center)
        .padding([0.0, TEXT_PAD_X])
        .clip(true)
        .style(wash(tokens.bg_chrome))
        .into()
}

fn line_number<'a>(number: Option<u32>, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    container(
        text(number.map(|n| n.to_string()).unwrap_or_default())
            .font(reading.mono)
            .size((reading.size - 1.0).max(11.0))
            .style(move |_| text::Style { color: Some(tokens.text_muted) }),
    )
    .width(Length::Fixed(LINE_NO_WIDTH))
    .height(Length::Fixed(ROW_HEIGHT))
    .align_x(Alignment::End)
    .align_y(Alignment::Center)
    .padding([0.0, 6.0])
    .into()
}

/// Background of a side, and of its changed words.
fn cell_washes(cell: Cell, tokens: DesignTokens) -> (Color, Color) {
    match cell {
        Cell::Context => (Color::TRANSPARENT, Color::TRANSPARENT),
        Cell::Changed => (tokens.modified_bg, tokens.modified_word_bg),
        Cell::Added => (tokens.added_bg, tokens.added_word_bg),
        Cell::Removed => (tokens.removed_bg, tokens.removed_word_bg),
        Cell::Empty => (tokens.bg_sidebar, tokens.bg_sidebar),
    }
}

fn side_cell<'a>(side: &Side, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    let (line_wash, word_wash) = cell_washes(side.cell, tokens);
    let ink = if side.cell == Cell::Context { tokens.text_secondary } else { tokens.text_primary };
    // One text widget per piece, not `rich_text`: iced 0.13 ignores `Wrapping::None`, and a
    // rich paragraph broke at the first highlighted span, so the fixed-height row clipped
    // everything after the change.
    let mut pieces = row![].height(Length::Fixed(ROW_HEIGHT)).align_y(Alignment::Center);
    for piece in &side.pieces {
        let words = text(piece.text.clone())
            .font(reading.mono)
            .size(reading.size)
            .wrapping(iced::widget::text::Wrapping::None)
            .style(move |_| text::Style { color: Some(ink) });
        pieces = pieces.push(if piece.changed {
            container(words).style(wash(word_wash)).into()
        } else {
            Element::from(words)
        });
    }
    container(
        row![
            line_number(side.number, tokens, reading),
            container(pieces)
                .width(Length::Fill)
                .height(Length::Fixed(ROW_HEIGHT))
                .align_y(Alignment::Center)
                .clip(true),
        ]
        .align_y(Alignment::Center),
    )
    .width(Length::FillPortion(1))
    .height(Length::Fixed(ROW_HEIGHT))
    .clip(true)
    .style(wash(line_wash))
    .into()
}

fn split_row<'a>(left: &Side, right: &Side, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    row![
        side_cell(left, tokens, reading),
        container(Space::new(Length::Fixed(DIVIDER_WIDTH), Length::Fixed(ROW_HEIGHT))).style(wash(tokens.border)),
        side_cell(right, tokens, reading),
    ]
    .height(Length::Fixed(ROW_HEIGHT))
    .width(Length::Fill)
    .into()
}

fn unified_row<'a>(
    old: Option<u32>,
    new: Option<u32>,
    kind: DiffLineKind,
    value: &str,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let (mark, ink, line_wash) = match kind {
        DiffLineKind::Added => ("+", tokens.added, tokens.added_bg),
        DiffLineKind::Removed => ("-", tokens.removed, tokens.removed_bg),
        DiffLineKind::Context => (" ", tokens.text_secondary, Color::TRANSPARENT),
    };
    container(
        row![
            line_number(old, tokens, reading),
            line_number(new, tokens, reading),
            container(
                text(mark)
                    .font(reading.mono)
                    .size(reading.size)
                    .style(move |_| text::Style { color: Some(ink) }),
            )
            .width(Length::Fixed(MARK_WIDTH)),
            container(
                text(value.to_string())
                    .font(reading.mono)
                    .size(reading.size)
                    .wrapping(iced::widget::text::Wrapping::None)
                    .style(move |_| text::Style {
                        color: Some(if kind == DiffLineKind::Context { tokens.text_secondary } else { tokens.text_primary }),
                    }),
            )
            .width(Length::Fill)
            .height(Length::Fixed(ROW_HEIGHT))
            .align_y(Alignment::Center)
            .clip(true),
        ]
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .height(Length::Fixed(ROW_HEIGHT))
    .clip(true)
    .style(wash(line_wash))
    .into()
}

/// How a file changed in a commit, read from its hunks: a hunk from `-0,0` is a new file and
/// one to `+0,0` a deleted file. Git prints exactly those ranges for added and removed files.
pub(crate) fn patch_kind(patch: &Patch) -> core_domain::ChangeKind {
    use core_domain::ChangeKind;
    let header = patch.hunks.first().map(|hunk| hunk.header.as_str()).unwrap_or_default();
    if patch.hunks.len() == 1 && header.starts_with("@@ -0,0 ") {
        ChangeKind::Added
    } else if patch.hunks.len() == 1 && header.contains(" +0,0 @@") {
        ChangeKind::Deleted
    } else {
        ChangeKind::Modified
    }
}

/// Added and removed line counts in a unified patch (context lines ignored).
pub(crate) fn patch_stats(patch: &Patch) -> (usize, usize) {
    let mut added = 0;
    let mut removed = 0;
    for hunk in &patch.hunks {
        for line in &hunk.lines {
            match line.kind {
                DiffLineKind::Added => added += 1,
                DiffLineKind::Removed => removed += 1,
                DiffLineKind::Context => {}
            }
        }
    }
    (added, removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_domain::{DiffLine, Hunk};

    #[test]
    fn new_and_deleted_files_are_read_from_their_hunk_ranges() {
        use core_domain::ChangeKind;
        let with_header = |header: &str| Patch {
            path: "a".into(),
            binary: false,
            hunks: vec![Hunk { header: header.into(), lines: vec![] }],
        };
        assert_eq!(patch_kind(&with_header("@@ -0,0 +1,3 @@")), ChangeKind::Added);
        assert_eq!(patch_kind(&with_header("@@ -1,3 +0,0 @@")), ChangeKind::Deleted);
        assert_eq!(patch_kind(&with_header("@@ -4,2 +4,3 @@ fn")), ChangeKind::Modified);
    }

    #[test]
    fn patch_stats_count_added_and_removed() {
        let line = |kind, text: &str| DiffLine { kind, text: text.into() };
        let sample = Patch {
            path: "a.rs".into(),
            binary: false,
            hunks: vec![Hunk {
                header: "@@ -1 +1 @@".into(),
                lines: vec![
                    line(DiffLineKind::Context, "keep"),
                    line(DiffLineKind::Added, "new"),
                    line(DiffLineKind::Added, "also"),
                    line(DiffLineKind::Removed, "old"),
                ],
            }],
        };
        assert_eq!(patch_stats(&sample), (2, 1));
    }

    #[test]
    fn every_side_of_a_change_has_a_wash() {
        let tokens = DesignTokens::deep_focus();
        for cell in [Cell::Changed, Cell::Added, Cell::Removed, Cell::Empty] {
            assert_ne!(cell_washes(cell, tokens).0, Color::TRANSPARENT, "{cell:?}");
        }
        assert_eq!(cell_washes(Cell::Context, tokens).0, Color::TRANSPARENT);
    }
}
