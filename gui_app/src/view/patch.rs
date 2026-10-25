use crate::app::{GitApp, Message};
use crate::theme::DesignTokens;
use core_domain::{DiffLine, DiffLineKind, Patch};
use iced::widget::text::Wrapping;
use iced::widget::{column, container, row, scrollable, text};
use iced::{Alignment, Background, Border, Color, Element, Length, Shadow, Theme};

use super::chrome::{command_button, Reading};

pub(super) fn patch_pane<'a>(
    app: &'a GitApp,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let mut body = column![].spacing(4).padding([4, 8]);
    if app.session().patch_pending {
        body = body.push(note("Loading…", tokens, reading));
    } else if let Some(error) = &app.session().patch_error {
        body = body.push(note(error, tokens, reading));
    } else if app.session().patches.is_empty() {
        body = body.push(note("No changes", tokens, reading));
    } else {
        let busy = app.session().state.loading;
        let mut budget = app.session().diff_lines_limit;
        let patches = visible_patches(&app.session().patches, app.session().focused_diff_path.as_deref());
        let hidden = {
            let total: usize = patches
                .iter()
                .flat_map(|patch| &patch.hunks)
                .map(|hunk| hunk.lines.len())
                .sum();
            total.saturating_sub(app.session().diff_lines_limit)
        };
        if app.session().focused_diff_path.is_some() && patches.len() < app.session().patches.len() {
            body = body.push(
                row![
                    command_button(
                        "All files",
                        Message::ClearDiffFocus,
                        true,
                        tokens,
                        reading,
                    ),
                    note(
                        &format!("Showing 1 of {}", app.session().patches.len()),
                        tokens,
                        reading,
                    ),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            );
        }
        'patches: for patch in patches {
            body = body.push(
                text(patch.path.clone())
                    .font(reading.mono)
                    .size(reading.size)
                    .wrapping(Wrapping::None)
                    .style(move |_| text::Style {
                        color: Some(tokens.accent_primary),
                    }),
            );
            if patch.binary {
                body = body.push(note("Binary file", tokens, reading));
                continue;
            }
            for (index, hunk) in patch.hunks.iter().enumerate() {
                let path = patch.path.clone();
                body = body.push(
                    text(hunk.header.clone())
                        .font(reading.mono)
                        .size((reading.size - 2.0).max(11.0))
                        .wrapping(Wrapping::None)
                        .style(move |_| text::Style {
                            color: Some(tokens.text_muted),
                        }),
                );
                let numbered = number_hunk_lines(hunk);
                for (old_no, new_no, line) in numbered {
                    if budget == 0 {
                        break 'patches;
                    }
                    budget -= 1;
                    body = body.push(diff_line(line, old_no, new_no, tokens, reading));
                }
                body = body.push(
                    row![
                        command_button(
                            "Stage hunk",
                            Message::StageHunk {
                                path: path.clone(),
                                index,
                            },
                            !busy,
                            tokens,
                            reading,
                        ),
                        command_button(
                            "Discard hunk",
                            Message::DiscardHunk { path, index },
                            !busy,
                            tokens,
                            reading,
                        ),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                );
            }
        }
        if hidden > 0 {
            body = body.push(
                row![
                    command_button(
                        SHOW_MORE_LABEL,
                        Message::ShowMoreDiff,
                        true,
                        tokens,
                        reading
                    ),
                    note(&format!("{hidden} more lines"), tokens, reading),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            );
        }
    }

    // Vertical only: iced panics if content fills the horizontal axis of a
    // horizontally scrolling scrollable. Long lines stay clipped via Wrapping::None.
    container(scrollable(body))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// Diff lines in `patches` beyond the first `limit`.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn hidden_lines(patches: &[Patch], limit: usize) -> usize {
    let total: usize = patches
        .iter()
        .flat_map(|patch| &patch.hunks)
        .map(|hunk| hunk.lines.len())
        .sum();
    total.saturating_sub(limit)
}

/// Patches to draw, optionally narrowed to one path.
pub(crate) fn visible_patches<'a>(
    patches: &'a [Patch],
    focus: Option<&str>,
) -> Vec<&'a Patch> {
    match focus {
        Some(path) => patches.iter().filter(|patch| patch.path == path).collect(),
        None => patches.iter().collect(),
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

const SHOW_MORE_LABEL: &str = "Show more lines";
const LINE_NO_WIDTH: f32 = 40.0;

/// Old/new start (and optional counts) from a unified hunk header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HunkRange {
    pub old_start: u32,
    pub new_start: u32,
}

