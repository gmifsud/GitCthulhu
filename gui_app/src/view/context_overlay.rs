//! Floating context menu for commits and local branches.

use crate::app::{GitApp, Message};
use crate::context_menu::context_actions;
use crate::reachability::session_head_tip;
use crate::theme::DesignTokens;
use iced::widget::{button, column, container, mouse_area, text};
use iced::{Background, Border, Element, Length, Padding, Shadow, Theme};

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

    let panel = container(list)
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
        });

    // Pin the panel to the pointer captured when the menu opened.
    let left = menu.x.max(0.0);
    let top = menu.y.max(0.0);

    // Backdrop click (or right-click) dismisses; menu buttons capture first so
    // choosing an action is unaffected.
    Some(
        mouse_area(
            container(panel)
                .width(Length::Fill)
                .height(Length::Fill)
                .padding(Padding {
                    top,
                    right: 0.0,
                    bottom: 0.0,
                    left,
                })
                .style(move |_theme: &Theme| container::Style {
                    background: Some(Background::Color(iced::Color {
                        a: 0.20,
                        ..tokens.bg_app
                    })),
                    text_color: None,
                    border: Border::default(),
                    shadow: Shadow::default(),
                }),
        )
        .on_press(Message::CloseContextMenu)
        .on_right_press(Message::CloseContextMenu)
        .into(),
    )
}
