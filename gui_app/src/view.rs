use crate::app::{GitApp, Message};
use crate::changes::{change_sections, FileAction};
use crate::commit_graph::{graph_rows, graph_width, GraphCell, ROW_HEIGHT};
use crate::jobs::{WorktreeEdit, COMMIT_PAGE_SIZE};
use crate::theme::DesignTokens;
use crate::tracking::tracking_summary;
use core_domain::Branch;
use iced::widget::button::Status;
use iced::widget::text::Wrapping;
use iced::widget::{
    button, canvas, column, container, horizontal_space, row, scrollable, text, text_input,
};
use iced::{Alignment, Background, Border, Color, Element, Length, Shadow, Theme};

impl GitApp {
    pub(crate) fn view(&self) -> Element<'_, Message> {
        let tokens = self.tokens;

        if self.adapter.is_none() {
            return welcome(self, tokens);
        }

        let busy = self.state.loading;
        let branch_name = current_branch_name(&self.state.branches);
        let summary = tracking_summary(self.state.current_status.as_ref());

        let mut toolbar = column![row![
            text(branch_name).size(16).style(move |_| text::Style {
                color: Some(tokens.accent_primary),
            }),
            text(summary).size(13).style(move |_| text::Style {
                color: Some(tokens.text_secondary),
            }),
            horizontal_space(),
            command_button("Refresh", Message::RefreshRepository, !busy, tokens),
            command_button("Fetch", Message::Fetch, !busy, tokens),
            command_button("Pull", Message::Pull, !busy, tokens),
            command_button("Push", Message::Push, !busy, tokens),
        ]
        .spacing(12)
        .align_y(Alignment::Center)]
        .spacing(8)
        .padding([12, 16]);

        if let Some(error) = &self.error {
            toolbar = toolbar.push(text(error.clone()).size(13).wrapping(Wrapping::Word).style(
                move |_| text::Style {
                    color: Some(tokens.accent_primary),
                },
            ));
        }

        let toolbar = container(toolbar)
            .width(Length::Fill)
            .style(move |_theme: &Theme| container::Style {
                background: Some(Background::Color(tokens.bg_sidebar)),
                text_color: Some(tokens.text_primary),
                border: Border::default(),
                shadow: Shadow::default(),
            });

        container(column![
            toolbar,
            hairline(tokens),
            row![
                branch_sidebar(&self.state.branches, tokens),
                history_pane(self, tokens),
                changes_pane(self, tokens),
            ]
            .width(Length::Fill)
            .height(Length::Fill),
        ])
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(tokens.bg_app)),
            text_color: Some(tokens.text_secondary),
            border: Border::default(),
            shadow: Shadow::default(),
        })
        .into()
    }
}

fn welcome<'a>(app: &'a GitApp, tokens: DesignTokens) -> Element<'a, Message> {
    let mut welcome = column![
        text("GitCthulhu").size(32).style(move |_| text::Style {
            color: Some(tokens.accent_primary)
        }),
        text("Open a repository to get started.")
            .size(16)
            .style(move |_| text::Style {
                color: Some(tokens.text_secondary)
            }),
        button("Open Repository…").on_press(Message::ChooseRepositoryFolder),
    ]
    .spacing(20)
    .padding(40);
    if let Some(error) = &app.error {
        welcome = welcome.push(text(error.clone()).style(move |_| text::Style {
            color: Some(tokens.accent_primary),
        }));
    }

    container(welcome)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(tokens.bg_app)),
            text_color: Some(tokens.text_primary),
            border: Border::default(),
            shadow: Shadow::default(),
        })
        .into()
}

fn current_branch_name(branches: &[Branch]) -> String {
    branches
        .iter()
        .find(|branch| branch.is_head && !branch.is_remote)
        .map(|branch| branch.name.clone())
        .unwrap_or_else(|| "Detached HEAD".to_string())
}

