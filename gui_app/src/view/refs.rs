use crate::app::Message;
use crate::context_menu::ContextTarget;
use crate::selection::Selection;
use crate::theme::DesignTokens;
use core_domain::{Branch, Stash, Tag};
use iced::widget::button::Status;
use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, mouse_area, row, scrollable, text, text_input};
use iced::{Alignment, Background, Border, Color, Element, Length, Shadow};

use super::chrome::{
    branch_chip, field_style, glyph, section_label, Reading, BRANCH_MARK, REMOTE_MARK, STASH_MARK,
    TAG_MARK,
};

pub(super) fn branch_switcher<'a>(
    app: &'a crate::app::GitApp,
    selection: &Selection,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let name = current_branch_name(&app.session().state.branches);
    let mut switcher = column![branch_chip(
        name,
        Message::ToggleBranchList,
        !app.session().state.loading,
        app.branch_list_open,
        tokens,
        reading,
    )]
    .spacing(8)
    .padding([8, 12]);

    if app.branch_list_open {
        let query = app.branch_query.as_str();
        switcher = switcher.push(
            text_input("Checkout branch…", &app.branch_query)
                .on_input(Message::BranchQueryChanged)
                .padding(6)
                .width(Length::Fill)
                .style(field_style(tokens)),
        );
        switcher = switcher.push(
            container(scrollable(branch_list(
                &app.session().state.branches,
                &app.session().state.stashes,
                &app.session().state.tags,
                selection,
                query,
                tokens,
                reading,
            )))
            .height(Length::Fixed(220.0))
            .width(Length::Fill),
        );
    }

    switcher.into()
}

fn branch_list<'a>(
    branches: &'a [Branch],
    stashes: &'a [Stash],
    tags: &'a [Tag],
    selection: &Selection,
    query: &str,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let mut list = column![].spacing(12).padding([4, 8]);
    let locals: Vec<&Branch> = branches
        .iter()
        .filter(|branch| !branch.is_remote && matches_query(&branch.name, query))
        .collect();
    let remotes: Vec<&Branch> = branches
        .iter()
        .filter(|branch| branch.is_remote && matches_query(&branch.name, query))
        .collect();
    let shown_tags: Vec<&Tag> = tags
        .iter()
        .filter(|tag| matches_query(&tag.name, query))
        .collect();
    let shown_stashes: Vec<&Stash> = stashes
        .iter()
        .filter(|stash| matches_query(&stash.message, query))
        .collect();
    let any = !locals.is_empty()
        || !remotes.is_empty()
        || !shown_tags.is_empty()
        || !shown_stashes.is_empty();
    if !locals.is_empty() {
        let mut local = column![section_label(tokens, reading, "Branches")].spacing(2);
        for branch in locals {
            let selected = branch.is_head
                || matches!(selection, Selection::Branch { name, .. } if name == &branch.name);
            local = local.push(ref_row(
                BRANCH_MARK,
                branch.name.clone(),
                Message::ChooseBranch(branch.name.clone()),
                selected,
                branch.is_head,
                Some(ContextTarget::LocalBranch {
                    name: branch.name.clone(),
                    tip: branch.tip.clone(),
                }),
                tokens,
                reading,
            ));
        }
        list = list.push(local);
    }
    if !remotes.is_empty() {
        let mut remote = column![section_label(tokens, reading, "Remotes")].spacing(2);
        for branch in remotes {
            let selected = matches!(selection, Selection::Remote { name } if name == &branch.name);
            remote = remote.push(ref_row(
                REMOTE_MARK,
                branch.name.clone(),
                Message::Select(Selection::Remote {
                    name: branch.name.clone(),
                }),
                selected,
                false,
                None,
                tokens,
                reading,
            ));
        }
        list = list.push(remote);
    }
    if !shown_tags.is_empty() {
        let mut tagged = column![section_label(tokens, reading, "Tags")].spacing(2);
        for tag in shown_tags {
            let selected = matches!(selection, Selection::Tag { name, .. } if name == &tag.name);
            tagged = tagged.push(ref_row(
                TAG_MARK,
                tag.name.clone(),
                Message::Select(Selection::Tag {
                    name: tag.name.clone(),
                    commit_id: tag.commit_id.clone(),
                }),
                selected,
                false,
                None,
                tokens,
                reading,
            ));
        }
        list = list.push(tagged);
    }
    if !shown_stashes.is_empty() {
        let mut saved = column![section_label(tokens, reading, "Stashes")].spacing(2);
        for stash in shown_stashes {
            let selected = matches!(
                selection,
                Selection::Stash { index, message }
                    if *index == stash.index && message == &stash.message
            );
            saved = saved.push(ref_row(
                STASH_MARK,
                stash.message.clone(),
                Message::Select(Selection::Stash {
                    index: stash.index,
                    message: stash.message.clone(),
                }),
                selected,
                false,
                None,
                tokens,
                reading,
            ));
        }
        list = list.push(saved);
    }
    if !any {
        list = list.push(
            text("No matching branch")
                .font(reading.font)
                .size(reading.size)
                .style(move |_| text::Style {
                    color: Some(tokens.text_muted),
                }),
        );
    }
    list.into()
}

#[allow(clippy::too_many_arguments)]
fn ref_row<'a>(
    mark: &'static str,
    label: String,
    message: Message,
    selected: bool,
    accent: bool,
    context: Option<ContextTarget>,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let mark_color = if accent {
        tokens.accent_primary
    } else if selected {
        tokens.text_primary
    } else {
        tokens.text_muted
    };
    let label_color = if accent {
        tokens.accent_primary
    } else if selected {
        tokens.text_primary
    } else {
        tokens.text_secondary
    };
    let row = button(
        row![
            glyph(mark, reading.size, mark_color, &reading),
            text(label)
                .font(reading.font)
                .size(reading.size)
                .wrapping(Wrapping::None)
                .style(move |_| text::Style {
                    color: Some(label_color),
                }),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding([4, 8])
    .on_press(message)
    .style(move |_theme, status| {
        let hot = matches!(status, Status::Hovered | Status::Pressed);
        iced::widget::button::Style {
            background: Some(Background::Color(if selected || hot {
                tokens.border
            } else {
                Color::TRANSPARENT
            })),
            text_color: label_color,
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 4.0.into(),
            },
            shadow: Shadow::default(),
        }
    });
    if let Some(target) = context {
        mouse_area(row)
            .on_right_press(Message::OpenContextMenu { target })
            .into()
    } else {
        row.into()
    }
}

pub(crate) fn matches_query(name: &str, query: &str) -> bool {
    let query = query.trim();
    query.is_empty() || name.to_lowercase().contains(&query.to_lowercase())
}

pub(super) fn current_branch_name(branches: &[Branch]) -> String {
    branches
        .iter()
        .find(|branch| branch.is_head && !branch.is_remote)
        .map(|branch| branch.name.clone())
        .unwrap_or_else(|| "Detached HEAD".to_string())
}
