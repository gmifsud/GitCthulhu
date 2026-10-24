use crate::app::{GitApp, Message};
use crate::commit_graph::{graph_rows, graph_width, GraphCell, ROW_HEIGHT};
use crate::jobs::COMMIT_PAGE_SIZE;
use crate::selection::{short_hash, Selection};
use crate::theme::DesignTokens;
use iced::widget::button::Status;
use iced::widget::text::Wrapping;
use iced::widget::{button, canvas, column, container, row, scrollable, text};
use iced::{Alignment, Background, Border, Color, Element, Length, Shadow, Theme};

use super::chrome::{panel_style, Reading};

pub(super) fn history_pane<'a>(
    app: &'a GitApp,
    selection: &Selection,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let shown_nodes = &app.state.dag_nodes[..app.state.dag_nodes.len().min(COMMIT_PAGE_SIZE)];
    let rows = graph_rows(shown_nodes);
    let lanes_width = Length::Fixed(graph_width(&rows));

    let mut commit_rows = column![];
    for (node, graph_row) in shown_nodes.iter().zip(rows) {
        let Some(commit) = app
            .state
            .commits
            .iter()
            .find(|commit| commit.id == node.commit_id)
        else {
            continue;
        };
        let selected = matches!(selection, Selection::Commit { id } if id == &commit.id);
        let label = row![
            canvas(GraphCell(graph_row))
                .width(lanes_width)
                .height(Length::Fixed(ROW_HEIGHT)),
            single_line(
                short_hash(&commit.id),
                Length::Fixed(72.0),
                reading,
                tokens,
                true
            ),
            single_line(&commit.message, Length::Fill, reading, tokens, false),
            single_line(&commit.author, Length::Fixed(140.0), reading, tokens, false),
        ]
        .spacing(12)
        .height(Length::Fixed(ROW_HEIGHT))
        .align_y(Alignment::Center);
        commit_rows = commit_rows.push(
            button(label)
                .width(Length::Fill)
                .padding(0)
                .on_press(Message::Select(Selection::Commit {
                    id: commit.id.clone(),
                }))
                .style(move |_theme, status| commit_style(tokens, status, selected)),
        );
    }

    if shown_nodes.is_empty() {
        commit_rows = commit_rows.push(
            text("No commits yet")
                .font(reading.font)
                .size(reading.size)
                .style(move |_| text::Style {
                    color: Some(tokens.text_muted),
                }),
        );
    }

    container(scrollable(commit_rows.padding([8, 16])))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_theme: &Theme| panel_style(tokens, false))
        .into()
}

fn single_line<'a>(
    value: &str,
    width: Length,
    reading: Reading,
    tokens: DesignTokens,
    mono: bool,
) -> Element<'a, Message> {
    let face = if mono { reading.mono } else { reading.font };
    container(
        text(value.to_string())
            .font(face)
            .size(reading.size)
            .wrapping(Wrapping::None)
            .style(move |_| text::Style {
                color: Some(if mono {
                    tokens.text_secondary
                } else {
                    tokens.text_primary
                }),
            }),
    )
    .width(width)
    .clip(true)
    .into()
}

fn commit_style(
    tokens: DesignTokens,
    status: Status,
    selected: bool,
) -> iced::widget::button::Style {
    let hot = matches!(status, Status::Hovered | Status::Pressed);
    iced::widget::button::Style {
        background: Some(Background::Color(if selected || hot {
            tokens.border
        } else {
            Color::TRANSPARENT
        })),
        text_color: tokens.text_primary,
        border: Border::default(),
        shadow: Shadow::default(),
    }
}