fn branch_sidebar<'a>(branches: &'a [Branch], tokens: DesignTokens) -> Element<'a, Message> {
    let mut local = column![section_label(tokens, "LOCAL")].spacing(2);
    let mut remote = column![section_label(tokens, "REMOTE")].spacing(2);
    let mut local_count = 0usize;
    let mut remote_count = 0usize;

    for branch in branches {
        if branch.is_remote {
            remote_count += 1;
            remote = remote.push(
                text(branch.name.as_str())
                    .size(13)
                    .wrapping(Wrapping::None)
                    .style(move |_| text::Style {
                        color: Some(tokens.text_muted),
                    }),
            );
        } else if branch.is_head {
            local_count += 1;
            local = local.push(
                text(branch.name.as_str())
                    .size(14)
                    .wrapping(Wrapping::None)
                    .style(move |_| text::Style {
                        color: Some(tokens.accent_primary),
                    }),
            );
        } else {
            local_count += 1;
            local = local.push(branch_button(
                &branch.name,
                Message::CheckoutBranch(branch.name.clone()),
                tokens,
            ));
        }
    }

    let mut sidebar = column![].spacing(16).padding(12);
    if local_count == 0 && remote_count == 0 {
        sidebar = sidebar.push(text("No branches").style(move |_| text::Style {
            color: Some(tokens.text_muted),
        }));
    } else {
        if local_count > 0 {
            sidebar = sidebar.push(local);
        }
        if remote_count > 0 {
            sidebar = sidebar.push(remote);
        }
    }

    container(scrollable(sidebar))
        .width(Length::Fixed(220.0))
        .height(Length::Fill)
        .style(move |_theme: &Theme| panel_style(tokens, true))
        .into()
}

fn history_pane<'a>(app: &'a GitApp, tokens: DesignTokens) -> Element<'a, Message> {
    let single_line = |value: &str, width: Length| {
        container(text(value.to_string()).size(13).wrapping(Wrapping::None))
            .width(width)
            .clip(true)
    };
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
        commit_rows = commit_rows.push(
            row![
                canvas(GraphCell(graph_row))
                    .width(lanes_width)
                    .height(Length::Fixed(ROW_HEIGHT)),
                single_line(&commit.id[..7], Length::Fixed(72.0)),
                single_line(&commit.message, Length::Fill),
                single_line(&commit.author, Length::Fixed(140.0)),
            ]
            .spacing(12)
            .height(Length::Fixed(ROW_HEIGHT))
            .align_y(Alignment::Center),
        );
    }

    if shown_nodes.is_empty() {
        commit_rows = commit_rows.push(text("No commits yet").style(move |_| text::Style {
            color: Some(tokens.text_muted),
        }));
    }

    container(scrollable(commit_rows.padding([8, 16])))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_theme: &Theme| panel_style(tokens, false))
        .into()
}

fn changes_pane<'a>(app: &'a GitApp, tokens: DesignTokens) -> Element<'a, Message> {
    let can_commit = !app.commit_message.trim().is_empty()
        && app
            .state
            .current_status
            .as_ref()
            .is_some_and(|status| !status.staged_files.is_empty());
    let mut changes = column![
        section_label(tokens, "CHANGES"),
        text_input("Commit message", &app.commit_message)
            .on_input(Message::CommitMessageChanged)
            .padding(8)
            .width(Length::Fill),
        accent_button(
            "Commit",
            Message::EditWorktree(WorktreeEdit::Commit(app.commit_message.clone())),
            can_commit,
            tokens,
        ),
    ]
    .spacing(8)
    .padding(12);

    match &app.state.current_status {
        None => {
            changes = changes.push(text("Not loaded yet").style(move |_| text::Style {
                color: Some(tokens.text_muted),
            }))
        }
        Some(status) => {
            let stageable: Vec<String> = status
                .unstaged_files
                .iter()
                .chain(status.untracked_files.iter())
                .cloned()
                .collect();
            if !stageable.is_empty() {
                changes = changes.push(command_button(
                    "Stage all",
                    Message::EditWorktree(WorktreeEdit::Stage(stageable)),
                    true,
                    tokens,
                ));
            }
            if !status.staged_files.is_empty() {
                changes = changes.push(command_button(
                    "Unstage all",
                    Message::EditWorktree(WorktreeEdit::Unstage(status.staged_files.clone())),
                    true,
                    tokens,
                ));
            }
            let sections = change_sections(status);
            if sections.is_empty() {
                changes = changes.push(text("Working tree clean").style(move |_| text::Style {
                    color: Some(tokens.text_muted),
                }));
            }
            for section in sections {
                let action_label = match section.action {
                    FileAction::Stage => "Stage",
                    FileAction::Unstage => "Unstage",
                };
                let mut files =
                    column![text(format!("{} ({})", section.title, section.files.len()))
                        .size(12)
                        .style(move |_| text::Style {
                            color: Some(tokens.text_muted),
                        })]
                    .spacing(4);
                for file in section.files {
                    let edit = match section.action {
                        FileAction::Stage => WorktreeEdit::Stage(vec![file.clone()]),
                        FileAction::Unstage => WorktreeEdit::Unstage(vec![file.clone()]),
                    };
                    files = files.push(
                        row![
                            text(file.as_str())
                                .size(13)
                                .wrapping(Wrapping::None)
                                .width(Length::Fill)
                                .style(move |_| text::Style {
                                    color: Some(tokens.text_secondary)
                                }),
                            link_button(action_label, Message::EditWorktree(edit), tokens),
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center),
                    );
                }
                changes = changes.push(files);
            }
        }
    }

    container(scrollable(changes))
        .width(Length::Fixed(300.0))
        .height(Length::Fill)
        .style(move |_theme: &Theme| panel_style(tokens, true))
        .into()
}

