//! The Activity tab of the bottom panel: everything the open repository has done, newest
//! first: what ran, the git command it amounts to, how long it took, and how it ended.

use crate::activity::{duration_label, Entry};
use crate::app::{GitApp, Message};
use crate::icons::Icon;
use crate::theme::DesignTokens;
use core_domain::ActivityOutcome;
use iced::widget::text::Wrapping;
use iced::widget::{column, container, row, scrollable, text};
use iced::{Alignment, Background, Border, Color, Element, Length, Theme};

use super::chrome::{glyph, Reading};

/// Rows drawn at most; the log keeps more than anyone scrolls through.
const MAX_ROWS: usize = 300;
const TIME_WIDTH: f32 = 116.0;
const DURATION_WIDTH: f32 = 76.0;

pub(super) fn activity_list<'a>(app: &'a GitApp, background: Color, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    let small = (reading.size - 1.0).max(10.0);
    let Some(repository) = app.repository_root() else {
        return note("Open a repository to see its activity.", tokens, reading);
    };
    let now = crate::activity::unix_now();
    let entries: Vec<&Entry> = app
        .activity
        .for_repository(&repository)
        .into_iter()
        .filter(|entry| {
            !app.activity_failures_only
                || (matches!(entry.record.outcome, ActivityOutcome::Failed(_)) && !entry.is_running())
        })
        .take(MAX_ROWS)
        .collect();
    if entries.is_empty() {
        return note(
            if app.activity_failures_only {
                "No failures recorded for this repository."
            } else {
                "Nothing yet. Fetch, pull, push, stash, commit or run a command and it shows up here."
            },
            tokens,
            reading,
        );
    }
    let mut list = column![].spacing(1).padding([4, 12]);
    for entry in entries {
        list = list.push(entry_row(entry, now, small, tokens, reading));
    }
    container(scrollable(list).height(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(background)),
            ..container::Style::default()
        })
        .into()
}

fn entry_row<'a>(entry: &'a Entry, now: i64, small: f32, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    let record = &entry.record;
    let (icon, ink) = match (&entry.running, &record.outcome) {
        (Some(_), _) => (Icon::Refresh, tokens.accent_primary),
        (None, ActivityOutcome::Succeeded) => (Icon::Check, tokens.added),
        (None, ActivityOutcome::Failed(_)) => (Icon::Close, tokens.removed),
        (None, ActivityOutcome::Stopped) => (Icon::Stop, tokens.text_muted),
    };
    let duration = match &entry.running {
        Some((_, began)) => format!("{} s…", began.elapsed().as_secs()),
        None => duration_label(record.duration_ms),
    };
    let mut headline = row![
        glyph(icon, reading.size, ink, &reading),
        container(
            text(crate::dates::log_date(now, record.started))
                .font(reading.mono)
                .size(small)
                .style(move |_| text::Style { color: Some(tokens.text_muted) }),
        )
        .width(Length::Fixed(TIME_WIDTH)),
        text(record.title.clone())
            .font(iced::Font {
                weight: iced::font::Weight::Semibold,
                ..reading.font
            })
            .size(reading.size)
            .style(move |_| text::Style { color: Some(tokens.text_primary) }),
        container(
            text(record.detail.clone())
                .font(reading.font)
                .size(reading.size)
                .wrapping(Wrapping::None)
                .style(move |_| text::Style { color: Some(tokens.text_secondary) }),
        )
        .width(Length::Fill)
        .clip(true),
    ]
    .spacing(10)
    .align_y(Alignment::Center);
    if !record.worktree.is_empty() {
        headline = headline.push(
            text(format!("in {}", crate::session::basename(&record.worktree)))
                .font(reading.font)
                .size(small)
                .style(move |_| text::Style { color: Some(tokens.text_muted) }),
        );
    }
    headline = headline.push(
        container(
            text(duration)
                .font(reading.mono)
                .size(small)
                .style(move |_| text::Style { color: Some(tokens.text_muted) }),
        )
        .width(Length::Fixed(DURATION_WIDTH))
        .align_x(Alignment::End),
    );

    let indent = reading.size + 10.0 + TIME_WIDTH + 10.0;
    let mut block = column![headline].spacing(2);
    if !record.command.is_empty() {
        block = block.push(
            container(
                text(record.command.clone())
                    .font(reading.mono)
                    .size(small)
                    .wrapping(Wrapping::None)
                    .style(move |_| text::Style { color: Some(tokens.text_muted) }),
            )
            .padding(iced::Padding::ZERO.left(indent))
            .clip(true),
        );
    }
    if let ActivityOutcome::Failed(message) = &record.outcome {
        block = block.push(
            container(
                text(message.clone())
                    .font(reading.mono)
                    .size(small)
                    .style(move |_| text::Style { color: Some(tokens.removed) }),
            )
            .padding(iced::Padding::ZERO.left(indent)),
        );
    }
    let failed = matches!(record.outcome, ActivityOutcome::Failed(_)) && !entry.is_running();
    container(block)
        .width(Length::Fill)
        .padding([4, 6])
        .style(move |_theme: &Theme| container::Style {
            background: failed.then_some(Background::Color(Color { a: 0.35, ..tokens.removed_bg })),
            border: Border {
                radius: 3.0.into(),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

fn note<'a>(value: &'a str, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    container(
        text(value)
            .font(reading.font)
            .size(reading.size)
            .style(move |_| text::Style { color: Some(tokens.text_muted) }),
    )
    .padding([10, 14])
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}
