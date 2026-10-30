//! What the Diff pane draws, worked out before any widget exists.
//!
//! Side by side, after the JetBrains diff viewer: removed and added runs inside a hunk are
//! paired line for line into *modified* rows; what is left over is *removed* (old side only)
//! or *added* (new side only) against an empty filler. Inside a modified pair the changed
//! words are marked by a token-level longest common subsequence.
//!
//! Every line kind has a fixed height, so the scroll offset of each difference is a plain
//! sum. The view draws with those heights and the reducer scrolls with the same numbers.

use core_domain::{DiffLineKind, Hunk, Patch};

/// Height of one diff text row.
pub(crate) const ROW_HEIGHT: f32 = 20.0;
/// Height of a file's title row.
pub(crate) const FILE_HEIGHT: f32 = 26.0;
/// Height of a hunk's `@@` row (it also carries Stage and Discard).
pub(crate) const HUNK_HEIGHT: f32 = 24.0;
/// Height of a note such as "Binary file".
pub(crate) const NOTE_HEIGHT: f32 = 22.0;
/// Rows kept above a difference when jumping to it.
const JUMP_MARGIN_ROWS: f32 = 3.0;
/// Above this many token pairs, inline marking falls back to prefix and suffix.
const MAX_TOKEN_PAIRS: usize = 40_000;

/// How one side of a row reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cell {
    Context,
    /// One half of a modified pair.
    Changed,
    Added,
    Removed,
    /// Nothing on this side (the other side was added or removed).
    Empty,
}

/// A run of text and whether it is part of the change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Piece {
    pub text: String,
    pub changed: bool,
}

/// One side of a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Side {
    pub number: Option<u32>,
    pub cell: Cell,
    pub pieces: Vec<Piece>,
}

/// One drawn line of the pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Line {
    File { path: String },
    Note { text: String },
    Hunk { path: String, index: usize, header: String },
    /// Side by side.
    Split { left: Side, right: Side },
    /// One column with both line numbers.
    Unified { old: Option<u32>, new: Option<u32>, kind: DiffLineKind, text: String },
}

impl Line {
    pub(crate) fn height(&self) -> f32 {
        match self {
            Line::File { .. } => FILE_HEIGHT,
            Line::Note { .. } => NOTE_HEIGHT,
            Line::Hunk { .. } => HUNK_HEIGHT,
            Line::Split { .. } | Line::Unified { .. } => ROW_HEIGHT,
        }
    }

    fn is_change(&self) -> bool {
        match self {
            Line::Split { left, right } => left.cell != Cell::Context || right.cell != Cell::Context,
            Line::Unified { kind, .. } => *kind != DiffLineKind::Context,
            _ => false,
        }
    }
}

/// The pane's lines, and how many text rows the budget left out.
pub(crate) struct Layout {
    pub lines: Vec<Line>,
    pub hidden: usize,
}

impl Layout {
    /// Index of the first line of each difference (a run of changed rows).
    pub(crate) fn changes(&self) -> Vec<usize> {
        let mut starts = Vec::new();
        let mut inside = false;
        for (index, line) in self.lines.iter().enumerate() {
            let change = line.is_change();
            if change && !inside {
                starts.push(index);
            }
            // File and hunk rows are never changes, so they end a run.
            inside = change;
        }
        starts
    }

    /// Scroll offset that shows line `index` a few rows below the top.
    pub(crate) fn offset_of(&self, index: usize) -> f32 {
        let top: f32 = self.lines.iter().take(index).map(Line::height).sum();
        (top - JUMP_MARGIN_ROWS * ROW_HEIGHT).max(0.0)
    }
}

/// The difference one step from `current` among `count`. With none current, forward is the
/// first and back the last. `None` past either end.
pub(crate) fn step(current: Option<usize>, count: usize, forward: bool) -> Option<usize> {
    let next = match (current, forward) {
        (None, true) => Some(0),
        (None, false) => count.checked_sub(1),
        (Some(at), true) => Some(at + 1),
        (Some(at), false) => at.checked_sub(1),
    };
    next.filter(|at| *at < count)
}

