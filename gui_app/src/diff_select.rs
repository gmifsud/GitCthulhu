//! Selecting text in the Diff pane, as in a read-only editor: a caret, a selection made with
//! the mouse or the keys, and the text it covers. Worked out on the pane's laid-out lines
//! ([`crate::diff_view::Layout`]), with no widget involved, so it is tested as plain data.
//!
//! A selection stays in the column it began in: the old side, the new side, or the single
//! column of a unified diff. Copying a side never drags in the other side's text.

use crate::diff_view::{Cell, Layout, Line, Side};

/// Which column of text a position is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pane {
    Left,
    Right,
    Unified,
}

/// A place between two characters: the layout line, and how many characters precede it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Pos {
    pub line: usize,
    pub col: usize,
}

/// A selection: where it began and where the caret is now. Equal ends are a bare caret.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DiffSelection {
    pub pane: Pane,
    pub anchor: Pos,
    pub caret: Pos,
}

/// How a click selects: a single click places the caret, a double click takes the word, a
/// triple click the line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Grain {
    Caret,
    Word,
    Line,
}

/// A caret movement from the keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Motion {
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    /// Up or down by this many lines (a screenful).
    PageUp(usize),
    PageDown(usize),
    Top,
    Bottom,
}

/// A key the Diff pane answers while it has the caret.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DiffKey {
    Copy,
    SelectAll,
    Move(Motion, bool),
}

impl DiffSelection {
    /// The two ends in reading order.
    pub(crate) fn range(&self) -> (Pos, Pos) {
        if self.anchor <= self.caret {
            (self.anchor, self.caret)
        } else {
            (self.caret, self.anchor)
        }
    }

    pub(crate) fn is_caret(&self) -> bool {
        self.anchor == self.caret
    }
}

fn side_text(side: &Side) -> Option<String> {
    // The filler opposite an added or removed line holds no text and takes no caret.
    (side.cell != Cell::Empty).then(|| side.pieces.iter().map(|piece| piece.text.as_str()).collect())
}

/// The text `pane` shows on `line`, or `None` when the line has none there: a file or hunk
/// header, a note, a filler, or a line of the other diff mode.
pub(crate) fn line_text(line: &Line, pane: Pane) -> Option<String> {
    match (line, pane) {
        (Line::Split { left, .. }, Pane::Left) => side_text(left),
        (Line::Split { right, .. }, Pane::Right) => side_text(right),
        (Line::Unified { text, .. }, Pane::Unified) => Some(text.clone()),
        _ => None,
    }
}

fn text_at(layout: &Layout, pane: Pane, line: usize) -> Option<String> {
    layout.lines.get(line).and_then(|line| line_text(line, pane))
}

fn length(layout: &Layout, pane: Pane, line: usize) -> usize {
    text_at(layout, pane, line).map_or(0, |text| text.chars().count())
}

/// The text line nearest `line`: itself, else the next one below, else the last one above.
fn snap(layout: &Layout, pane: Pane, line: usize) -> Option<usize> {
    let has = |index: usize| text_at(layout, pane, index).is_some();
    let last = layout.lines.len().checked_sub(1)?;
    let line = line.min(last);
    (line..=last).find(|&index| has(index)).or_else(|| (0..line).rev().find(|&index| has(index)))
}

/// `pos` moved onto real text: the nearest text line, and a column within it.
fn clamp(layout: &Layout, pane: Pane, pos: Pos) -> Option<Pos> {
    let line = snap(layout, pane, pos.line)?;
    // Snapped off a header onto another line: the column no longer means anything there.
    let col = if line == pos.line { pos.col.min(length(layout, pane, line)) } else { 0 };
    Some(Pos { line, col })
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The run of like characters around column `col`: a word, a run of spaces, or one symbol.
pub(crate) fn word_at(text: &str, col: usize) -> (usize, usize) {
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return (0, 0);
    }
    let at = col.min(chars.len() - 1);
    let class = |c: char| if is_word(c) { 0 } else if c.is_whitespace() { 1 } else { 2 };
    let kind = class(chars[at]);
    if kind == 2 {
        return (at, at + 1);
    }
    let start = (0..at).rev().take_while(|&index| class(chars[index]) == kind).last().unwrap_or(at);
    let end = (at..chars.len()).take_while(|&index| class(chars[index]) == kind).last().map_or(at, |index| index + 1);
    (start, end)
}

