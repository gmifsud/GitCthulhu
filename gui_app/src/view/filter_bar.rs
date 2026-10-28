//! The log's filter bar, after the JetBrains Git log: text search, Branch ▾, User ▾, and the
//! current-branch toggle. Every control narrows the History table; none changes the repository.

use crate::app::{GitApp, Message, HISTORY_SEARCH_ID};
use crate::log_filter::{author_choices, branch_choice, branch_choices, LogChoice};
use crate::theme::DesignTokens;
use iced::widget::{container, horizontal_space, pick_list, row, text, text_input};
use iced::{Alignment, Background, Border, Color, Element, Length, Shadow, Theme};

use super::chrome::{embedded_field_style, glyph, hairline, icon_button, Reading, SEARCH_MARK};

const SEARCH_PLACEHOLDER: &str = "Text or hash";
const SEARCH_SHORTCUT: &str = "Ctrl K";
const SEARCH_PORTION: u16 = 1;
const SEARCH_MAX_WIDTH: f32 = 320.0;
const FIELD_RADIUS: f32 = 4.0;
const BAR_SPACING: f32 = 6.0;
const BAR_PADDING: [f32; 2] = [4.0, 8.0];
const PICK_MAX_WIDTH: f32 = 220.0;
const FOCUS_TIP_KEY: &str = "graph-focus";
const FOCUS_MARK: &str = "◎";

pub(super) fn filter_bar<'a>(app: &'a GitApp, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    let session = app.session();
    let focus = session.focus_current_branch;
    let branch = branch_choice(focus, session.branch_filter.as_deref());
    let author = session
        .author_filter
        .clone()
        .map_or(LogChoice::All, LogChoice::Named);
    let bar = row![
        search_field(&app.history_query, tokens, reading),
        picker("Branch", branch_choices(&session.state.branches), branch, Message::BranchChoicePicked, tokens, reading),
        picker("User", author_choices(&session.state.commits), author, Message::AuthorChoicePicked, tokens, reading),
        horizontal_space(),
        icon_button(
            FOCUS_MARK,
            if focus { "Show all branches" } else { "Show only the current branch" },
            FOCUS_TIP_KEY,
            app.tip_ready(FOCUS_TIP_KEY),
            Message::ToggleFocusCurrentBranch,
            true,
            focus,
            tokens,
            reading,
        ),
    ]
    .spacing(BAR_SPACING)
    .align_y(Alignment::Center);
    iced::widget::column![container(bar).padding(BAR_PADDING).width(Length::Fill), hairline(tokens)].into()
}

/// "Label: value ▾". The value reads in the accent colour while a filter is set.
fn picker<'a>(
    label: &'static str,
    options: Vec<LogChoice>,
    selected: LogChoice,
    on_pick: fn(LogChoice) -> Message,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let active = selected != LogChoice::All;
    row![
        text(format!("{label}:"))
            .font(reading.font)
            .size(reading.size)
            .style(move |_| text::Style { color: Some(tokens.text_muted) }),
        container(
            pick_list(options, Some(selected), on_pick)
                .font(reading.font)
                .text_size(reading.size)
                .padding([2, 6])
                .style(move |_theme: &Theme, _status| pick_list::Style {
                    text_color: if active { tokens.accent_primary } else { tokens.text_primary },
                    placeholder_color: tokens.text_muted,
                    handle_color: tokens.text_secondary,
                    background: Background::Color(Color::TRANSPARENT),
                    border: Border { color: tokens.chip_border, width: 1.0, radius: FIELD_RADIUS.into() },
                }),
        )
        .max_width(PICK_MAX_WIDTH),
    ]
    .spacing(4)
    .align_y(Alignment::Center)
    .into()
}

fn search_field<'a>(
    query: &'a str,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    container(
        row![
            glyph(
                SEARCH_MARK,
                reading.size,
                tokens.text_secondary,
                &reading,
            ),
            text_input(SEARCH_PLACEHOLDER, query)
                .id(HISTORY_SEARCH_ID)
                .on_input(Message::HistoryQueryChanged)
                .font(reading.font)
                .size(reading.size)
                .padding(0)
                .width(Length::Fill)
                .style(embedded_field_style(tokens)),
            container(
                text(SEARCH_SHORTCUT)
                    .font(reading.mono)
                    .size((reading.size - 2.0).max(10.0))
                    .style(move |_| text::Style {
                        color: Some(tokens.text_muted),
                    }),
            )
            .padding([1, 5])
            .style(move |_theme: &Theme| container::Style {
                background: Some(Background::Color(Color::TRANSPARENT)),
                text_color: Some(tokens.text_muted),
                border: Border {
                    color: tokens.chip_border,
                    width: 1.0,
                    radius: 3.0.into(),
                },
                shadow: Shadow::default(),
            }),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding([4, 10])
    .width(Length::FillPortion(SEARCH_PORTION))
    .max_width(SEARCH_MAX_WIDTH)
    .style(move |_theme: &Theme| container::Style {
        background: Some(Background::Color(tokens.bg_app)),
        text_color: Some(tokens.text_secondary),
        border: Border {
            color: tokens.chip_border,
            width: 1.0,
            radius: FIELD_RADIUS.into(),
        },
        shadow: Shadow::default(),
    })
    .into()
}
