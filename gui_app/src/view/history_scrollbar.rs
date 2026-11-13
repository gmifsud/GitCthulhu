//! The history's own scrollbar.
//!
//! iced sizes a scrollable's thumb in proportion to its content and lets it shrink to two
//! pixels. A history is tens of thousands of rows, so the built-in thumb was a hairline that
//! could not be grabbed. This bar keeps the thumb at least [`MIN_THUMB`] tall and maps its
//! travel onto the whole list.
//!
//! It sends a message only while the button is held on it: a pointer merely passing over
//! costs nothing (ADR 0007).

use iced::mouse;
use iced::widget::canvas::{self, event, Event};
use iced::{Color, Point, Rectangle, Renderer, Size, Theme};

use crate::app::Message;
use crate::theme::DesignTokens;

/// Width the bar takes beside the list.
pub(crate) const BAR_WIDTH: f32 = 12.0;
/// The thumb is never shorter than this, however long the history.
pub(crate) const MIN_THUMB: f32 = 40.0;
const THUMB_INSET: f32 = 3.0;

/// Where the thumb sits in a track of a given height.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Thumb {
    pub top: f32,
    pub height: f32,
}

/// The thumb for a list `content` tall, seen through `viewport`, scrolled to `scroll`.
/// `None` when the whole list fits: there is nothing to scroll.
pub(crate) fn thumb(track: f32, viewport: f32, content: f32, scroll: f32) -> Option<Thumb> {
    let reach = content - viewport;
    if track <= 0.0 || viewport <= 0.0 || reach <= 0.5 {
        return None;
    }
    let height = (track * viewport / content).max(MIN_THUMB).min(track);
    let travel = track - height;
    let top = travel * (scroll / reach).clamp(0.0, 1.0);
    Some(Thumb { top, height })
}

/// The scroll offset that puts the thumb's top at `top`, and whether that is the very end.
pub(crate) fn offset_at(top: f32, thumb_height: f32, track: f32, viewport: f32, content: f32) -> (f32, bool) {
    let reach = (content - viewport).max(0.0);
    let travel = track - thumb_height;
    if travel <= 0.0 {
        return (0.0, false);
    }
    let fraction = (top / travel).clamp(0.0, 1.0);
    (reach * fraction, fraction >= 1.0)
}

/// The bar for one frame: where the list is, and its colours.
pub(crate) struct HistoryScrollbar {
    /// Height of everything in the list.
    pub content: f32,
    pub scroll: f32,
    pub tokens: DesignTokens,
}

/// Held by the pointer: how far below the thumb's top it was grabbed.
#[derive(Debug, Default)]
pub(crate) struct Grab(Option<f32>);

impl HistoryScrollbar {
    fn scroll_to(&self, top: f32, thumb_height: f32, bounds: Rectangle) -> Message {
        let (offset, end) = offset_at(top, thumb_height, bounds.height, bounds.height, self.content);
        Message::HistoryScrollTo {
            offset,
            end,
            height: bounds.height,
            content: self.content,
        }
    }
}

impl canvas::Program<Message> for HistoryScrollbar {
    type State = Grab;

