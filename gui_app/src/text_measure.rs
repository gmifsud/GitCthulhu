//! Exact text sizes from iced's own text engine (cosmic-text), so a row can be made as tall as
//! its wrapped text needs before it is drawn. The history list builds only the rows in view and
//! must know every row's height up front; guessing from an average glyph width either clipped
//! names or wasted space.
//!
//! Results are cached per (text, font, size, width): the list is laid out on every redraw, and
//! shaping the same subjects again each frame would be the slow part.

use std::cell::RefCell;
use std::collections::HashMap;

use iced::advanced::text::{LineHeight, Paragraph as _, Shaping, Text, Wrapping};
use iced::{alignment, Font, Pixels, Size};

/// Entries kept before the cache is emptied and starts again.
const CACHE_LIMIT: usize = 20_000;

#[derive(Clone, PartialEq, Eq, Hash)]
struct Key {
    text: String,
    font: Font,
    size_tenths: u32,
    width_px: u32,
}

thread_local! {
    static CACHE: RefCell<HashMap<Key, Size>> = RefCell::new(HashMap::new());
}

/// The size `text` takes at `size` px in `font`, wrapped to `width` px (between words, or
/// inside a word too long for the line), as iced will draw it.
pub(crate) fn measure(text: &str, font: Font, size: f32, width: f32) -> Size {
    let key = Key {
        text: text.to_string(),
        font,
        size_tenths: (size * 10.0).round() as u32,
        width_px: width.max(1.0).floor() as u32,
    };
    if let Some(found) = CACHE.with(|cache| cache.borrow().get(&key).copied()) {
        return found;
    }
    let paragraph = iced::advanced::graphics::text::Paragraph::with_text(Text {
        content: text,
        bounds: Size::new(width.max(1.0), f32::INFINITY),
        size: Pixels(size),
        line_height: LineHeight::default(),
        font,
        horizontal_alignment: alignment::Horizontal::Left,
        vertical_alignment: alignment::Vertical::Top,
        shaping: Shaping::Basic,
        wrapping: Wrapping::WordOrGlyph,
    });
    let measured = paragraph.min_bounds();
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() >= CACHE_LIMIT {
            cache.clear();
        }
        cache.insert(key, measured);
    });
    measured
}

/// Width of `text` on one line.
pub(crate) fn width(text: &str, font: Font, size: f32) -> f32 {
    measure(text, font, size, f32::MAX / 4.0).width
}

/// Height of `text` wrapped to `width`.
pub(crate) fn height(text: &str, font: Font, size: f32, width: f32) -> f32 {
    measure(text, font, size, width).height
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn longer_text_is_wider_and_a_narrow_width_wraps_it_onto_more_lines() {
        let short = width("main", Font::DEFAULT, 14.0);
        let long = width("feature/tenant-db-isolation-shared-instance", Font::DEFAULT, 14.0);
        assert!(short > 0.0 && long > short * 4.0, "{short} vs {long}");
        let one_line = height("feature/tenant-db-isolation-shared-instance", Font::DEFAULT, 14.0, 2_000.0);
        let wrapped = height("feature/tenant-db-isolation-shared-instance", Font::DEFAULT, 14.0, 80.0);
        assert!(wrapped >= one_line * 2.0, "{one_line} vs {wrapped}");
    }
}
