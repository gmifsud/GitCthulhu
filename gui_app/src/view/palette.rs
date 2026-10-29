//! Ctrl+P quick-open overlay: one field filtering branches, remotes, worktrees, tags,
//! stashes, changed files, loaded commits, and recent repositories.

use crate::app::{GitApp, Message, PALETTE_INPUT_ID};
use crate::palette::SCOPE_HINT;
use crate::theme::DesignTokens;
use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, mouse_area, row, scrollable, text, text_input};
use iced::{Alignment, Background, Border, Color, Element, Length, Shadow, Theme};

use super::chrome::{field_style, Reading};

const PALETTE_WIDTH: f32 = 640.0;
const PALETTE_TOP: f32 = 56.0;
const LIST_HEIGHT: f32 = 380.0;
const BADGE_WIDTH: f32 = 64.0;
/// iced 0.13 can wrap `Wrapping::None` text in a narrow cell; rows clamp to one line.
const ROW_HEIGHT: f32 = 22.0;
const PLACEHOLDER: &str = "Go to branch, tag, stash, worktree, file, commit, repository…";

pub(super) fn palette_overlay<'a>(
    app: &'a GitApp,
    tokens: DesignTokens,
    reading: Reading,
) -> Option<Element<'a, Message>> {
    let palette = app.palette.as_ref()?;
    let (items, ranked) = app.palette_results();
    let small = (reading.size - 2.0).max(10.0);

    let field = text_input(PLACEHOLDER, &palette.query)
        .id(PALETTE_INPUT_ID)
        .on_input(Message::PaletteQueryChanged)
        .on_submit(Message::PaletteSubmit)
        .font(reading.font)
        .size(reading.size)
        .padding([6, 10])
        .style(field_style(tokens));

    let mut list = column![].spacing(1);
    for (position, &index) in ranked.iter().enumerate() {
        let item = &items[index];
        let selected = position == palette.selected;
        let line = row![
            container(
                text(item.kind.badge())
                    .font(reading.mono)
                    .size(small)
                    .style(move |_| text::Style {
                        color: Some(tokens.text_muted),
                    }),
            )
            .width(Length::Fixed(BADGE_WIDTH)),
            container(
                text(item.label.clone())
                    .font(reading.font)
                    .size(reading.size)
                    .wrapping(Wrapping::None)
                    .style(move |_| text::Style {
                        color: Some(tokens.text_primary),
                    }),
            )
            .width(Length::FillPortion(3))
            .clip(true),
            container(
                text(item.detail.clone())
                    .font(reading.mono)
                    .size(small)
                    .wrapping(Wrapping::None)
                    .style(move |_| text::Style {
                        color: Some(tokens.text_secondary),
                    }),
            )
            .width(Length::FillPortion(2))
            .align_x(Alignment::End)
            .clip(true),
        ]
        .spacing(8)
        .height(Length::Fixed(ROW_HEIGHT))
        .align_y(Alignment::Center);
        let line = container(line).height(Length::Fixed(ROW_HEIGHT)).clip(true);
        list = list.push(
            button(line)
                .width(Length::Fill)
                .padding([4, 8])
                .on_press(Message::PaletteActivate(position))
                .style(move |_theme, status| {
                    let hot = matches!(
                        status,
                        iced::widget::button::Status::Hovered
                            | iced::widget::button::Status::Pressed
                    );
                    iced::widget::button::Style {
                        background: Some(Background::Color(if selected {
                            tokens.border
                        } else if hot {
                            tokens.bg_raised
                        } else {
                            Color::TRANSPARENT
                        })),
                        text_color: tokens.text_primary,
                        border: Border {
                            color: if selected {
                                tokens.accent_primary
                            } else {
                                Color::TRANSPARENT
                            },
                            width: 1.0,
                            radius: 3.0.into(),
                        },
                        shadow: Shadow::default(),
                    }
                }),
        );
    }
    if ranked.is_empty() {
        list = list.push(
            container(
                text("No matches")
                    .font(reading.font)
                    .size(reading.size)
                    .style(move |_| text::Style {
                        color: Some(tokens.text_muted),
                    }),
            )
            .padding([6, 8]),
        );
    }

    let footer = text(format!("↑↓ move · Enter open · Esc close   {SCOPE_HINT}"))
        .font(reading.font)
        .size(small)
        .wrapping(Wrapping::None)
        .style(move |_| text::Style {
            color: Some(tokens.text_muted),
        });

    let panel = container(
        column![
            field,
            container(scrollable(list).height(Length::Shrink)).max_height(LIST_HEIGHT),
            container(footer).clip(true),
        ]
        .spacing(8),
    )
    .padding(10)
    .width(Length::Fixed(PALETTE_WIDTH))
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

    // Clicking the dimmed backdrop closes the palette; clicks on the panel stay on it.
    // `PaletteMove(0)` changes nothing; it only stops a click on the panel's padding
    // from reaching the backdrop.
    let panel = mouse_area(panel).on_press(Message::PaletteMove(0));
    let backdrop = mouse_area(
        container(column![panel])
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(iced::Padding {
                top: PALETTE_TOP,
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
