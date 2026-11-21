//! Ctrl+/ shortcuts sheet: every catalogue action with its key, grouped by category.

use crate::actions::ACTIONS;
use crate::app::{GitApp, Message};
use crate::theme::DesignTokens;
use iced::widget::{column, container, mouse_area, opaque, row, scrollable, text};
use iced::{Alignment, Background, Border, Color, Element, Length, Shadow, Theme};

use super::chrome::Reading;

const SHEET_WIDTH: f32 = 520.0;
const SHEET_TOP: f32 = 56.0;
const LIST_HEIGHT: f32 = 460.0;
/// Shown where an action has no default key.
const NO_CHORD: &str = "no shortcut";

pub(super) fn shortcuts_overlay<'a>(
    app: &'a GitApp,
    tokens: DesignTokens,
    reading: Reading,
) -> Option<Element<'a, Message>> {
    if !app.shortcuts_open {
        return None;
    }
    let small = (reading.size - 2.0).max(10.0);

    // Categories in the order the catalogue first names them.
    let mut categories: Vec<&'static str> = Vec::new();
    for spec in ACTIONS {
        if !categories.contains(&spec.category) {
            categories.push(spec.category);
        }
    }

    let mut list = column![].spacing(2);
    for category in categories {
        list = list.push(
            container(
                text(category)
                    .font(reading.font)
                    .size(small)
                    .style(move |_| text::Style {
                        color: Some(tokens.text_muted),
                    }),
            )
            .padding(iced::Padding {
                top: 10.0,
                bottom: 2.0,
                ..iced::Padding::ZERO
            }),
        );
        for spec in ACTIONS.iter().filter(|spec| spec.category == category) {
            let chord = spec.chord.map(|chord| chord.label());
            let bound = chord.is_some();
            list = list.push(
                row![
                    text(spec.label)
                        .font(reading.font)
                        .size(reading.size)
                        .width(Length::Fill)
                        .style(move |_| text::Style {
                            color: Some(tokens.text_primary),
                        }),
                    text(chord.unwrap_or_else(|| NO_CHORD.to_string()))
                        .font(reading.mono)
                        .size(small)
                        .style(move |_| text::Style {
                            color: Some(if bound {
                                tokens.text_secondary
                            } else {
                                tokens.text_muted
                            }),
                        }),
                ]
                .spacing(12)
                .padding([2, 8])
                .align_y(Alignment::Center),
            );
        }
    }

    let title = text("Keyboard shortcuts")
        .font(reading.font)
        .size(reading.size + 2.0)
        .style(move |_| text::Style {
            color: Some(tokens.text_primary),
        });
    let footer = text("Esc close")
        .font(reading.font)
        .size(small)
        .style(move |_| text::Style {
            color: Some(tokens.text_muted),
        });

    let panel = container(
        column![
            title,
            container(scrollable(list).height(Length::Shrink)).max_height(LIST_HEIGHT),
            footer,
        ]
        .spacing(8),
    )
    .padding(14)
    .width(Length::Fixed(SHEET_WIDTH))
    .style(move |_theme: &Theme| container::Style {
        background: Some(Background::Color(tokens.bg_chrome)),
        text_color: Some(tokens.text_primary),
        border: Border {
            color: tokens.chip_border,
            width: 1.0,
            radius: 6.0.into(),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.5),
            offset: iced::Vector::new(0.0, 6.0),
            blur_radius: 18.0,
        },
    });

    // Clicking the dimmed backdrop closes the sheet; `opaque` keeps a click on the panel
    // from reaching the backdrop.
    let panel = opaque(panel);
    let backdrop = mouse_area(
        container(column![panel])
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(iced::Padding {
                top: SHEET_TOP,
                ..iced::Padding::ZERO
            })
            .center_x(Length::Fill)
            .style(move |_theme: &Theme| container::Style {
                background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.35))),
                ..container::Style::default()
            }),
    )
    .on_press(Message::Escape);
    Some(backdrop.into())
}