/// A press at `pos` in `pane`. `extend` (Shift held) moves only the caret of a selection
/// already in that pane. `None` when the press is not on text.
pub(crate) fn pressed(
    layout: &Layout,
    current: Option<DiffSelection>,
    pane: Pane,
    pos: Pos,
    grain: Grain,
    extend: bool,
) -> Option<DiffSelection> {
    let text = text_at(layout, pane, pos.line)?;
    let col = pos.col.min(text.chars().count());
    let at = Pos { line: pos.line, col };
    match grain {
        Grain::Caret => match current {
            Some(held) if extend && held.pane == pane => Some(DiffSelection { caret: at, ..held }),
            _ => Some(DiffSelection { pane, anchor: at, caret: at }),
        },
        Grain::Word => {
            let (start, end) = word_at(&text, col);
            Some(DiffSelection { pane, anchor: Pos { line: at.line, col: start }, caret: Pos { line: at.line, col: end } })
        }
        Grain::Line => Some(DiffSelection {
            pane,
            anchor: Pos { line: at.line, col: 0 },
            caret: Pos { line: at.line, col: text.chars().count() },
        }),
    }
}

/// The selection with its caret dragged to `pos`, kept on text.
pub(crate) fn dragged(layout: &Layout, held: DiffSelection, pos: Pos) -> DiffSelection {
    match clamp(layout, held.pane, pos) {
        Some(caret) => DiffSelection { caret, ..held },
        None => held,
    }
}

/// Everything in `pane`, or `None` when it has no text.
pub(crate) fn select_all(layout: &Layout, pane: Pane) -> Option<DiffSelection> {
    let first = snap(layout, pane, 0)?;
    let last = (0..layout.lines.len()).rev().find(|&index| text_at(layout, pane, index).is_some())?;
    Some(DiffSelection {
        pane,
        anchor: Pos { line: first, col: 0 },
        caret: Pos { line: last, col: length(layout, pane, last) },
    })
}

/// The selection after a caret movement. Without `extend` the selection collapses to the caret.
pub(crate) fn moved(layout: &Layout, held: DiffSelection, motion: Motion, extend: bool) -> DiffSelection {
    let pane = held.pane;
    let has = |index: usize| text_at(layout, pane, index).is_some();
    let above = |from: usize| (0..from).rev().find(|&index| has(index));
    let below = |from: usize| (from + 1..layout.lines.len()).find(|&index| has(index));
    let steps = |from: usize, count: usize, next: &dyn Fn(usize) -> Option<usize>| {
        (0..count).fold(from, |line, _| next(line).unwrap_or(line))
    };
    let Pos { line, col } = held.caret;
    let on = |line: usize| Pos { line, col: col.min(length(layout, pane, line)) };
    let caret = match motion {
        Motion::Left if col > 0 => Pos { line, col: col - 1 },
        Motion::Left => above(line).map_or(held.caret, |line| Pos { line, col: length(layout, pane, line) }),
        Motion::Right if col < length(layout, pane, line) => Pos { line, col: col + 1 },
        Motion::Right => below(line).map_or(held.caret, |line| Pos { line, col: 0 }),
        Motion::Up => above(line).map_or(held.caret, on),
        Motion::Down => below(line).map_or(held.caret, on),
        Motion::Home => Pos { line, col: 0 },
        Motion::End => Pos { line, col: length(layout, pane, line) },
        Motion::PageUp(count) => on(steps(line, count.max(1), &above)),
        Motion::PageDown(count) => on(steps(line, count.max(1), &below)),
        Motion::Top => snap(layout, pane, 0).map_or(held.caret, |line| Pos { line, col: 0 }),
        Motion::Bottom => (0..layout.lines.len())
            .rev()
            .find(|&index| has(index))
            .map_or(held.caret, |line| Pos { line, col: length(layout, pane, line) }),
    };
    DiffSelection { pane, anchor: if extend { held.anchor } else { caret }, caret }
}

/// The selected text, one line per diff line. Headers and fillers inside the range are left
/// out: what is copied is the file's text, as that side shows it.
pub(crate) fn selected_text(layout: &Layout, held: &DiffSelection) -> String {
    let (start, end) = held.range();
    let mut lines = Vec::new();
    for index in start.line..=end.line.min(layout.lines.len().saturating_sub(1)) {
        let Some(text) = text_at(layout, held.pane, index) else {
            continue;
        };
        let from = if index == start.line { start.col } else { 0 };
        let to = if index == end.line { end.col } else { usize::MAX };
        lines.push(text.chars().skip(from).take(to.saturating_sub(from)).collect::<String>());
    }
    lines.join("\n")
}