    fn update(
        &self,
        state: &mut Self::State,
        event: Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> (event::Status, Option<Message>) {
        let Event::Mouse(event) = event else {
            return (event::Status::Ignored, None);
        };
        let Some(thumb) = thumb(bounds.height, bounds.height, self.content, self.scroll) else {
            state.0 = None;
            return (event::Status::Ignored, None);
        };
        match event {
            mouse::Event::ButtonPressed(mouse::Button::Left) => {
                let Some(at) = cursor.position_in(bounds) else {
                    return (event::Status::Ignored, None);
                };
                let on_thumb = at.y >= thumb.top && at.y <= thumb.top + thumb.height;
                if on_thumb {
                    state.0 = Some(at.y - thumb.top);
                    return (event::Status::Captured, None);
                }
                // A press on the track brings the thumb's middle under the pointer, and the
                // drag carries on from there.
                let grab = thumb.height / 2.0;
                state.0 = Some(grab);
                (event::Status::Captured, Some(self.scroll_to(at.y - grab, thumb.height, bounds)))
            }
            mouse::Event::CursorMoved { .. } => {
                let (Some(grab), Some(at)) = (state.0, cursor.position()) else {
                    return (event::Status::Ignored, None);
                };
                let top = at.y - bounds.y - grab;
                (event::Status::Captured, Some(self.scroll_to(top, thumb.height, bounds)))
            }
            mouse::Event::ButtonReleased(mouse::Button::Left) => {
                state.0 = None;
                (event::Status::Ignored, None)
            }
            _ => (event::Status::Ignored, None),
        }
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        if let Some(thumb) = thumb(bounds.height, bounds.height, self.content, self.scroll) {
            let hot = state.0.is_some() || cursor.is_over(bounds);
            let ink: Color = if hot { self.tokens.text_muted } else { self.tokens.border };
            let width = bounds.width - THUMB_INSET * 2.0;
            let shape = canvas::Path::rounded_rectangle(
                Point::new(THUMB_INSET, thumb.top + 1.0),
                Size::new(width, (thumb.height - 2.0).max(1.0)),
                (width / 2.0).into(),
            );
            frame.fill(&shape, ink);
        }
        vec![frame.into_geometry()]
    }

    fn mouse_interaction(&self, state: &Self::State, bounds: Rectangle, cursor: mouse::Cursor) -> mouse::Interaction {
        if state.0.is_some() {
            mouse::Interaction::Grabbing
        } else if cursor.is_over(bounds) && thumb(bounds.height, bounds.height, self.content, self.scroll).is_some() {
            mouse::Interaction::Grab
        } else {
            mouse::Interaction::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_very_long_history_still_has_a_thumb_that_can_be_grabbed() {
        // 50,000 rows of 28 px behind a 600 px view: in proportion the thumb would be 0.26 px.
        let thumb = thumb(600.0, 600.0, 1_400_000.0, 0.0).expect("scrollable");
        assert_eq!(thumb.height, MIN_THUMB);
        assert_eq!(thumb.top, 0.0);
    }

    #[test]
    fn a_short_list_keeps_a_proportional_thumb_and_a_fitting_one_has_none() {
        let half = thumb(600.0, 600.0, 1_200.0, 0.0).expect("scrollable");
        assert_eq!(half.height, 300.0);
        assert!(thumb(600.0, 600.0, 600.0, 0.0).is_none());
        assert!(thumb(600.0, 600.0, 200.0, 0.0).is_none());
    }

    #[test]
    fn the_thumb_reaches_the_bottom_exactly_at_the_end_of_the_list() {
        let content = 1_400_000.0;
        let end = thumb(600.0, 600.0, content, content - 600.0).expect("scrollable");
        assert_eq!(end.top + end.height, 600.0);
        // Past the end (a longer list was on screen a moment ago) it stays in the track.
        let past = thumb(600.0, 600.0, content, content * 2.0).expect("scrollable");
        assert_eq!(past.top + past.height, 600.0);
    }

    #[test]
    fn dragging_the_thumb_maps_its_travel_onto_the_whole_list() {
        let (content, track) = (1_400_000.0, 600.0);
        let travel = track - MIN_THUMB;
        assert_eq!(offset_at(0.0, MIN_THUMB, track, track, content), (0.0, false));
        assert_eq!(offset_at(travel, MIN_THUMB, track, track, content), (content - track, true));
        let (middle, end) = offset_at(travel / 2.0, MIN_THUMB, track, track, content);
        assert_eq!(middle, (content - track) / 2.0);
        assert!(!end);
        // Dragged beyond either end of the track, it stops at that end.
        assert_eq!(offset_at(-50.0, MIN_THUMB, track, track, content).0, 0.0);
        assert_eq!(offset_at(9_999.0, MIN_THUMB, track, track, content), (content - track, true));
    }

    #[test]
    fn where_the_thumb_is_drawn_and_where_a_drag_puts_it_agree() {
        let (content, track) = (90_000.0, 500.0);
        for scroll in [0.0, 1_234.0, 44_750.0, 89_500.0] {
            let thumb = thumb(track, track, content, scroll).expect("scrollable");
            let (offset, _) = offset_at(thumb.top, thumb.height, track, track, content);
            assert!((offset - scroll).abs() < 0.5, "{scroll} came back as {offset}");
        }
    }
}
