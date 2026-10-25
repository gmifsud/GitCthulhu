use crate::app::Message;
use crate::appearance::{font_family, font_size};
use crate::theme::DesignTokens;
use core_domain::AppSettings;
use iced::widget::button::Status;
use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, row, text};
// mouse_area + tooltip used by icon_button
use iced::{
    font::Family, Alignment, Background, Border, Color, Element, Font, Length, Shadow, Theme,
};

/// Fold / menu / branch marks live in Segoe UI Symbol. UI faces (Segoe UI,
/// Familjen Grotesk) often omit them, which is why tree and branch chrome looked blank.
pub(super) const FOLD_CLOSED: &str = "▸";
pub(super) const FOLD_OPEN: &str = "▾";
pub(super) const MENU_CLOSED: &str = "▾";
pub(super) const MENU_OPEN: &str = "▴";
pub(super) const BRANCH_MARK: &str = "⎇";
pub(super) const SEARCH_MARK: &str = "⌕";
pub(super) const REMOTE_MARK: &str = "↑";
pub(super) const TAG_MARK: &str = "◆";
pub(super) const STASH_MARK: &str = "≡";

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

pub(super) fn fold_mark(folded: bool) -> &'static str {
    if folded {
        FOLD_CLOSED
    } else {
        FOLD_OPEN
    }
}

/// One chrome glyph, always drawn with the symbol face.
pub(super) fn glyph<'a>(
    mark: &'a str,
    size: f32,
    color: Color,
    reading: &Reading,
) -> Element<'a, Message> {
    text(mark)
        .font(symbol_reading(reading).font)
        .size(size)
        .line_height(1.0)
        .style(move |_| text::Style {
            color: Some(color),
        })
        .into()
}

/// Branch control: symbol + name + chevron on a pill chip (wireframe `.chip.branch`).
pub(super) fn branch_chip<'a>(
    name: impl Into<String>,
    message: Message,
    enabled: bool,
    open: bool,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let chevron = if open { MENU_OPEN } else { MENU_CLOSED };
    let label = name.into();
    button(
        row![
            glyph(
                BRANCH_MARK,
                reading.size,
                if enabled {
                    tokens.accent_primary
                } else {
                    tokens.text_muted
                },
                &reading,
            ),
            text(label)
                .font(reading.font)
                .size(reading.size)
                .wrapping(Wrapping::None)
                .style(move |_| text::Style {
                    color: Some(if enabled {
                        tokens.accent_primary
                    } else {
                        tokens.text_muted
                    }),
                }),
            glyph(
                chevron,
                (reading.size - 1.0).max(10.0),
                tokens.text_muted,
                &reading,
            ),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    )
    .padding([3, 10])
    .on_press_maybe(enabled.then_some(message))
    .style(move |_theme, status| {
        let hot = matches!(status, Status::Hovered | Status::Pressed);
        iced::widget::button::Style {
            background: Some(Background::Color(if hot && enabled {
                tokens.bg_raised
            } else {
                tokens.chip
            })),
            text_color: tokens.accent_primary,
            border: Border {
                color: if hot && enabled {
                    tokens.accent_primary
                } else {
                    tokens.chip_border
                },
                width: 1.0,
                radius: 999.0.into(),
            },
            shadow: Shadow::default(),
        }
    })
    .into()
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

pub(super) fn compact(reading: Reading) -> Reading {
    Reading {
        size: (reading.size - 2.0).max(11.0),
        ..reading
    }
}

pub(super) fn field_style(
    tokens: DesignTokens,
) -> impl Fn(&Theme, iced::widget::text_input::Status) -> iced::widget::text_input::Style {
    move |_theme, status| {
        let focused = matches!(status, iced::widget::text_input::Status::Focused);
        iced::widget::text_input::Style {
            background: Background::Color(tokens.bg_app),
            border: Border {
                color: if focused {
                    tokens.accent_primary
                } else {
                    tokens.chip_border
                },
                width: 1.0,
                radius: 4.0.into(),
            },
            icon: tokens.text_muted,
            placeholder: tokens.text_muted,
            value: tokens.text_primary,
            selection: tokens.accent_primary,
        }
    }
}

/// Text input drawn inside a chrome shell that already owns the border.
pub(super) fn embedded_field_style(
    tokens: DesignTokens,
) -> impl Fn(&Theme, iced::widget::text_input::Status) -> iced::widget::text_input::Style {
    move |_theme, _status| iced::widget::text_input::Style {
        background: Background::Color(Color::TRANSPARENT),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 0.0.into(),
        },
        icon: tokens.text_muted,
        placeholder: tokens.text_muted,
        value: tokens.text_primary,
        selection: tokens.accent_primary,
    }
}

