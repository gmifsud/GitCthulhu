//! The lost-stash dialog: stash commits git still holds but no `stash@{n}` names, each with an
//! Apply button. Opened by the "Find lost stashes" action; Esc or a click outside closes it.

use crate::app::{GitApp, LostStashesView, Message};
use crate::selection::short_hash;
use crate::theme::DesignTokens;
use core_domain::LostStash;
use iced::widget::{column, container, mouse_area, opaque, row, scrollable, text};
use iced::{Alignment, Background, Border, Color, Element, Length, Shadow, Theme};

use super::chrome::{accent_button, quiet_button, Reading};

const SHEET_WIDTH: f32 = 600.0;
const SHEET_TOP: f32 = 56.0;
const LIST_HEIGHT: f32 = 420.0;
/// Shown when the search finds nothing.
pub(crate) const EMPTY_TEXT: &str =
    "No lost stashes. Dropped stashes stay findable until git garbage-collects them.";
const SEARCHING_TEXT: &str = "Searching…";

pub(super) fn lost_stashes_overlay<'a>(
    app: &'a GitApp,
    tokens: DesignTokens,
    reading: Reading,
) -> Option<Element<'a, Message>> {
    let view = app.lost_stashes.as_ref()?;
    let small = (reading.size - 2.0).max(10.0);
    let muted = move |_: &Theme| text::Style { color: Some(tokens.text_muted) };

    let body: Element<'a, Message> = match view {
        LostStashesView::Searching => text(SEARCHING_TEXT).font(reading.font).size(reading.size).style(muted).into(),
        LostStashesView::Failed(reason) => text(reason.as_str())
            .font(reading.font)
            .size(reading.size)
            .style(move |_| text::Style { color: Some(tokens.text_primary) })
            .into(),
        LostStashesView::Found(found) if found.is_empty() => {
            text(EMPTY_TEXT).font(reading.font).size(reading.size).style(muted).into()
        }
        LostStashesView::Found(found) => {
            let now = crate::activity::unix_now();
            // Applying waits for the tab's own reload, and for an apply already running.
            let can_apply = !app.lost_stash_applying && !app.session().state.loading;
            let mut list = column![].spacing(2);
            for stash in found {
                let applied = app.lost_stashes_applied.contains(&stash.commit_id);
                list = list.push(stash_row(stash, now, applied, can_apply, small, tokens, reading));
            }
            container(scrollable(list).height(Length::Shrink)).max_height(LIST_HEIGHT).into()
        }
    };

    let title = text("Lost stashes")
        .font(reading.font)
        .size(reading.size + 2.0)
        .style(move |_| text::Style { color: Some(tokens.text_primary) });
    let footer = row![
        text("Apply keeps the stash findable. Esc closes.").font(reading.font).size(small).style(muted),
        container(quiet_button("Close", Message::CloseLostStashes, true, tokens, reading))
            .width(Length::Fill)
            .align_right(Length::Fill),
    ]
    .align_y(Alignment::Center);

    let panel = container(column![title, body, footer].spacing(10))
        .padding(14)
        .width(Length::Fixed(SHEET_WIDTH))
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(tokens.bg_chrome)),
            text_color: Some(tokens.text_primary),
            border: Border { color: tokens.chip_border, width: 1.0, radius: 6.0.into() },
            shadow: Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.5),
                offset: iced::Vector::new(0.0, 6.0),
                blur_radius: 18.0,
            },
        });

    // A click on the dimmed backdrop closes the dialog; `opaque` keeps a click on the panel
    // from reaching the backdrop.
    let backdrop = mouse_area(
        container(column![opaque(panel)])
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(iced::Padding { top: SHEET_TOP, ..iced::Padding::ZERO })
            .center_x(Length::Fill)
            .style(move |_theme: &Theme| container::Style {
                background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.35))),
                ..container::Style::default()
            }),
    )
    .on_press(Message::CloseLostStashes);
    Some(backdrop.into())
}

fn stash_row<'a>(
    stash: &'a LostStash,
    now: i64,
    applied: bool,
    can_apply: bool,
    small: f32,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let action: Element<'a, Message> = if applied {
        text("Applied").font(reading.font).size(small).style(move |_| text::Style { color: Some(tokens.text_muted) }).into()
    } else {
        accent_button("Apply", Message::ApplyLostStash(stash.commit_id.clone()), can_apply, tokens, reading)
    };
    row![
        text(stash.message.as_str())
            .font(reading.font)
            .size(reading.size)
            .width(Length::Fill)
            .style(move |_| text::Style { color: Some(tokens.text_primary) }),
        text(crate::dates::log_date(now, stash.timestamp))
            .font(reading.font)
            .size(small)
            .style(move |_| text::Style { color: Some(tokens.text_secondary) }),
        text(short_hash(&stash.commit_id))
            .font(reading.mono)
            .size(small)
            .style(move |_| text::Style { color: Some(tokens.text_muted) }),
        action,
    ]
    .spacing(12)
    .padding([2, 8])
    .align_y(Alignment::Center)
    .into()
}
