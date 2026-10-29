use crate::app::{GitApp, Message};
use crate::theme::DesignTokens;
use iced::widget::{column, container, row, text, text_editor, text_input};
use iced::{Background, Border, Color, Element, Font, Length, Shadow, Theme};

use super::chrome::{command_button, field_style, Reading};

const PANE_HEIGHT: f32 = 160.0;
const PROMPT_MARK: &str = "PS ";

pub(super) fn terminal_pane<'a>(
    app: &'a GitApp,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let bar = row![
        text("Terminal")
            .font(reading.font)
            .size(reading.size)
            .style(move |_| text::Style {
                color: Some(tokens.accent_primary),
            }),
        command_button("Clear", Message::ClearTerminal, true, tokens, reading),
        command_button(
            if app.terminal_open { "Hide" } else { "Show" },
            Message::ToggleTerminal,
            true,
            tokens,
            reading,
        ),
    ]
    .spacing(8)
    .padding([4, 8]);

    let mut pane = column![bar].width(Length::Fill);
    if app.terminal_open {
        let colours = ShellColours {
            prompt: tokens.accent_primary,
            error: tokens.removed,
            output: tokens.text_primary,
        };
        let placeholder = if app.shell_busy {
            "Running…"
        } else {
            "Type a command and press Enter"
        };
        let mut input = text_input(placeholder, &app.shell_input)
            .font(reading.mono)
            .size(reading.size)
            .padding([4, 8])
            .on_input(Message::ShellInputChanged)
            .style(field_style(tokens));
        if !app.shell_busy {
            input = input.on_submit(Message::ShellSubmit);
        }
        pane = pane
            .push(
                text_editor(&app.terminal_log)
                    .font(reading.mono)
                    .size(reading.size)
                    .padding(8)
                    .placeholder("Command output appears here.")
                    .on_action(Message::TerminalEdited)
                    .highlight_with::<ShellHighlighter>(colours, shell_format)
                    .style(move |_theme, _status| shell_style(tokens))
                    .height(Length::Fill),
            )
            .push(
                row![
                    text(">")
                        .font(reading.mono)
                        .size(reading.size)
                        .style(move |_| text::Style {
                            color: Some(tokens.accent_primary),
                        }),
                    input,
                ]
                .spacing(6)
                .padding([4, 8])
                .align_y(iced::Alignment::Center),
            );
    }

    container(pane)
        .width(Length::Fill)
        .height(if app.terminal_open {
            Length::Fixed(PANE_HEIGHT)
        } else {
            Length::Shrink
        })
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(tokens.bg_app)),
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

#[derive(Debug, Clone, Copy, PartialEq)]
struct ShellColours {
    prompt: Color,
    error: Color,
    output: Color,
}

struct ShellHighlighter {
    colours: ShellColours,
    line: usize,
}

impl iced::advanced::text::Highlighter for ShellHighlighter {
    type Settings = ShellColours;
    type Highlight = Color;
    type Iterator<'a> = std::option::IntoIter<(std::ops::Range<usize>, Color)>;

    fn new(settings: &Self::Settings) -> Self {
        Self {
            colours: *settings,
            line: 0,
        }
    }

    fn update(&mut self, new_settings: &Self::Settings) {
        self.colours = *new_settings;
    }

    fn change_line(&mut self, line: usize) {
        self.line = line;
    }

    fn highlight_line(&mut self, line: &str) -> Self::Iterator<'_> {
        self.line = self.line.saturating_add(1);
        if line.is_empty() {
            return None.into_iter();
        }
        Some((0..line.len(), shell_line_colour(line, &self.colours))).into_iter()
    }

    fn current_line(&self) -> usize {
        self.line
    }
}

fn shell_format(colour: &Color, _theme: &Theme) -> iced::advanced::text::highlighter::Format<Font> {
    iced::advanced::text::highlighter::Format {
        color: Some(*colour),
        font: None,
    }
}

fn shell_line_colour(line: &str, colours: &ShellColours) -> Color {
    if line.starts_with(PROMPT_MARK) {
        colours.prompt
    } else if shell_error(line) {
        colours.error
    } else {
        colours.output
    }
}

fn shell_error(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    [
        "error",
        "failed",
        "fatal",
        "not connected",
        "no model",
        "interrupted",
        "exit ",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn shell_style(tokens: DesignTokens) -> text_editor::Style {
    text_editor::Style {
        background: Background::Color(tokens.bg_app),
        border: Border::default(),
        icon: tokens.text_muted,
        placeholder: tokens.text_muted,
        value: tokens.text_primary,
        selection: tokens.accent_primary,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn colours() -> ShellColours {
        ShellColours {
            prompt: Color::from_rgb(1.0, 0.5, 0.0),
            error: Color::from_rgb(1.0, 0.2, 0.2),
            output: Color::from_rgb(0.9, 0.9, 0.9),
        }
    }

    #[test]
    fn shell_lines_colour_prompts_errors_and_output() {
        let colours = colours();
        assert_eq!(shell_line_colour("PS Kilo", &colours), colours.prompt);
        assert_eq!(
            shell_line_colour("model list is not connected", &colours),
            colours.error
        );
        assert_eq!(
            shell_line_colour("No model is configured.", &colours),
            colours.error
        );
        assert_eq!(shell_line_colour("status 200", &colours), colours.output);
    }
}
