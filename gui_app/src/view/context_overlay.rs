//! Floating context menu for commits and local branches.

use crate::app::{GitApp, Message};
use crate::context_menu::context_actions;
use crate::reachability::session_head_tip;
use crate::theme::DesignTokens;
use iced::widget::{button, column, container, text};
use iced::{Background, Border, Element, Length, Shadow, Theme};

use super::chrome::Reading;

pub(super) fn context_menu_overlay<'a>(
    app: &'a GitApp,
    tokens: DesignTokens,
    reading: Reading,
) -> Option<Element<'a, Message>> {
    let menu = app.context_menu.as_ref()?;
    let session = app.session();
    let head = session_head_tip(
        &session.state.branches,
        session
            .state
            .current_status
            .as_ref()
            .and_then(|status| status.head_commit_id.as_deref()),
    );
    let actions = context_actions(
        &menu.target,
        &app.worktree_catalogue,
        head.as_deref(),
        session.state.loading,
    );
    let mut list = column![].spacing(2).padding(6);
    for action in actions {
        let kind = action.kind.clone();
        let enabled = action.enabled;
        list = list.push(
            button(
                text(action.label)
                    .font(reading.font)
                    .size(reading.size)
                    .style(move |_| text::Style {
                        color: Some(if enabled {
                            tokens.text_primary
                        } else {
                            tokens.text_muted
                        }),
                    }),
            )
            .width(Length::Fill)
            .padding([6, 10])
            .on_press_maybe(enabled.then_some(Message::ContextMenuAction(kind)))
            .style(move |_theme, status| {
                let hot =
                    matches!(status, iced::widget::button::Status::Hovered) && enabled;
                iced::widget::button::Style {
                    background: Some(Background::Color(if hot {
                        tokens.bg_raised
                    } else {
                        tokens.bg_chrome
                    })),
                    text_color: if enabled {
                        tokens.text_primary
                    } else {
                        tokens.text_muted
                    },
                    border: Border::default(),
                    shadow: Shadow::default(),
                }
            }),
        );
    }
    list = list.push(
        button(
            text("Cancel")
                .font(reading.font)
                .size(reading.size)
                .style(move |_| text::Style {
                    color: Some(tokens.text_muted),
                }),
        )
        .width(Length::Fill)
        .padding([6, 10])
        .on_press(Message::CloseContextMenu)
        .style(move |_theme, _status| iced::widget::button::Style {
            background: Some(Background::Color(tokens.bg_chrome)),
            text_color: tokens.text_muted,
            border: Border::default(),
            shadow: Shadow::default(),
        }),
    );

    Some(
        container(
            container(list)
                .width(Length::Fixed(220.0))
                .style(move |_theme: &Theme| container::Style {
                    background: Some(Background::Color(tokens.bg_chrome)),
                    text_color: Some(tokens.text_primary),
                    border: Border {
                        color: tokens.chip_border,
                        width: 1.0,
                        radius: 6.0.into(),
                    },
                    shadow: Shadow::default(),
                }),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(iced::Color {
                a: 0.35,
                ..tokens.bg_app
            })),
            text_color: None,
            border: Border::default(),
            shadow: Shadow::default(),
        })
        .into(),
    )
}