fn section_label(tokens: DesignTokens, title: &'static str) -> Element<'static, Message> {
    text(title)
        .size(11)
        .style(move |_| text::Style {
            color: Some(tokens.text_muted),
        })
        .into()
}

fn panel_style(tokens: DesignTokens, ruled: bool) -> container::Style {
    container::Style {
        background: Some(Background::Color(if ruled {
            tokens.bg_sidebar
        } else {
            tokens.bg_app
        })),
        text_color: Some(tokens.text_secondary),
        border: Border {
            color: tokens.border,
            width: if ruled { 1.0 } else { 0.0 },
            radius: 0.0.into(),
        },
        shadow: Shadow::default(),
    }
}

fn hairline(tokens: DesignTokens) -> Element<'static, Message> {
    container(column![])
        .width(Length::Fill)
        .height(Length::Fixed(1.0))
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(tokens.border)),
            text_color: None,
            border: Border::default(),
            shadow: Shadow::default(),
        })
        .into()
}

fn link_button<'a>(label: &'a str, message: Message, tokens: DesignTokens) -> Element<'a, Message> {
    button(text(label).size(12))
        .padding([2, 6])
        .on_press(message)
        .style(move |_theme, status| {
            let hot = matches!(status, Status::Hovered | Status::Pressed);
            iced::widget::button::Style {
                background: Some(Background::Color(if hot {
                    tokens.border
                } else {
                    Color::TRANSPARENT
                })),
                text_color: tokens.accent_primary,
                border: Border {
                    color: Color::TRANSPARENT,
                    width: 0.0,
                    radius: 4.0.into(),
                },
                shadow: Shadow::default(),
            }
        })
        .into()
}

fn command_button<'a>(
    label: &'a str,
    message: Message,
    enabled: bool,
    tokens: DesignTokens,
) -> Element<'a, Message> {
    button(text(label).size(13))
        .padding([5, 10])
        .on_press_maybe(enabled.then_some(message))
        .style(move |_theme, status| bordered_button(tokens, status, enabled, false))
        .into()
}

fn accent_button<'a>(
    label: &'a str,
    message: Message,
    enabled: bool,
    tokens: DesignTokens,
) -> Element<'a, Message> {
    button(text(label).size(13))
        .width(Length::Fill)
        .padding([6, 10])
        .on_press_maybe(enabled.then_some(message))
        .style(move |_theme, status| bordered_button(tokens, status, enabled, true))
        .into()
}

fn branch_button<'a>(
    name: &'a str,
    message: Message,
    tokens: DesignTokens,
) -> Element<'a, Message> {
    button(text(name).size(13).wrapping(Wrapping::None))
        .width(Length::Fill)
        .padding([4, 8])
        .on_press(message)
        .style(move |_theme, status| {
            let hot = matches!(status, Status::Hovered | Status::Pressed);
            iced::widget::button::Style {
                background: Some(Background::Color(if hot {
                    tokens.border
                } else {
                    Color::TRANSPARENT
                })),
                text_color: tokens.text_secondary,
                border: Border {
                    color: Color::TRANSPARENT,
                    width: 0.0,
                    radius: 4.0.into(),
                },
                shadow: Shadow::default(),
            }
        })
        .into()
}

fn bordered_button(
    tokens: DesignTokens,
    status: Status,
    enabled: bool,
    accent: bool,
) -> iced::widget::button::Style {
    let hot = matches!(status, Status::Hovered | Status::Pressed);
    let background = if accent && enabled {
        tokens.accent_primary
    } else if hot {
        tokens.border
    } else {
        tokens.bg_app
    };
    iced::widget::button::Style {
        background: Some(Background::Color(background)),
        text_color: if enabled {
            tokens.text_primary
        } else {
            tokens.text_secondary
        },
        border: Border {
            color: if accent && enabled {
                tokens.accent_primary
            } else {
                tokens.border
            },
            width: 1.0,
            radius: 4.0.into(),
        },
        shadow: Shadow::default(),
    }
}