/// Parses `@@ -a,b +c,d @@` and the short forms `@@ -a +c @@`, `@@ -1 +0,0 @@`.
pub(crate) fn parse_hunk_header(header: &str) -> Option<HunkRange> {
    let trimmed = header.trim();
    let rest = trimmed.strip_prefix("@@")?;
    let rest = rest.trim_start();
    let (old_part, after_old) = rest.split_once('+')?;
    let old_part = old_part.trim();
    let old_part = old_part.strip_prefix('-')?;
    let new_part = after_old
        .split_whitespace()
        .next()?
        .trim_end_matches('@')
        .trim();
    let old_start = parse_side(old_part)?;
    let new_start = parse_side(new_part)?;
    Some(HunkRange {
        old_start,
        new_start,
    })
}

fn parse_side(part: &str) -> Option<u32> {
    let start = part.split(',').next()?.trim();
    start.parse().ok()
}

/// `(old_line, new_line, line)` for each hunk line. Missing sides are `None`.
pub(crate) fn number_hunk_lines(
    hunk: &core_domain::Hunk,
) -> Vec<(Option<u32>, Option<u32>, &DiffLine)> {
    let range = parse_hunk_header(&hunk.header).unwrap_or(HunkRange {
        old_start: 1,
        new_start: 1,
    });
    let mut old = range.old_start;
    let mut new = range.new_start;
    let mut out = Vec::with_capacity(hunk.lines.len());
    for line in &hunk.lines {
        match line.kind {
            DiffLineKind::Context => {
                out.push((Some(old), Some(new), line));
                old = old.saturating_add(1);
                new = new.saturating_add(1);
            }
            DiffLineKind::Removed => {
                out.push((Some(old), None, line));
                old = old.saturating_add(1);
            }
            DiffLineKind::Added => {
                out.push((None, Some(new), line));
                new = new.saturating_add(1);
            }
        }
    }
    out
}

fn format_line_no(number: Option<u32>) -> String {
    match number {
        Some(n) if n > 0 => format!("{n}"),
        _ => String::new(),
    }
}

