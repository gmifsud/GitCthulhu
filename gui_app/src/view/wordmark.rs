//! The app name at the left of the header, in the style chosen under Appearance.

use crate::app::Message;
use crate::icons::Icon;
use crate::theme::DesignTokens;
use core_domain::HeaderMark;
use iced::font::Weight;
use iced::widget::{container, row, text};
use iced::{Alignment, Background, Border, Color, Element, Font, Length, Theme};

use super::chrome::{glyph, Reading};

const BADGE_SIZE: f32 = 20.0;
const BADGE_RADIUS: f32 = 5.0;
/// Thin space (U+2009) between small capitals, since iced has no letter spacing.
const THIN_SPACE: &str = "\u{2009}";

/// The label each style shows in Settings.
pub(crate) fn mark_label(mark: HeaderMark) -> &'static str {
    match mark {
        HeaderMark::TwoTone => "Two-tone wordmark",
        HeaderMark::Badge => "Badge and name",
        HeaderMark::SmallCaps => "Spaced capitals",
        HeaderMark::Prompt => "Shell prompt",
        HeaderMark::BadgeOnly => "Badge only",
    }
}

pub(super) fn wordmark<'a>(mark: HeaderMark, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    let size = reading.size + 1.0;
    let bold = Font {
        weight: Weight::Bold,
        ..reading.font
    };
    let ink = |colour: Color| move |_: &Theme| text::Style { color: Some(colour) };
    match mark {
        HeaderMark::TwoTone => row![
            text("Git").font(bold).size(size).style(ink(tokens.text_secondary)),
            text("Cthulhu").font(bold).size(size).style(ink(tokens.accent_primary)),
        ]
        .align_y(Alignment::Center)
        .into(),
        HeaderMark::Badge => row![
            badge(tokens, reading),
            text("GitCthulhu").font(bold).size(size).style(ink(tokens.text_primary)),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into(),
        HeaderMark::SmallCaps => text(spaced("GITCTHULHU"))
            .font(Font {
                weight: Weight::Semibold,
                ..reading.font
            })
            .size((reading.size - 1.0).max(10.0))
            .style(ink(tokens.text_muted))
            .into(),
        HeaderMark::Prompt => row![
            glyph(Icon::ChevronRight, reading.size, tokens.accent_primary, &reading),
            text("gitcthulhu").font(reading.mono).size(reading.size).style(ink(tokens.text_primary)),
        ]
        .spacing(4)
        .align_y(Alignment::Center)
        .into(),
        HeaderMark::BadgeOnly => badge(tokens, reading),
    }
}

/// An accent tile with a heavy "G": readable at 20 px and still recognisable as the taskbar icon's colour.
fn badge<'a>(tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    container(
        text("G")
            .font(Font {
                weight: Weight::Black,
                ..reading.font
            })
            .size(reading.size + 1.0)
            .style(|_| text::Style {
                color: Some(Color::BLACK),
            }),
    )
    .width(Length::Fixed(BADGE_SIZE))
    .height(Length::Fixed(BADGE_SIZE))
    .center_x(Length::Fixed(BADGE_SIZE))
    .center_y(Length::Fixed(BADGE_SIZE))
    .style(move |_theme: &Theme| container::Style {
        background: Some(Background::Color(tokens.accent_primary)),
        border: Border {
            radius: BADGE_RADIUS.into(),
            ..Border::default()
        },
        ..container::Style::default()
    })
    .into()
}

fn spaced(word: &str) -> String {
    word.chars()
        .map(String::from)
        .collect::<Vec<_>>()
        .join(THIN_SPACE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spaced_capitals_put_a_thin_space_between_letters_only() {
        assert_eq!(spaced("GIT"), "G\u{2009}I\u{2009}T");
    }

    #[test]
    fn every_mark_has_a_settings_label() {
        for mark in HeaderMark::ALL {
            assert!(!mark_label(mark).is_empty());
        }
    }
}