/// Lay out `patches` (only `focus` when set), at most `budget` text rows.
pub(crate) fn layout(patches: &[Patch], focus: Option<&str>, budget: usize, split: bool) -> Layout {
    let mut lines = Vec::new();
    let mut left = budget;
    let mut hidden = 0;
    for patch in patches.iter().filter(|patch| focus.is_none_or(|path| patch.path == path)) {
        lines.push(Line::File { path: patch.path.clone() });
        if patch.binary {
            lines.push(Line::Note { text: "Binary file".to_string() });
            continue;
        }
        for (index, hunk) in patch.hunks.iter().enumerate() {
            let rows = if split { split_rows(hunk) } else { unified_rows(hunk) };
            if left == 0 {
                hidden += rows.len();
                continue;
            }
            lines.push(Line::Hunk { path: patch.path.clone(), index, header: hunk.header.clone() });
            let shown = rows.len().min(left);
            hidden += rows.len() - shown;
            left -= shown;
            lines.extend(rows.into_iter().take(shown));
        }
    }
    Layout { lines, hidden }
}

/// Old and new start lines from `@@ -a,b +c,d @@`. Missing or bad headers start at 1.
fn starts(header: &str) -> (u32, u32) {
    let number = |part: &str| part.split(',').next().and_then(|n| n.parse().ok());
    let mut parts = header.split_whitespace().skip(1);
    let old = parts.next().and_then(|p| p.strip_prefix('-')).and_then(number);
    let new = parts.next().and_then(|p| p.strip_prefix('+')).and_then(number);
    (old.unwrap_or(1), new.unwrap_or(1))
}

fn unified_rows(hunk: &Hunk) -> Vec<Line> {
    let (mut old, mut new) = starts(&hunk.header);
    hunk.lines
        .iter()
        .map(|line| {
            let (o, n) = match line.kind {
                DiffLineKind::Context => (Some(old), Some(new)),
                DiffLineKind::Removed => (Some(old), None),
                DiffLineKind::Added => (None, Some(new)),
            };
            old += u32::from(o.is_some());
            new += u32::from(n.is_some());
            Line::Unified { old: o, new: n, kind: line.kind, text: line.text.clone() }
        })
        .collect()
}

fn whole(text: &str, changed: bool) -> Vec<Piece> {
    vec![Piece { text: text.to_string(), changed }]
}

fn empty_side() -> Side {
    Side { number: None, cell: Cell::Empty, pieces: Vec::new() }
}

/// Side-by-side rows for one hunk.
pub(crate) fn split_rows(hunk: &Hunk) -> Vec<Line> {
    let (mut old, mut new) = starts(&hunk.header);
    let mut rows = Vec::new();
    let mut lines = hunk.lines.iter().peekable();
    while let Some(line) = lines.next() {
        if line.kind == DiffLineKind::Context {
            rows.push(Line::Split {
                left: Side { number: Some(old), cell: Cell::Context, pieces: whole(&line.text, false) },
                right: Side { number: Some(new), cell: Cell::Context, pieces: whole(&line.text, false) },
            });
            old += 1;
            new += 1;
            continue;
        }
        // A change run: removed lines, then added lines (git prints them in that order).
        let mut removed = Vec::new();
        let mut added = Vec::new();
        let mut current = Some(line);
        while let Some(line) = current {
            match line.kind {
                DiffLineKind::Removed if added.is_empty() => removed.push(line.text.as_str()),
                DiffLineKind::Added => added.push(line.text.as_str()),
                _ => break,
            }
            current = match lines.peek() {
                Some(next) if next.kind != DiffLineKind::Context && !(next.kind == DiffLineKind::Removed && !added.is_empty()) => lines.next(),
                _ => None,
            };
        }
        for pair in 0..removed.len().max(added.len()) {
            let row = match (removed.get(pair), added.get(pair)) {
                (Some(before), Some(after)) => {
                    let (left, right) = changed_pieces(before, after);
                    let row = Line::Split {
                        left: Side { number: Some(old), cell: Cell::Changed, pieces: left },
                        right: Side { number: Some(new), cell: Cell::Changed, pieces: right },
                    };
                    old += 1;
                    new += 1;
                    row
                }
                (Some(before), None) => {
                    let row = Line::Split {
                        left: Side { number: Some(old), cell: Cell::Removed, pieces: whole(before, false) },
                        right: empty_side(),
                    };
                    old += 1;
                    row
                }
                (None, Some(after)) => {
                    let row = Line::Split {
                        left: empty_side(),
                        right: Side { number: Some(new), cell: Cell::Added, pieces: whole(after, false) },
                    };
                    new += 1;
                    row
                }
                (None, None) => unreachable!("pair index is below the longer run"),
            };
            rows.push(row);
        }
    }
    rows
}

