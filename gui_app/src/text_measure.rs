//! Exact text sizes from iced's own text engine (cosmic-text), so a row can be made as tall as
//! its wrapped text needs before it is drawn. The history list builds only the rows in view and
//! must know every row's height up front; guessing from an average glyph width either clipped
//! names or wasted space.
//!
//! Results are cached per (text, font, size, width): the list is laid out on every redraw, and
//! shaping the same subjects again each frame would be the slow part. The text is the inner
//! key, so a hit is looked up by `&str` and copies nothing.

use std::cell::RefCell;
use std::collections::HashMap;

use iced::advanced::text::{LineHeight, Paragraph as _, Shaping, Text, Wrapping};
use iced::{alignment, Font, Pixels, Size};

/// Entries kept before the cache is emptied and starts again.
const CACHE_LIMIT: usize = 20_000;

/// Everything but the text: one map of texts per style.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Style {
    font: Font,
    size_tenths: u32,
    width_px: u32,
}

#[derive(Default)]
struct Cache {
    by_style: HashMap<Style, HashMap<String, Size>>,
    entries: usize,
}

thread_local! {
    static CACHE: RefCell<Cache> = RefCell::new(Cache::default());
}

/// The size `text` takes at `size` px in `font`, wrapped to `width` px (between words, or
/// inside a word too long for the line), as iced will draw it.
pub(crate) fn measure(text: &str, font: Font, size: f32, width: f32) -> Size {
    let style = Style {
        font,
        size_tenths: (size * 10.0).round() as u32,
        width_px: width.max(1.0).floor() as u32,
    };
    let hit = CACHE.with(|cache| {
        cache
            .borrow()
            .by_style
            .get(&style)
            .and_then(|texts| texts.get(text))
            .copied()
    });
    if let Some(found) = hit {
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
        if cache.entries >= CACHE_LIMIT {
            cache.by_style.clear();
            cache.entries = 0;
        }
        if cache
            .by_style
            .entry(style)
            .or_default()
            .insert(text.to_string(), measured)
            .is_none()
        {
            cache.entries += 1;
        }
    });
    measured
}

/// Width of `text` on one line.
pub(crate) fn width(text: &str, font: Font, size: f32) -> f32 {
    measure(text, font, size, f32::MAX / 4.0).width
}

/// Allowance on top of the summed advances, against rounding in the text engine.
const FIT_MARGIN: f32 = 2.0;

/// True when `text` certainly fits on one line `width` wide, worked out without laying it out.
///
/// Measured text is laid out with basic shaping, which places each glyph at its own advance:
/// no kerning, no ligatures. So a line's width is the sum of its characters' advances, and for
/// printable ASCII those come from a table measured once per font and size. Any other
/// character (accents, CJK, emoji, a tab) answers false and the text is laid out as before.
pub(crate) fn fits_on_one_line(text: &str, font: Font, size: f32, width: f32) -> bool {
    if text.is_empty() || !text.bytes().all(|byte| (b' '..=b'~').contains(&byte)) {
        return false;
    }
    ADVANCES.with(|tables| {
        let mut tables = tables.borrow_mut();
        let key = (font, (size * 10.0).round() as u32);
        let table = tables.entry(key).or_insert_with(|| ascii_advances(font, size));
        let total: f32 = text.bytes().map(|byte| table[usize::from(byte - b' ')]).sum();
        total + FIT_MARGIN <= width
    })
}

/// Each printable ASCII character's advance at `size` in `font`, measured as a run of ten so
/// rounding of a single narrow glyph does not shrink it.
fn ascii_advances(font: Font, size: f32) -> [f32; 95] {
    let mut table = [0.0; 95];
    for (slot, byte) in (b' '..=b'~').enumerate() {
        let run: String = std::iter::repeat(byte as char).take(10).collect();
        // A run of spaces measures as nothing (trailing whitespace): measure it between bars.
        table[slot] = if byte == b' ' {
            (measure_unwrapped(&format!("|{run}|"), font, size) - measure_unwrapped("||", font, size)) / 10.0
        } else {
            measure_unwrapped(&run, font, size) / 10.0
        };
    }
    table
}

