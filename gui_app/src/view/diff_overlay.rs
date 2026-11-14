//! The layer over the Diff pane's rows that makes it read like a read-only editor: it draws
//! the selection and the caret, and turns presses, drags and keys into selection messages.
//!
//! It sits on top of the rows and captures nothing, so the hunk buttons beneath still take
//! their clicks. A message is sent on a press, on a move while the button is held, and on a
//! key while the pane has the caret: a pointer merely passing over costs nothing (ADR 0007).

use std::time::{Duration, Instant};

use iced::keyboard::{self, key::Named, Key};
use iced::mouse;
use iced::widget::canvas::{self, event, Event};
use iced::{Color, Point, Rectangle, Renderer, Size, Theme};

use crate::app::Message;
use crate::diff_select::{line_at, DiffKey, DiffSelection, Grain, Motion, Pane, Pos};
use crate::diff_view::ROW_HEIGHT;
use crate::theme::DesignTokens;

/// Presses this close together, on the same spot, count as a double or triple click.
const MULTI_CLICK: Duration = Duration::from_millis(400);
const CARET_WIDTH: f32 = 1.5;

/// Where the text of each column sits across a pane `width` wide.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Columns {
    pub split: bool,
    /// Width of everything left of a column's text: its line numbers (and the +/- mark).
    pub gutter: f32,
    /// Width of the rule between the two sides.
    pub divider: f32,
}

impl Columns {
    /// The text of `pane` runs from the first value to the second.
    pub(crate) fn span(&self, pane: Pane, width: f32) -> (f32, f32) {
        let side = (width - self.divider) / 2.0;
        match pane {
            Pane::Unified => (self.gutter, width),
            Pane::Left => (self.gutter, side),
            Pane::Right => (side + self.divider + self.gutter, width),
        }
    }

    /// The column under `x`. A point in a gutter belongs to the text beside it.
    pub(crate) fn pane_at(&self, x: f32, width: f32) -> Pane {
        if !self.split {
            Pane::Unified
        } else if x < (width - self.divider) / 2.0 {
            Pane::Left
        } else {
            Pane::Right
        }
    }

    /// The character boundary nearest `x` in `pane`.
    pub(crate) fn col_at(&self, pane: Pane, x: f32, width: f32, char_width: f32) -> usize {
        let (from, _) = self.span(pane, width);
        if char_width <= 0.0 {
            return 0;
        }
        ((x - from) / char_width).round().max(0.0) as usize
    }
}

/// The overlay for one frame.
pub(crate) struct DiffOverlay {
    /// `tops[i]..tops[i + 1]` is line `i` ([`crate::diff_select::tops`]).
    pub tops: Vec<f32>,
    /// Whether each line is a row of text (not a file, hunk or note row).
    pub text_rows: Vec<bool>,
    pub columns: Columns,
    pub char_width: f32,
    pub selection: Option<DiffSelection>,
    /// The pane has the caret: keys go to it.
    pub focused: bool,
    /// Lines a Page key moves by.
    pub page: usize,
    pub tokens: DesignTokens,
}

/// What the pointer is doing here.
#[derive(Debug, Default)]
pub(crate) struct Pointer {
    dragging: bool,
    last_press: Option<(Instant, Pos, Pane)>,
    clicks: u8,
}

impl DiffOverlay {
    /// The text position under a point of the overlay, when that point is on a text row.
    fn hit(&self, at: Point, width: f32) -> Option<(Pane, Pos)> {
        let line = line_at(&self.tops, at.y)?;
        if !self.text_rows.get(line).copied().unwrap_or(false) {
            return None;
        }
        let pane = self.columns.pane_at(at.x, width);
        Some((pane, Pos { line, col: self.columns.col_at(pane, at.x, width, self.char_width) }))
    }

    /// While dragging, the pointer may leave the text: the nearest line still counts.
    fn drag_target(&self, at: Point, width: f32, pane: Pane) -> Pos {
        let last = self.tops.len().saturating_sub(2);
        let line = if at.y < 0.0 { 0 } else { line_at(&self.tops, at.y).unwrap_or(last) };
        Pos { line, col: self.columns.col_at(pane, at.x, width, self.char_width) }
    }