/// Words, runs of spaces, and single punctuation marks.
fn tokens(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let class = |c: char| {
        if c.is_alphanumeric() || c == '_' {
            0
        } else if c.is_whitespace() {
            1
        } else {
            2
        }
    };
    let mut previous: Option<u8> = None;
    for (at, c) in text.char_indices() {
        let kind = class(c);
        if let Some(prev) = previous {
            if prev != kind || kind == 2 {
                out.push(&text[start..at]);
                start = at;
            }
        }
        previous = Some(kind);
    }
    if start < text.len() {
        out.push(&text[start..]);
    }
    out
}

fn merged(parts: Vec<(&str, bool)>) -> Vec<Piece> {
    let mut pieces: Vec<Piece> = Vec::new();
    for (text, changed) in parts {
        match pieces.last_mut() {
            Some(last) if last.changed == changed => last.text.push_str(text),
            _ => pieces.push(Piece { text: text.to_string(), changed }),
        }
    }
    pieces
}

/// Each side of a modified pair split into unchanged and changed runs.
pub(crate) fn changed_pieces(before: &str, after: &str) -> (Vec<Piece>, Vec<Piece>) {
    let a = tokens(before);
    let b = tokens(after);
    if a.len().saturating_mul(b.len()) > MAX_TOKEN_PAIRS {
        return prefix_suffix(before, after);
    }
    // lcs[i][j] = longest common subsequence of a[i..] and b[j..].
    let mut lcs = vec![vec![0u32; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            lcs[i][j] = if a[i] == b[j] { lcs[i + 1][j + 1] + 1 } else { lcs[i + 1][j].max(lcs[i][j + 1]) };
        }
    }
    let (mut i, mut j) = (0, 0);
    let (mut left, mut right) = (Vec::new(), Vec::new());
    while i < a.len() && j < b.len() {
        if a[i] == b[j] {
            left.push((a[i], false));
            right.push((b[j], false));
            i += 1;
            j += 1;
        } else if lcs[i + 1][j] >= lcs[i][j + 1] {
            left.push((a[i], true));
            i += 1;
        } else {
            right.push((b[j], true));
            j += 1;
        }
    }
    left.extend(a[i..].iter().map(|token| (*token, true)));
    right.extend(b[j..].iter().map(|token| (*token, true)));
    (merged(left), merged(right))
}

