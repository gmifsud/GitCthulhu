//! The bottom panel: the interactive shell (a pseudoconsole running pwsh) and the
//! Activity log, under one bar that folds and unfolds the panel.

use crate::app::{BottomTab, GitApp, Message};
use crate::icons::Icon;
use crate::theme::DesignTokens;
use iced::widget::{column, container, horizontal_space, mouse_area, row, text};
use iced::{Alignment, Background, Border, Color, Element, Length, Shadow, Theme};

use super::chrome::{command_button, glyph, Reading};

/// The grab strip on the terminal's top edge.
const GRIP_HEIGHT: f32 = 4.0;
/// How much darker than the app background the console is.
const CONSOLE_SHADE: f32 = 0.72;

pub(super) fn terminal_pane<'a>(
    app: &'a GitApp,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let console_bg = shade(tokens.bg_app, CONSOLE_SHADE);
    let mut pane = column![
        grip(app.terminal_open, tokens),
        // The whole bar folds and unfolds the panel. Its buttons (tabs, Clear, Stop, Hide)
        // take their own clicks first, so Clear never also folds it.
        mouse_area(header(app, tokens, reading)).on_press(Message::ToggleTerminal),
    ].width(Length::Fill);

    if app.terminal_open && app.bottom_tab == BottomTab::Activity {
        pane = pane.push(super::activity::activity_list(app, console_bg, tokens, reading));
    } else if app.terminal_open {
        // A real shell on a pseudoconsole: typing, tab completion, history, colours and
        // interactive programs are the shell's own. A click inside gives it the keyboard.
        let body: Element<'a, Message> = match &app.pty {
            Some(shell) => container(terminal_widget::TerminalView::show(shell).map(Message::Pty))
                .padding([4, 8])
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
            None => mouse_area(
                container(
                    text(app.pty_error.clone().unwrap_or_else(|| "Click to start the shell.".to_string()))
                        .font(reading.font)
                        .size(reading.size)
                        .style(move |_| text::Style { color: Some(tokens.text_muted) }),
                )
                .padding(12)
                .width(Length::Fill)
                .height(Length::Fill),
            )
            .on_press(Message::RestartShell)
            .into(),
        };
        pane = pane.push(
            container(body)
                .width(Length::Fill)
                .height(Length::Fill)
                .style(move |_theme: &Theme| container::Style {
                    background: Some(Background::Color(console_bg)),
                    ..container::Style::default()
                }),
        );
    }

    container(pane)
        .width(Length::Fill)
        .height(if app.terminal_open {
            Length::Fixed(app.terminal_height)
        } else {
            Length::Shrink
        })
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(tokens.bg_chrome)),
            text_color: Some(tokens.text_primary),
            border: Border {
                color: tokens.border,
                width: 1.0,
                radius: 0.0.into(),
            },
            shadow: Shadow::default(),
        })
        .into()
}

/// The top edge. Dragging it resizes the open terminal.
fn grip<'a>(open: bool, tokens: DesignTokens) -> Element<'a, Message> {
    let strip = container(column![])
        .width(Length::Fill)
        .height(Length::Fixed(GRIP_HEIGHT))
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(tokens.bg_chrome)),
            ..container::Style::default()
        });
    let area = mouse_area(strip);
    if open {
        area.on_press(Message::TerminalDragStart)
            .interaction(iced::mouse::Interaction::ResizingVertically)
            .into()
    } else {
        area.into()
    }
}

fn header<'a>(app: &'a GitApp, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    let small = (reading.size - 1.0).max(10.0);
    let on_activity = app.bottom_tab == BottomTab::Activity;
    let failures = app
        .repository_root()
        .map(|repository| app.activity.fresh_failures(&repository))
        .unwrap_or(0);
    let mut activity_tab = row![
        glyph(Icon::ListFlat, reading.size, if on_activity { tokens.accent_primary } else { tokens.text_muted }, &reading),
        text("Activity").font(reading.font).size(reading.size),
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    if failures > 0 {
        activity_tab = activity_tab.push(count_badge(failures, tokens, reading));
    }
    let mut bar = row![
        tab(
            row![
                glyph(Icon::Terminal, reading.size, if on_activity { tokens.text_muted } else { tokens.accent_primary }, &reading),
                text("Terminal").font(reading.font).size(reading.size),
                text(crate::jobs::shell_name())
                    .font(reading.mono)
                    .size(small)
                    .style(move |_| text::Style { color: Some(tokens.text_muted) }),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
            app.terminal_open && !on_activity,
            Message::SetBottomTab(BottomTab::Terminal),
            tokens,
        ),
        tab(activity_tab, app.terminal_open && on_activity, Message::SetBottomTab(BottomTab::Activity), tokens),
        horizontal_space(),
    ]
    .spacing(4)
    .padding([2, 8])
    .align_y(Alignment::Center);
    if on_activity {
        bar = bar
            .push(command_button(
                if app.activity_failures_only { "Show all" } else { "Only failures" },
                Message::ToggleActivityFailuresOnly,
                true,
                tokens,
                reading,
            ))
            .push(command_button("Clear log", Message::ClearActivity, app.terminal_open, tokens, reading));
    } else {
        if app.shell_busy {
            bar = bar.push(super::chrome::labelled_button(
                Icon::Stop,
                "Stop".to_string(),
                Message::ShellInterrupt,
                true,
                false,
                tokens,
                reading,
            ));
        }
        bar = bar.push(command_button("Clear", Message::ClearTerminal, app.terminal_open, tokens, reading));
    }
    bar.push(command_button(
            if app.terminal_open { "Hide" } else { "Show" },
            Message::ToggleTerminal,
            true,
            tokens,
            reading,
        ))
        .into()
}

/// A bottom-panel tab: underlined in the accent when it is the one showing.
fn tab<'a>(
    label: impl Into<Element<'a, Message>>,
    active: bool,
    message: Message,
    tokens: DesignTokens,
) -> Element<'a, Message> {
    iced::widget::button(label)
        .padding([3, 10])
        .on_press(message)
        .style(move |_theme, status| {
            let hot = matches!(status, iced::widget::button::Status::Hovered | iced::widget::button::Status::Pressed);
            iced::widget::button::Style {
                background: Some(Background::Color(if active || hot { tokens.bg_raised } else { Color::TRANSPARENT })),
                text_color: if active { tokens.text_primary } else { tokens.text_secondary },
                border: Border {
                    color: if active { tokens.accent_primary } else { Color::TRANSPARENT },
                    width: if active { 1.0 } else { 0.0 },
                    radius: 4.0.into(),
                },
                shadow: Shadow::default(),
            }
        })
        .into()
}

/// A small red count, for failures in this repository since it was opened.
fn count_badge<'a>(count: usize, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    container(
        text(count.to_string())
            .font(reading.font)
            .size((reading.size - 3.0).max(9.0))
            .style(|_| text::Style { color: Some(Color::WHITE) }),
    )
    .padding([0, 5])
    .style(move |_theme: &Theme| container::Style {
        background: Some(Background::Color(tokens.removed)),
        border: Border {
            radius: 8.0.into(),
            ..Border::default()
        },
        ..container::Style::default()
    })
    .into()
}

fn shade(colour: Color, by: f32) -> Color {
    Color {
        r: colour.r * by,
        g: colour.g * by,
        b: colour.b * by,
        a: colour.a,
    }
}