fn measure_unwrapped(text: &str, font: Font, size: f32) -> f32 {
    measure(text, font, size, f32::MAX / 4.0).width
}

/// `text` on one line `width` wide: whole when it fits, else cut and ended with "…".
///
/// Called for every row in view on every frame, so plain-ASCII text is cut with the advance
/// table (no layout at all); anything else is laid out, and those layouts are cached.
pub(crate) fn ellipsize(text: &str, font: Font, size: f32, width: f32) -> String {
    if text.is_empty() || width <= 0.0 {
        return String::new();
    }
    let ascii = text.bytes().all(|byte| (b' '..=b'~').contains(&byte));
    if fits_on_one_line(text, font, size, width) || (!ascii && measure_unwrapped(text, font, size) + FIT_MARGIN <= width) {
        return text.to_string();
    }
    let room = width - measure_unwrapped(ELLIPSIS, font, size) - FIT_MARGIN;
    if room <= 0.0 {
        return ELLIPSIS.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let keep = if ascii {
        ADVANCES.with(|tables| {
            let mut tables = tables.borrow_mut();
            let table = tables.entry((font, (size * 10.0).round() as u32)).or_insert_with(|| ascii_advances(font, size));
            let mut used = 0.0;
            chars
                .iter()
                .take_while(|&&c| {
                    used += table[usize::from(c as u8 - b' ')];
                    used <= room
                })
                .count()
        })
    } else {
        // The longest prefix that fits, by halving: laid-out widths grow with length.
        let (mut low, mut high) = (0, chars.len());
        while low < high {
            let middle = (low + high + 1) / 2;
            let prefix: String = chars[..middle].iter().collect();
            if measure_unwrapped(&prefix, font, size) <= room {
                low = middle;
            } else {
                high = middle - 1;
            }
        }
        low
    };
    let mut cut: String = chars[..keep].iter().collect::<String>().trim_end().to_string();
    cut.push_str(ELLIPSIS);
    cut
}

const ELLIPSIS: &str = "…";

thread_local! {
    /// Printable ASCII advances per (font, size in tenths).
    static ADVANCES: RefCell<HashMap<(Font, u32), [f32; 95]>> = RefCell::new(HashMap::new());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn longer_text_is_wider_and_a_narrow_width_wraps_it_onto_more_lines() {
        let short = width("main", Font::DEFAULT, 14.0);
        let long = width("feature/tenant-db-isolation-shared-instance", Font::DEFAULT, 14.0);
        assert!(short > 0.0 && long > short * 4.0, "{short} vs {long}");
        let one_line = measure("feature/tenant-db-isolation-shared-instance", Font::DEFAULT, 14.0, 2_000.0).height;
        let wrapped = measure("feature/tenant-db-isolation-shared-instance", Font::DEFAULT, 14.0, 80.0).height;
        assert!(wrapped >= one_line * 2.0, "{one_line} vs {wrapped}");
    }

    /// The shortcut must agree with real layout: never "fits" for text that wraps, and close
    /// enough that text which fits is not sent to layout for nothing.
    #[test]
    fn the_one_line_shortcut_agrees_with_layout() {
        let samples = [
            "Merge branch 'feature/tenant-db-isolation-shared-instance' into feature/tenant-db-isolation-shared-instance_LaneB",
            "fix(tenant-isolation): validate tenant IDs for host-side resolution",
            "docs: update CIM target branches to eyesel integration",
            "WWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWW",
            "iiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiii",
            "a  b   c    d     e      f       {tabs and spaces}  ~!@#$%^&*()_+`-=[]\\;',./",
        ];
        for size in [12.0, 14.0, 18.0] {
            for text in samples {
                let real = width(text, Font::DEFAULT, size);
                let real_lines = |at: f32| measure(text, Font::DEFAULT, size, at).height;
                let one = real_lines(f32::MAX / 4.0);
                for at in [real * 0.5, real - 1.0, real + FIT_MARGIN + 1.0, real * 1.5] {
                    if fits_on_one_line(text, Font::DEFAULT, size, at) {
                        assert_eq!(real_lines(at), one, "said to fit but wraps: {text:?} at {at} (size {size})");
                    }
                }
                assert!(
                    fits_on_one_line(text, Font::DEFAULT, size, real + FIT_MARGIN + 1.0),
                    "fits by layout but not by the shortcut: {text:?} ({real} px, size {size})"
                );
            }
        }
        assert!(!fits_on_one_line("café", Font::DEFAULT, 14.0, 10_000.0), "not ASCII: laid out instead");
        assert!(!fits_on_one_line("", Font::DEFAULT, 14.0, 10_000.0));
    }

    #[test]
    fn text_too_long_is_cut_with_an_ellipsis_and_what_is_left_fits() {
        for text in ["feature/tenant-db-isolation-shared-instance_LaneB", "Gregory Mifsud", "café crème brûlée au caramel"] {
            let whole = width(text, Font::DEFAULT, 13.0);
            assert_eq!(ellipsize(text, Font::DEFAULT, 13.0, whole + 10.0), text, "it fits: kept whole");
            for room in [whole * 0.3, whole * 0.6, whole - 4.0] {
                let cut = ellipsize(text, Font::DEFAULT, 13.0, room);
                assert!(cut.ends_with('…'), "{cut:?}");
                assert!(width(&cut, Font::DEFAULT, 13.0) <= room + 0.5, "{cut:?} is wider than {room}");
                assert!(text.starts_with(cut.trim_end_matches('…')), "{cut:?} is not a start of {text:?}");
            }
        }
        assert_eq!(ellipsize("anything", Font::DEFAULT, 13.0, 0.0), "");
        assert_eq!(ellipsize("", Font::DEFAULT, 13.0, 100.0), "");
    }

    #[test]
    fn the_same_text_measures_the_same_from_the_cache_and_fresh() {
        let fresh = measure("feature/x", Font::DEFAULT, 14.0, 120.0);
        let cached = measure("feature/x", Font::DEFAULT, 14.0, 120.0);
        assert_eq!(fresh, cached);
        let other_width = measure("feature/x", Font::DEFAULT, 14.0, 30.0);
        assert!(other_width.height >= fresh.height, "a narrower width is its own entry");
    }
}

#[cfg(test)]
mod no_wrap_tests {
    use iced::advanced::text::{LineHeight, Paragraph as _, Shaping, Text, Wrapping};
    use iced::{alignment, Font, Pixels, Size};

    fn height(wrapping: Wrapping, width: f32) -> f32 {
        iced::advanced::graphics::text::Paragraph::with_text(Text {
            content: "seventy-eight files",
            bounds: Size::new(width, f32::INFINITY),
            size: Pixels(14.0),
            line_height: LineHeight::default(),
            font: Font::DEFAULT,
            horizontal_alignment: alignment::Horizontal::Left,
            vertical_alignment: alignment::Vertical::Top,
            shaping: Shaping::Advanced,
            wrapping,
        })
        .min_bounds()
        .height
    }

    /// The bug behind every "squashed column": iced 0.13.0 ignored `Wrapping::None`, so text
    /// squeezed into a few pixels stood one letter per line. vendor/iced_graphics fixes it;
    /// this fails if the build ever goes back to the unpatched crate.
    #[test]
    fn text_told_not_to_wrap_stays_on_one_line_however_narrow() {
        let one_line = height(Wrapping::None, 10_000.0);
        assert_eq!(height(Wrapping::None, 8.0), one_line);
        assert!(height(Wrapping::WordOrGlyph, 8.0) > one_line * 5.0, "wrapping text still wraps");
    }
}
