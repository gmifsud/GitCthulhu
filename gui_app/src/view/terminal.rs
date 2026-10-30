//! The terminal: a console under the workspace. Scrollback on top, the prompt and command
//! line underneath it, the way a shell reads. Each line is coloured by what printed it.

use std::sync::Arc;

use crate::app::{GitApp, Message, SHELL_INPUT_ID};
use crate::icons::Icon;
use crate::terminal::LineKind;
use crate::theme::DesignTokens;
use iced::widget::{column, container, horizontal_space, mouse_area, row, text, text_editor, text_input};
use iced::{Alignment, Background, Border, Color, Element, Font, Length, Shadow, Theme};

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
    let mut pane = column![grip(app.terminal_open, tokens), header(app, tokens, reading)].width(Length::Fill);

    if app.terminal_open {
        let colours = ShellColours {
            prompt: tokens.accent_primary,
            command: tokens.text_primary,
            error: tokens.removed,
            output: tokens.text_secondary,
            note: tokens.text_muted,
            kinds: KindList(app.console.kinds()),
        };
        let log = text_editor(&app.terminal_log)
            .font(reading.mono)
            .size(reading.size)
            .padding([6, 12])
            .on_action(Message::TerminalEdited)
            .highlight_with::<ShellHighlighter>(colours, shell_format)
            .style(move |_theme, _status| text_editor::Style {
                background: Background::Color(console_bg),
                border: Border::default(),
                icon: tokens.text_muted,
                placeholder: tokens.text_muted,
                value: tokens.text_secondary,
                selection: Color { a: 0.45, ..tokens.accent_primary },
            })
            .height(Length::Fill);

        let placeholder = if app.shell_busy {
            "running… Ctrl+C stops it"
        } else {
            ""
        };
        let mut input = text_input(placeholder, &app.shell_input)
            .id(text_input::Id::new(SHELL_INPUT_ID))
            .font(reading.mono)
            .size(reading.size)
            .padding(0)
            .style(move |_theme, _status| text_input::Style {
                background: Background::Color(Color::TRANSPARENT),
                border: Border::default(),
                icon: tokens.text_muted,
                placeholder: tokens.text_muted,
                value: tokens.text_primary,
                selection: Color { a: 0.45, ..tokens.accent_primary },
            });
        if !app.shell_busy {
            input = input.on_input(Message::ShellInputChanged).on_submit(Message::ShellSubmit);
        }
        let prompt_ink = if app.shell_busy { tokens.text_muted } else { tokens.accent_primary };
        let command_line = mouse_area(
            container(
                row![
                    text(app.shell_prompt())
                        .font(reading.mono)
                        .size(reading.size)
                        .style(move |_| text::Style { color: Some(prompt_ink) }),
                    input,
                ]
                .align_y(Alignment::Center),
            )
            .width(Length::Fill)
            .padding([4, 12]),
        )
        .on_press(Message::FocusShell);

        pane = pane.push(
            container(column![log, command_line])
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
    let mut bar = row![
        glyph(Icon::Terminal, reading.size, tokens.accent_primary, &reading),
        text("Terminal")
            .font(reading.font)
            .size(reading.size)
            .style(move |_| text::Style { color: Some(tokens.text_primary) }),
        text(crate::jobs::shell_name())
            .font(reading.mono)
            .size(small)
            .style(move |_| text::Style { color: Some(tokens.text_muted) }),
        horizontal_space(),
    ]
    .spacing(8)
    .padding([3, 10])
    .align_y(Alignment::Center);
    if app.shell_busy {
        bar = bar.push(
            super::chrome::labelled_button(
                Icon::Stop,
                "Stop".to_string(),
                Message::ShellInterrupt,
                true,
                false,
                tokens,
                reading,
            ),
        );
    }
    bar.push(command_button("Clear", Message::ClearTerminal, app.terminal_open, tokens, reading))
        .push(command_button(
            if app.terminal_open { "Hide" } else { "Show" },
            Message::ToggleTerminal,
            true,
            tokens,
            reading,
        ))
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

/// One kind per line, shared with the console. Compared by pointer: the console replaces the
/// list on every change, so a new pointer is exactly "the lines changed".
#[derive(Debug, Clone)]
struct KindList(Arc<Vec<LineKind>>);

impl PartialEq for KindList {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

#[derive(Debug, Clone, PartialEq)]
struct ShellColours {
    prompt: Color,
    command: Color,
    error: Color,
    output: Color,
    note: Color,
    kinds: KindList,
}

/// Colours each line by the kind the console recorded for it. iced highlights lines in order
/// from `current_line`, so the line index is the position in the kind list.
struct ShellHighlighter {
    colours: ShellColours,
    line: usize,
}

impl iced::advanced::text::Highlighter for ShellHighlighter {
    type Settings = ShellColours;
    type Highlight = Color;
    type Iterator<'a> = std::vec::IntoIter<(std::ops::Range<usize>, Color)>;

    fn new(settings: &Self::Settings) -> Self {
        Self {
            colours: settings.clone(),
            line: 0,
        }
    }

    fn update(&mut self, new_settings: &Self::Settings) {
        // Appends leave earlier lines' kinds alone, so recolour only from the first line whose
        // kind changed. A clear or trim shifts everything and restarts near the top.
        let changed_from = first_difference(&self.colours.kinds.0, &new_settings.kinds.0);
        self.line = self.line.min(changed_from);
        self.colours = new_settings.clone();
    }

    fn change_line(&mut self, line: usize) {
        self.line = line;
    }

    fn highlight_line(&mut self, line: &str) -> Self::Iterator<'_> {
        let index = self.line;
        self.line = self.line.saturating_add(1);
        let kind = self.colours.kinds.0.get(index).copied().unwrap_or(LineKind::Output);
        line_spans(line, kind, &self.colours).into_iter()
    }

    fn current_line(&self) -> usize {
        self.line
    }
}

/// Index of the first line whose kind differs, or the shorter length when one list extends
/// the other.
fn first_difference(old: &[LineKind], new: &[LineKind]) -> usize {
    old.iter()
        .zip(new)
        .position(|(a, b)| a != b)
        .unwrap_or_else(|| old.len().min(new.len()))
}

/// A prompt line is the prompt in the accent and the command in the primary colour; every
/// other line is one colour.
fn line_spans(line: &str, kind: LineKind, colours: &ShellColours) -> Vec<(std::ops::Range<usize>, Color)> {
    if line.is_empty() {
        return Vec::new();
    }
    match kind {
        LineKind::Prompt => match prompt_end(line) {
            Some(end) => vec![(0..end, colours.prompt), (end..line.len(), colours.command)],
            None => vec![(0..line.len(), colours.prompt)],
        },
        LineKind::Error => vec![(0..line.len(), colours.error)],
        LineKind::Note => vec![(0..line.len(), colours.note)],
        LineKind::Output => vec![(0..line.len(), colours.output)],
    }
}

/// Byte offset just after the prompt's `> ` (PowerShell) or `$ ` (POSIX).
fn prompt_end(line: &str) -> Option<usize> {
    ["> ", "$ "]
        .iter()
        .filter_map(|mark| line.find(mark).map(|at| at + mark.len()))
        .min()
}

fn shell_format(colour: &Color, _theme: &Theme) -> iced::advanced::text::highlighter::Format<Font> {
    iced::advanced::text::highlighter::Format {
        color: Some(*colour),
        font: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn colours() -> ShellColours {
        ShellColours {
            prompt: Color::from_rgb(1.0, 0.5, 0.0),
            command: Color::WHITE,
            error: Color::from_rgb(1.0, 0.2, 0.2),
            output: Color::from_rgb(0.8, 0.8, 0.8),
            note: Color::from_rgb(0.5, 0.5, 0.5),
            kinds: KindList(Arc::new(Vec::new())),
        }
    }

    #[test]
    fn a_prompt_line_splits_into_prompt_and_command() {
        let c = colours();
        let line = "PS D:\\Repos\\x [main]> git status";
        let spans = line_spans(line, LineKind::Prompt, &c);
        let split = line.find("> ").unwrap() + 2;
        assert_eq!(spans, vec![(0..split, c.prompt), (split..line.len(), c.command)]);
    }

    #[test]
    fn other_lines_take_their_kind_colour() {
        let c = colours();
        assert_eq!(line_spans("fatal: no", LineKind::Error, &c), vec![(0..9, c.error)]);
        assert_eq!(line_spans("[Kilo] hi", LineKind::Note, &c), vec![(0..9, c.note)]);
        assert!(line_spans("", LineKind::Output, &c).is_empty());
    }

    #[test]
    fn recolouring_restarts_where_the_kinds_first_differ() {
        use LineKind::{Error, Output, Prompt};
        assert_eq!(first_difference(&[Prompt, Output], &[Prompt, Output, Error]), 2);
        assert_eq!(first_difference(&[Prompt, Output], &[Output, Error]), 0);
        assert_eq!(first_difference(&[Prompt, Output], &[]), 0);
    }

    #[test]
    fn kind_lists_compare_by_identity() {
        let shared = Arc::new(vec![LineKind::Output]);
        assert_eq!(KindList(Arc::clone(&shared)), KindList(Arc::clone(&shared)));
        assert_ne!(KindList(shared), KindList(Arc::new(vec![LineKind::Output])));
    }
}