    fn key(&self, key: &Key, modifiers: keyboard::Modifiers) -> Option<DiffKey> {
        let command = modifiers.command();
        let shift = modifiers.shift();
        let motion = match key {
            Key::Character(c) if command && c.as_str().eq_ignore_ascii_case("c") => return Some(DiffKey::Copy),
            Key::Character(c) if command && c.as_str().eq_ignore_ascii_case("a") => return Some(DiffKey::SelectAll),
            Key::Named(Named::ArrowLeft) => Motion::Left,
            Key::Named(Named::ArrowRight) => Motion::Right,
            Key::Named(Named::ArrowUp) => Motion::Up,
            Key::Named(Named::ArrowDown) => Motion::Down,
            Key::Named(Named::Home) if command => Motion::Top,
            Key::Named(Named::End) if command => Motion::Bottom,
            Key::Named(Named::Home) => Motion::Home,
            Key::Named(Named::End) => Motion::End,
            Key::Named(Named::PageUp) => Motion::PageUp(self.page),
            Key::Named(Named::PageDown) => Motion::PageDown(self.page),
            _ => return None,
        };
        Some(DiffKey::Move(motion, shift))
    }
}

impl canvas::Program<Message> for DiffOverlay {
    type State = Pointer;

    fn update(
        &self,
        state: &mut Self::State,
        event: Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> (event::Status, Option<Message>) {
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let hit = cursor.position_in(bounds).and_then(|at| self.hit(at, bounds.width));
                let Some((pane, pos)) = hit else {
                    // A press anywhere else takes the caret away from the diff.
                    state.dragging = false;
                    return (event::Status::Ignored, self.focused.then_some(Message::DiffBlur));
                };
                let now = Instant::now();
                let repeat = state
                    .last_press
                    .is_some_and(|(when, last, side)| now.duration_since(when) <= MULTI_CLICK && last == pos && side == pane);
                state.clicks = if repeat { state.clicks % 3 + 1 } else { 1 };
                state.last_press = Some((now, pos, pane));
                state.dragging = true;
                let grain = match state.clicks {
                    2 => Grain::Word,
                    3 => Grain::Line,
                    _ => Grain::Caret,
                };
                (event::Status::Ignored, Some(Message::DiffPress { pane, pos, grain }))
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) if state.dragging => {
                let (Some(held), Some(at)) = (self.selection, cursor.position()) else {
                    return (event::Status::Ignored, None);
                };
                let at = Point::new(at.x - bounds.x, at.y - bounds.y);
                let pos = self.drag_target(at, bounds.width, held.pane);
                (event::Status::Ignored, Some(Message::DiffDrag(pos)))
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.dragging = false;
                (event::Status::Ignored, None)
            }
            Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) if self.focused => {
                (event::Status::Ignored, self.key(&key, modifiers).map(Message::DiffKey))
            }
            _ => (event::Status::Ignored, None),
        }
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        if let Some(held) = self.selection {
            let (from, to) = self.columns.span(held.pane, bounds.width);
            let x_of = |col: usize| (from + col as f32 * self.char_width).min(to);
            let (start, end) = held.range();
            let wash = Color { a: 0.32, ..self.tokens.accent_primary };
            if !held.is_caret() {
                for line in start.line..=end.line {
                    if !self.text_rows.get(line).copied().unwrap_or(false) {
                        continue;
                    }
                    let Some(top) = self.tops.get(line) else { break };
                    let left = if line == start.line { x_of(start.col) } else { from };
                    let right = if line == end.line { x_of(end.col) } else { to };
                    if right > left {
                        frame.fill_rectangle(Point::new(left, *top), Size::new(right - left, ROW_HEIGHT), wash);
                    }
                }
            }
            if self.focused {
                if let Some(top) = self.tops.get(held.caret.line) {
                    frame.fill_rectangle(
                        Point::new(x_of(held.caret.col), *top + 2.0),
                        Size::new(CARET_WIDTH, ROW_HEIGHT - 4.0),
                        self.tokens.text_primary,
                    );
                }
            }
        }
        vec![frame.into_geometry()]
    }

    fn mouse_interaction(&self, state: &Self::State, bounds: Rectangle, cursor: mouse::Cursor) -> mouse::Interaction {
        let over_text = cursor.position_in(bounds).is_some_and(|at| self.hit(at, bounds.width).is_some());
        if state.dragging || over_text {
            mouse::Interaction::Text
        } else {
            // Not ours: the button underneath shows its own cursor.
            mouse::Interaction::None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPLIT: Columns = Columns { split: true, gutter: 44.0, divider: 1.0 };
    const UNIFIED: Columns = Columns { split: false, gutter: 102.0, divider: 1.0 };

    #[test]
    fn each_side_of_a_split_diff_has_its_own_text_span() {
        assert_eq!(SPLIT.span(Pane::Left, 801.0), (44.0, 400.0));
        assert_eq!(SPLIT.span(Pane::Right, 801.0), (445.0, 801.0));
        assert_eq!(UNIFIED.span(Pane::Unified, 801.0), (102.0, 801.0));
    }

    #[test]
    fn a_point_belongs_to_the_column_it_is_over_gutter_included() {
        assert_eq!(SPLIT.pane_at(10.0, 801.0), Pane::Left, "the left gutter");
        assert_eq!(SPLIT.pane_at(399.0, 801.0), Pane::Left);
        assert_eq!(SPLIT.pane_at(410.0, 801.0), Pane::Right, "the right gutter");
        assert_eq!(UNIFIED.pane_at(700.0, 801.0), Pane::Unified);
    }

    #[test]
    fn a_point_maps_to_the_nearest_character_boundary() {
        // Characters 8 px wide: boundaries at 44, 52, 60…
        assert_eq!(SPLIT.col_at(Pane::Left, 44.0, 801.0, 8.0), 0);
        assert_eq!(SPLIT.col_at(Pane::Left, 47.0, 801.0, 8.0), 0, "nearer the boundary before");
        assert_eq!(SPLIT.col_at(Pane::Left, 49.0, 801.0, 8.0), 1, "nearer the boundary after");
        assert_eq!(SPLIT.col_at(Pane::Left, 5.0, 801.0, 8.0), 0, "in the gutter: the start of the line");
        assert_eq!(SPLIT.col_at(Pane::Right, 445.0 + 80.0, 801.0, 8.0), 10);
        assert_eq!(SPLIT.col_at(Pane::Left, 100.0, 801.0, 0.0), 0, "no width known yet");
    }

    fn overlay() -> DiffOverlay {
        DiffOverlay {
            tops: vec![0.0, 26.0, 50.0, 70.0, 90.0],
            text_rows: vec![false, false, true, true],
            columns: SPLIT,
            char_width: 8.0,
            selection: None,
            focused: true,
            page: 20,
            tokens: DesignTokens::deep_focus(),
        }
    }

    #[test]
    fn a_press_lands_on_text_rows_only() {
        let overlay = overlay();
        assert_eq!(overlay.hit(Point::new(60.0, 55.0), 801.0), Some((Pane::Left, Pos { line: 2, col: 2 })));
        assert_eq!(overlay.hit(Point::new(60.0, 30.0), 801.0), None, "a hunk row keeps its buttons");
        assert_eq!(overlay.hit(Point::new(60.0, 500.0), 801.0), None, "below the last line");
    }

    #[test]
    fn a_drag_past_either_end_still_names_a_line() {
        let overlay = overlay();
        assert_eq!(overlay.drag_target(Point::new(60.0, -40.0), 801.0, Pane::Left).line, 0);
        assert_eq!(overlay.drag_target(Point::new(60.0, 9_000.0), 801.0, Pane::Left).line, 3);
    }

    #[test]
    fn the_keys_of_a_read_only_editor_are_answered() {
        let overlay = overlay();
        let none = keyboard::Modifiers::empty();
        let ctrl = keyboard::Modifiers::COMMAND;
        assert_eq!(overlay.key(&Key::Character("c".into()), ctrl), Some(DiffKey::Copy));
        assert_eq!(overlay.key(&Key::Character("A".into()), ctrl), Some(DiffKey::SelectAll));
        assert_eq!(overlay.key(&Key::Character("c".into()), none), None, "typing does nothing: read-only");
        assert_eq!(overlay.key(&Key::Named(Named::ArrowDown), keyboard::Modifiers::SHIFT), Some(DiffKey::Move(Motion::Down, true)));
        assert_eq!(overlay.key(&Key::Named(Named::End), ctrl), Some(DiffKey::Move(Motion::Bottom, false)));
        assert_eq!(overlay.key(&Key::Named(Named::PageDown), none), Some(DiffKey::Move(Motion::PageDown(20), false)));
    }
}