fn diff_line<'a>(
    line: &DiffLine,
    old_no: Option<u32>,
    new_no: Option<u32>,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let (mark, color, tint) = match line.kind {
        DiffLineKind::Added => ("+", tokens.added, tokens.added_bg),
        DiffLineKind::Removed => ("-", tokens.removed, tokens.removed_bg),
        DiffLineKind::Context => (" ", tokens.text_secondary, Color::TRANSPARENT),
    };
    let gutter = tokens.text_muted;
    container(
        row![
            text(format_line_no(old_no))
                .font(reading.mono)
                .size((reading.size - 2.0).max(11.0))
                .width(Length::Fixed(LINE_NO_WIDTH))
                .wrapping(Wrapping::None)
                .style(move |_| text::Style {
                    color: Some(gutter),
                }),
            text(format_line_no(new_no))
                .font(reading.mono)
                .size((reading.size - 2.0).max(11.0))
                .width(Length::Fixed(LINE_NO_WIDTH))
                .wrapping(Wrapping::None)
                .style(move |_| text::Style {
                    color: Some(gutter),
                }),
            text(format!("{mark}{}", line.text))
                .font(reading.mono)
                .size(reading.size)
                .wrapping(Wrapping::None)
                .style(move |_| text::Style { color: Some(color) }),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .style(move |_theme: &Theme| container::Style {
        background: Some(Background::Color(tint)),
        text_color: Some(color),
        border: Border::default(),
        shadow: Shadow::default(),
    })
    .into()
}

fn note<'a>(value: &str, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    text(value.to_string())
        .font(reading.font)
        .size(reading.size)
        .style(move |_| text::Style {
            color: Some(tokens.text_muted),
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_domain::{DiffLine, Hunk};

    fn patch(lines: usize) -> Patch {
        Patch {
            path: "src/lib.rs".to_string(),
            binary: false,
            hunks: vec![Hunk {
                header: "@@ -1 +1 @@".to_string(),
                lines: (0..lines)
                    .map(|index| DiffLine {
                        kind: DiffLineKind::Context,
                        text: format!("line {index}"),
                    })
                    .collect(),
            }],
        }
    }

    #[test]
    fn nothing_is_hidden_when_the_diff_fits_the_limit() {
        assert_eq!(hidden_lines(&[patch(10)], 10), 0);
        assert_eq!(hidden_lines(&[], 10), 0);
    }

    #[test]
    fn patch_stats_count_added_and_removed() {
        let sample = Patch {
            path: "a.rs".into(),
            binary: false,
            hunks: vec![Hunk {
                header: "@@ -1 +1 @@".into(),
                lines: vec![
                    DiffLine {
                        kind: DiffLineKind::Context,
                        text: "keep".into(),
                    },
                    DiffLine {
                        kind: DiffLineKind::Added,
                        text: "new".into(),
                    },
                    DiffLine {
                        kind: DiffLineKind::Added,
                        text: "also".into(),
                    },
                    DiffLine {
                        kind: DiffLineKind::Removed,
                        text: "old".into(),
                    },
                ],
            }],
        };
        assert_eq!(patch_stats(&sample), (2, 1));
    }

    #[test]
    fn visible_patches_honour_focus() {
        let a = patch(1);
        let mut b = patch(1);
        b.path = "other.rs".into();
        let all = [a, b];
        assert_eq!(visible_patches(&all, None).len(), 2);
        assert_eq!(visible_patches(&all, Some("other.rs")).len(), 1);
    }

    #[test]
    fn hidden_lines_count_every_patch() {
        assert_eq!(hidden_lines(&[patch(300), patch(300)], 400), 200);
    }

    #[test]
    fn hunk_headers_parse_full_and_short_forms() {
        assert_eq!(
            parse_hunk_header("@@ -10,5 +12,7 @@ fn main"),
            Some(HunkRange {
                old_start: 10,
                new_start: 12
            })
        );
        assert_eq!(
            parse_hunk_header("@@ -8 +9 @@"),
            Some(HunkRange {
                old_start: 8,
                new_start: 9
            })
        );
        assert_eq!(
            parse_hunk_header("@@ -1 +0,0 @@"),
            Some(HunkRange {
                old_start: 1,
                new_start: 0
            })
        );
        assert_eq!(parse_hunk_header("not a hunk"), None);
    }

    #[test]
    fn number_hunk_lines_advances_each_side() {
        let hunk = Hunk {
            header: "@@ -8,3 +9,4 @@".into(),
            lines: vec![
                DiffLine {
                    kind: DiffLineKind::Context,
                    text: "a".into(),
                },
                DiffLine {
                    kind: DiffLineKind::Removed,
                    text: "b".into(),
                },
                DiffLine {
                    kind: DiffLineKind::Added,
                    text: "c".into(),
                },
                DiffLine {
                    kind: DiffLineKind::Added,
                    text: "d".into(),
                },
            ],
        };
        let numbered = number_hunk_lines(&hunk);
        assert_eq!(numbered[0].0, Some(8));
        assert_eq!(numbered[0].1, Some(9));
        assert_eq!(numbered[1].0, Some(9));
        assert_eq!(numbered[1].1, None);
        assert_eq!(numbered[2].0, None);
        assert_eq!(numbered[2].1, Some(10));
        assert_eq!(numbered[3].0, None);
        assert_eq!(numbered[3].1, Some(11));
    }
}
