use crate::app::Message;
use crate::appearance::{font_family, font_size};
use crate::theme::DesignTokens;
use core_domain::AppSettings;
use iced::widget::button::Status;
use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, text};
use iced::{font::Family, Background, Border, Color, Element, Font, Length, Shadow, Theme};

/// The face and size chosen under Appearance. Hashes and paths stay monospace.
#[derive(Clone, Copy)]
pub(super) struct Reading {
    pub font: Font,
    pub mono: Font,
    pub size: f32,
}

pub(super) fn reading(settings: &AppSettings) -> Reading {
    let family = font_family(&settings.ui_font);
    let font = Font {
        family: Family::Name(family),
        ..Font::DEFAULT
    };
    let mono = if family == "Fragment Mono" {
        font
    } else {
        Font {
            family: Family::Monospace,
            ..Font::DEFAULT
        }
    };
    Reading {
        font,
        mono,
        size: f32::from(font_size(settings.ui_size)),
    }
}

/// Segoe UI does not contain the layout and rail glyphs. This face does.
const SYMBOL_FONT: &str = "Segoe UI Symbol";

pub(super) fn symbol_reading(reading: &Reading) -> Reading {
    Reading {
        font: Font {
            family: Family::Name(SYMBOL_FONT),
            ..reading.font
        },
        mono: reading.mono,
        size: reading.size,
    }
}

pub(super) fn section_label<'a>(
    tokens: DesignTokens,
    reading: Reading,
    title: &'static str,
) -> Element<'a, Message> {
    text(title)
        .font(reading.font)
        .size((reading.size - 2.0).max(11.0))
        .style(move |_| text::Style {
            color: Some(tokens.text_muted),
        })
        .into()
}

pub(super) fn panel_style(tokens: DesignTokens, ruled: bool) -> container::Style {
    container::Style {
        background: Some(Background::Color(if ruled {
            tokens.bg_sidebar
        } else {
            tokens.bg_app
        })),
        text_color: Some(tokens.text_secondary),
        border: Border {
            color: tokens.border,
            width: if ruled { 1.0 } else { 0.0 },
            radius: 0.0.into(),
        },
        shadow: Shadow::default(),
    }
}

pub(super) fn hairline(tokens: DesignTokens) -> Element<'static, Message> {
    container(column![])
        .width(Length::Fill)
        .height(Length::Fixed(1.0))
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(tokens.border)),
            text_color: None,
            border: Border::default(),
            shadow: Shadow::default(),
        })
        .into()
}

pub(super) fn command_button<'a>(
    label: &'a str,
    message: Message,
    enabled: bool,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    button(text(label).font(reading.font).size(reading.size))
        .padding([5, 10])
        .on_press_maybe(enabled.then_some(message))
        .style(move |_theme, status| bordered_button(tokens, status, enabled, false))
        .into()
}

pub(super) fn accent_button<'a>(
    label: impl Into<String>,
    message: Message,
    enabled: bool,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    button(text(label.into()).font(reading.font).size(reading.size))
        .padding([6, 12])
        .on_press_maybe(enabled.then_some(message))
        .style(move |_theme, status| bordered_button(tokens, status, enabled, true))
        .into()
}

pub(super) fn row_button<'a>(
    label: impl Into<String>,
    message: Message,
    selected: bool,
    accent: bool,
    tokens: DesignTokens,
    reading: Reading,
    mono: bool,
) -> Element<'a, Message> {
    let face = if mono { reading.mono } else { reading.font };
    let color = if accent {
        tokens.accent_primary
    } else if selected {
        tokens.text_primary
    } else {
        tokens.text_secondary
    };
    button(
        text(label.into())
            .font(face)
            .size(reading.size)
            .wrapping(Wrapping::None),
    )
    .width(Length::Fill)
    .padding([4, 8])
    .on_press(message)
    .style(move |_theme, status| {
        let hot = matches!(status, Status::Hovered | Status::Pressed);
        iced::widget::button::Style {
            background: Some(Background::Color(if selected || hot {
                tokens.border
            } else {
                Color::TRANSPARENT
            })),
            text_color: color,
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 4.0.into(),
            },
            shadow: Shadow::default(),
        }
    })
    .into()
}

fn bordered_button(
    tokens: DesignTokens,
    status: Status,
    enabled: bool,
    accent: bool,
) -> iced::widget::button::Style {
    let hot = matches!(status, Status::Hovered | Status::Pressed);
    let background = if accent && enabled {
        tokens.accent_primary
    } else if hot && enabled {
        tokens.border
    } else {
        tokens.bg_app
    };
    iced::widget::button::Style {
        background: Some(Background::Color(background)),
        text_color: if enabled {
            if accent {
                Color::BLACK
            } else {
                tokens.text_primary
            }
        } else {
            tokens.text_muted
        },
        border: Border {
            color: if accent && enabled {
                tokens.accent_primary
            } else {
                tokens.border
            },
            width: 1.0,
            radius: 4.0.into(),
        },
        shadow: Shadow::default(),
    }
}