/// Common prefix and suffix unchanged, the middle changed. For very long lines.
fn prefix_suffix(before: &str, after: &str) -> (Vec<Piece>, Vec<Piece>) {
    let prefix = before
        .char_indices()
        .zip(after.chars())
        .take_while(|((_, x), y)| x == y)
        .last()
        .map_or(0, |((at, c), _)| at + c.len_utf8());
    let rest_a = &before[prefix..];
    let rest_b = &after[prefix..];
    let suffix = rest_a.chars().rev().zip(rest_b.chars().rev()).take_while(|(x, y)| x == y).map(|(c, _)| c.len_utf8()).sum::<usize>();
    let split = |text: &str, rest: &str| {
        let cut = rest.len() - suffix.min(rest.len());
        merged(vec![(&text[..prefix], false), (&rest[..cut], true), (&rest[cut..], false)])
            .into_iter()
            .filter(|piece| !piece.text.is_empty())
            .collect()
    };
    (split(before, rest_a), split(after, rest_b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_domain::DiffLine;

    fn hunk(header: &str, lines: &[(DiffLineKind, &str)]) -> Hunk {
        Hunk {
            header: header.into(),
            lines: lines.iter().map(|(kind, text)| DiffLine { kind: *kind, text: (*text).into() }).collect(),
        }
    }

    fn cells(rows: &[Line]) -> Vec<(Option<u32>, Cell, Option<u32>, Cell)> {
        rows.iter()
            .map(|row| match row {
                Line::Split { left, right } => (left.number, left.cell, right.number, right.cell),
                _ => panic!("split rows only"),
            })
            .collect()
    }

    use DiffLineKind::{Added as A, Context as C, Removed as R};

    #[test]
    fn removed_and_added_runs_pair_into_modified_rows() {
        let rows = split_rows(&hunk("@@ -10,4 +10,5 @@", &[(C, "a"), (R, "b"), (R, "c"), (A, "B"), (A, "C"), (A, "D"), (C, "e")]));
        assert_eq!(
            cells(&rows),
            [
                (Some(10), Cell::Context, Some(10), Cell::Context),
                (Some(11), Cell::Changed, Some(11), Cell::Changed),
                (Some(12), Cell::Changed, Some(12), Cell::Changed),
                (None, Cell::Empty, Some(13), Cell::Added),
                (Some(13), Cell::Context, Some(14), Cell::Context),
            ]
        );
    }

    #[test]
    fn a_lone_removal_leaves_the_new_side_empty() {
        let rows = split_rows(&hunk("@@ -1,2 +1,1 @@", &[(C, "a"), (R, "b")]));
        assert_eq!(cells(&rows)[1], (Some(2), Cell::Removed, None, Cell::Empty));
    }

    #[test]
    fn a_removal_after_an_addition_starts_a_new_run() {
        let rows = split_rows(&hunk("@@ -1,2 +1,2 @@", &[(A, "x"), (R, "y")]));
        assert_eq!(
            cells(&rows),
            [(None, Cell::Empty, Some(1), Cell::Added), (Some(1), Cell::Removed, None, Cell::Empty)]
        );
    }

    #[test]
    fn only_the_changed_words_are_marked() {
        let (left, right) = changed_pieces(r#"<PackageReference Include="RestSharp" />"#, r#"<PackageReference Include="NPOI" />"#);
        let changed = |pieces: &[Piece]| pieces.iter().filter(|p| p.changed).map(|p| p.text.clone()).collect::<Vec<_>>();
        assert_eq!(changed(&left), ["RestSharp"]);
        assert_eq!(changed(&right), ["NPOI"]);
        let joined: String = right.iter().map(|p| p.text.as_str()).collect();
        assert_eq!(joined, r#"<PackageReference Include="NPOI" />"#);
    }

    #[test]
    fn long_lines_fall_back_to_prefix_and_suffix() {
        let before = format!("{}A{}", "x ".repeat(300), " y".repeat(300));
        let after = format!("{}B{}", "x ".repeat(300), " y".repeat(300));
        let (left, right) = changed_pieces(&before, &after);
        assert_eq!(left.iter().filter(|p| p.changed).map(|p| p.text.as_str()).collect::<Vec<_>>(), ["A"]);
        assert_eq!(right.iter().filter(|p| p.changed).map(|p| p.text.as_str()).collect::<Vec<_>>(), ["B"]);
    }

    fn patch(path: &str, hunks: Vec<Hunk>) -> Patch {
        Patch { path: path.into(), hunks, binary: false }
    }

    #[test]
    fn each_run_of_changes_is_one_difference() {
        let patches = [patch(
            "a.rs",
            vec![
                hunk("@@ -1,5 +1,5 @@", &[(C, "a"), (R, "b"), (A, "B"), (C, "c"), (A, "d"), (C, "e")]),
                hunk("@@ -20,1 +20,1 @@", &[(R, "x"), (A, "y")]),
            ],
        )];
        let layout = layout(&patches, None, 1_000, true);
        // File, hunk, a, b|B, c, |d, e, hunk, x|y
        assert_eq!(layout.changes(), [3, 5, 8]);
        assert_eq!(layout.offset_of(0), 0.0);
        let top_of_d = FILE_HEIGHT + HUNK_HEIGHT + 3.0 * ROW_HEIGHT;
        assert_eq!(layout.offset_of(5), top_of_d - 3.0 * ROW_HEIGHT);
    }

    #[test]
    fn hunk_headers_parse_full_and_short_forms() {
        assert_eq!(starts("@@ -10,5 +12,7 @@ fn main"), (10, 12));
        assert_eq!(starts("@@ -8 +9 @@"), (8, 9));
        assert_eq!(starts("@@ -1 +0,0 @@"), (1, 0));
        assert_eq!(starts("not a hunk"), (1, 1));
    }

    #[test]
    fn stepping_stops_at_either_end() {
        assert_eq!(step(None, 3, true), Some(0));
        assert_eq!(step(None, 3, false), Some(2));
        assert_eq!(step(Some(2), 3, true), None);
        assert_eq!(step(Some(0), 3, false), None);
        assert_eq!(step(None, 0, true), None);
    }

    #[test]
    fn the_budget_counts_text_rows_and_reports_the_rest() {
        let patches = [patch("a.rs", vec![hunk("@@ -1,3 +1,3 @@", &[(C, "a"), (C, "b"), (C, "c")])])];
        let layout = layout(&patches, None, 2, false);
        assert_eq!(layout.hidden, 1);
        assert_eq!(layout.lines.len(), 4);
    }

    #[test]
    fn unified_rows_number_both_sides() {
        let rows = unified_rows(&hunk("@@ -8,2 +9,2 @@", &[(C, "a"), (R, "b"), (A, "c")]));
        let numbers: Vec<_> = rows
            .iter()
            .map(|row| match row {
                Line::Unified { old, new, .. } => (*old, *new),
                _ => panic!(),
            })
            .collect();
        assert_eq!(numbers, [(Some(8), Some(9)), (Some(9), None), (None, Some(10))]);
    }
}
