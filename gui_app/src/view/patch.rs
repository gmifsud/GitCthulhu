use crate::app::{GitApp, Message};
use crate::theme::DesignTokens;
use core_domain::DiffLineKind;
use iced::widget::{column, container, row, scrollable, text};
use iced::{Alignment, Element, Length};

use super::chrome::{command_button, Reading};

pub(super) fn patch_pane<'a>(
    app: &'a GitApp,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let mut body = column![].spacing(8).padding([8, 16]);
    if app.patch_pending {
        body = body.push(note("Loading…", tokens, reading));
    } else if let Some(error) = &app.patch_error {
        body = body.push(note(error, tokens, reading));
    } else if app.patches.is_empty() {
        body = body.push(note("No changes", tokens, reading));
    } else {
        for patch in &app.patches {
            body = body.push(
                text(patch.path.clone())
                    .font(reading.mono)
                    .size(reading.size)
                    .style(move |_| text::Style {
                        color: Some(tokens.accent_primary),
                    }),
            );
            if patch.binary {
                body = body.push(note("Binary file", tokens, reading));
                continue;
            }
            for (index, hunk) in patch.hunks.iter().enumerate() {
                let path = patch.path.clone();
                body = body.push(
                    text(hunk.header.clone())
                        .font(reading.mono)
                        .size((reading.size - 2.0).max(11.0))
                        .style(move |_| text::Style {
                            color: Some(tokens.text_muted),
                        }),
                );
                for line in &hunk.lines {
                    let (mark, color) = match line.kind {
                        DiffLineKind::Added => ("+", tokens.added),
                        DiffLineKind::Removed => ("-", tokens.removed),
                        DiffLineKind::Context => (" ", tokens.text_secondary),
                    };
                    body = body.push(
                        text(format!("{mark}{}", line.text))
                            .font(reading.mono)
                            .size(reading.size)
                            .style(move |_| text::Style { color: Some(color) }),
                    );
                }
                let busy = app.state.loading;
                body = body.push(
                    row![
                        command_button(
                            "Stage hunk",
                            Message::StageHunk {
                                path: path.clone(),
                                index,
                            },
                            !busy,
                            tokens,
                            reading,
                        ),
                        command_button(
                            "Discard hunk",
                            Message::DiscardHunk { path, index },
                            !busy,
                            tokens,
                            reading,
                        ),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                );
            }
        }
    }

    container(scrollable(body))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn note<'a>(value: &str, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    text(value.to_string())
        .font(reading.font)
        .size(reading.size)
        .style(move |_| text::Style {
            color: Some(tokens.text_muted),
        })
        .into()
}