/// The top of every line, and the bottom of the last: `tops[i]..tops[i + 1]` is line `i`.
pub(crate) fn tops(layout: &Layout) -> Vec<f32> {
    let mut tops = Vec::with_capacity(layout.lines.len() + 1);
    let mut top = 0.0;
    tops.push(top);
    for line in &layout.lines {
        top += line.height();
        tops.push(top);
    }
    tops
}

/// The line drawn at height `y`, if any.
pub(crate) fn line_at(tops: &[f32], y: f32) -> Option<usize> {
    if y < 0.0 || tops.len() < 2 || y >= *tops.last()? {
        return None;
    }
    Some(tops.partition_point(|&top| top <= y).saturating_sub(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff_view::Piece;
    use core_domain::DiffLineKind;

    fn side(cell: Cell, text: &str) -> Side {
        Side { number: Some(1), cell, pieces: vec![Piece { text: text.to_string(), changed: false }] }
    }

    fn empty() -> Side {
        Side { number: None, cell: Cell::Empty, pieces: vec![] }
    }

    /// 0 file · 1 hunk · 2 context · 3 changed pair · 4 added (right only) · 5 hunk · 6 context
    fn split() -> Layout {
        Layout {
            lines: vec![
                Line::File { path: "a.rs".into() },
                Line::Hunk { path: "a.rs".into(), index: 0, header: "@@".into() },
                Line::Split { left: side(Cell::Context, "fn main() {"), right: side(Cell::Context, "fn main() {") },
                Line::Split { left: side(Cell::Changed, "    old_call();"), right: side(Cell::Changed, "    new_call(1);") },
                Line::Split { left: empty(), right: side(Cell::Added, "    extra();") },
                Line::Hunk { path: "a.rs".into(), index: 1, header: "@@".into() },
                Line::Split { left: side(Cell::Context, "}"), right: side(Cell::Context, "}") },
            ],
            hidden: 0,
        }
    }

    fn at(line: usize, col: usize) -> Pos {
        Pos { line, col }
    }

    fn held(pane: Pane, anchor: Pos, caret: Pos) -> DiffSelection {
        DiffSelection { pane, anchor, caret }
    }

    #[test]
    fn a_click_places_the_caret_on_text_and_nowhere_else() {
        let layout = split();
        let caret = pressed(&layout, None, Pane::Right, at(3, 99), Grain::Caret, false).expect("on text");
        assert_eq!(caret, held(Pane::Right, at(3, 16), at(3, 16)), "past the end of the line is its end");
        assert!(caret.is_caret());
        assert_eq!(pressed(&layout, None, Pane::Left, at(1, 0), Grain::Caret, false), None, "a hunk header");
        assert_eq!(pressed(&layout, None, Pane::Left, at(4, 0), Grain::Caret, false), None, "a filler");
        assert_eq!(pressed(&layout, None, Pane::Unified, at(2, 0), Grain::Caret, false), None, "the other mode");
    }

    #[test]
    fn a_shift_click_extends_only_a_selection_in_the_same_column() {
        let layout = split();
        let start = pressed(&layout, None, Pane::Left, at(2, 3), Grain::Caret, false);
        let extended = pressed(&layout, start, Pane::Left, at(3, 4), Grain::Caret, true).expect("extended");
        assert_eq!(extended, held(Pane::Left, at(2, 3), at(3, 4)));
        let other = pressed(&layout, start, Pane::Right, at(3, 4), Grain::Caret, true).expect("a new caret");
        assert!(other.is_caret() && other.pane == Pane::Right);
    }

    #[test]
    fn a_double_click_takes_the_word_and_a_triple_click_the_line() {
        let layout = split();
        let word = pressed(&layout, None, Pane::Left, at(3, 6), Grain::Word, false).expect("word");
        assert_eq!(selected_text(&layout, &word), "old_call");
        let line = pressed(&layout, None, Pane::Right, at(3, 6), Grain::Line, false).expect("line");
        assert_eq!(selected_text(&layout, &line), "    new_call(1);");
        assert_eq!(word_at("a  b", 1), (1, 3), "a run of spaces");
        assert_eq!(word_at("f(x)", 1), (1, 2), "one symbol");
        assert_eq!(word_at("", 0), (0, 0));
    }

    #[test]
    fn copied_text_is_one_side_only_without_headers_or_fillers() {
        let layout = split();
        let right = held(Pane::Right, at(2, 3), at(6, 1));
        assert_eq!(selected_text(&layout, &right), "main() {\n    new_call(1);\n    extra();\n}");
        // The same lines on the old side: the filler opposite the added line is skipped.
        let left = held(Pane::Left, at(6, 1), at(2, 3));
        assert_eq!(selected_text(&layout, &left), "main() {\n    old_call();\n}", "a backwards drag reads the same");
        assert_eq!(selected_text(&layout, &held(Pane::Left, at(2, 3), at(2, 3))), "");
    }

    #[test]
    fn select_all_covers_the_column_from_its_first_text_to_its_last() {
        let layout = split();
        let all = select_all(&layout, Pane::Left).expect("text");
        assert_eq!(all, held(Pane::Left, at(2, 0), at(6, 1)));
        assert_eq!(select_all(&layout, Pane::Unified), None);
    }

    #[test]
    fn the_caret_moves_over_text_lines_and_skips_headers_and_fillers() {
        let layout = split();
        let caret = |line, col| held(Pane::Left, at(line, col), at(line, col));
        assert_eq!(moved(&layout, caret(3, 14), Motion::Down, false).caret, at(6, 1), "past the filler and the header");
        assert_eq!(moved(&layout, caret(6, 0), Motion::Up, false).caret, at(3, 0));
        assert_eq!(moved(&layout, caret(3, 0), Motion::Left, false).caret, at(2, 11), "wraps to the end of the line above");
        assert_eq!(moved(&layout, caret(2, 11), Motion::Right, false).caret, at(3, 0));
        assert_eq!(moved(&layout, caret(2, 0), Motion::Up, false).caret, at(2, 0), "nowhere to go: stays");
        assert_eq!(moved(&layout, caret(3, 5), Motion::Home, false).caret, at(3, 0));
        assert_eq!(moved(&layout, caret(3, 5), Motion::End, false).caret, at(3, 15));
        assert_eq!(moved(&layout, caret(2, 0), Motion::PageDown(50), false).caret, at(6, 0));
        assert_eq!(moved(&layout, caret(6, 1), Motion::Top, false).caret, at(2, 0));
        assert_eq!(moved(&layout, caret(2, 0), Motion::Bottom, false).caret, at(6, 1));
    }

    #[test]
    fn shift_keeps_the_anchor_and_a_plain_move_collapses_the_selection() {
        let layout = split();
        let start = held(Pane::Right, at(2, 0), at(2, 0));
        let extended = moved(&layout, moved(&layout, start, Motion::Down, true), Motion::End, true);
        assert_eq!(extended, held(Pane::Right, at(2, 0), at(3, 16)));
        assert_eq!(selected_text(&layout, &extended), "fn main() {\n    new_call(1);");
        assert!(moved(&layout, extended, Motion::Right, false).is_caret());
    }

    #[test]
    fn a_drag_onto_a_header_lands_on_the_nearest_text() {
        let layout = split();
        let start = held(Pane::Left, at(2, 3), at(2, 3));
        assert_eq!(dragged(&layout, start, at(5, 7)).caret, at(6, 0), "the hunk header: the line below");
        assert_eq!(dragged(&layout, start, at(3, 99)).caret, at(3, 15));
        assert_eq!(dragged(&layout, start, at(99, 0)).caret, at(6, 0), "below the last line");
    }

    #[test]
    fn a_unified_diff_is_one_column() {
        let layout = Layout {
            lines: vec![
                Line::Unified { old: Some(1), new: None, kind: DiffLineKind::Removed, text: "old".into() },
                Line::Unified { old: None, new: Some(1), kind: DiffLineKind::Added, text: "new".into() },
            ],
            hidden: 0,
        };
        let all = select_all(&layout, Pane::Unified).expect("text");
        assert_eq!(selected_text(&layout, &all), "old\nnew");
    }

    #[test]
    fn a_height_maps_to_the_line_drawn_there() {
        let layout = split();
        let tops = tops(&layout);
        assert_eq!(tops.len(), layout.lines.len() + 1);
        assert_eq!(line_at(&tops, 0.0), Some(0));
        assert_eq!(line_at(&tops, tops[2]), Some(2), "a line starts at its top");
        assert_eq!(line_at(&tops, tops[3] - 0.5), Some(2));
        assert_eq!(line_at(&tops, *tops.last().expect("tops")), None, "below the last line");
        assert_eq!(line_at(&tops, -1.0), None);
    }
}