pub(super) fn command_button<'a>(
    label: &'a str,
    message: Message,
    enabled: bool,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    button(text(label).font(reading.font).size(reading.size))
        .padding([2, 6])
        .on_press_maybe(enabled.then_some(message))
        .style(move |_theme, status| bordered_button(tokens, status, enabled, false))
        .into()
}

/// Compact symbol control for working-area toolbars.
///
/// Always wraps in `tooltip` so the hover target is never remounted when the
/// two-second tip becomes ready (that remount was clearing hover and flickering).
pub(super) fn icon_button<'a>(
    mark: &'static str,
    title: impl Into<String>,
    tip_key: impl Into<String>,
    tip_ready: bool,
    message: Message,
    enabled: bool,
    accent: bool,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    use iced::widget::{mouse_area, tooltip};

    let title = title.into();
    let tip_key = tip_key.into();
    let size = (reading.size + 1.0).max(12.0);
    let ink = if accent && enabled {
        tokens.text_primary
    } else if enabled {
        tokens.text_secondary
    } else {
        tokens.text_muted
    };
    let control = button(glyph(mark, size, ink, &reading))
        .padding([3, 5])
        .on_press_maybe(enabled.then_some(message))
        .style(move |_theme, status| bordered_button(tokens, status, enabled, accent));

    let sensed = mouse_area(control)
        .on_enter(Message::IconTipEnter {
            key: tip_key.clone(),
            title: title.clone(),
        })
        .on_exit(Message::IconTipLeave { key: tip_key });

    let tip: Element<'a, Message> = if tip_ready {
        container(
            text(title)
                .font(reading.font)
                .size((reading.size - 1.0).max(10.0)),
        )
        .padding([4, 8])
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(tokens.bg_app)),
            text_color: Some(tokens.text_primary),
            border: Border {
                color: tokens.border,
                width: 1.0,
                radius: 4.0.into(),
            },
            shadow: Shadow::default(),
        })
        .into()
    } else {
        // Keep a zero-size tip so the outer `tooltip` wrapper never swaps away.
        container(text("").size(1.0))
            .width(Length::Fixed(0.0))
            .height(Length::Fixed(0.0))
            .into()
    };

    tooltip(sensed, tip, tooltip::Position::Bottom)
        .gap(4)
        .into()
}

/// Borderless control for Open… / Settings — matches the wireframe `.cmd.quiet`.
pub(super) fn quiet_button<'a>(
    label: &'a str,
    message: Message,
    enabled: bool,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    button(text(label).font(reading.font).size(reading.size))
        .padding([2, 6])
        .on_press_maybe(enabled.then_some(message))
        .style(move |_theme, status| {
            let hot = matches!(status, Status::Hovered | Status::Pressed);
            iced::widget::button::Style {
                background: Some(Background::Color(Color::TRANSPARENT)),
                text_color: if !enabled {
                    tokens.text_muted
                } else if hot {
                    tokens.text_primary
                } else {
                    tokens.text_secondary
                },
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

pub(super) fn pane_title<'a>(
    label: impl Into<String>,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    text(label.into())
        .font(reading.font)
        .size((reading.size - 2.0).max(11.0))
        .style(move |_| text::Style {
            color: Some(tokens.text_muted),
        })
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
        .padding([2, 8])
        .on_press_maybe(enabled.then_some(message))
        .style(move |_theme, status| bordered_button(tokens, status, enabled, true))
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
