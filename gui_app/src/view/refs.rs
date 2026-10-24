use crate::app::Message;
use crate::selection::Selection;
use crate::theme::DesignTokens;
use core_domain::{Branch, Stash, Tag};
use iced::widget::{column, container, scrollable, text, text_input};
use iced::{Element, Length};

use super::chrome::{accent_button, row_button, section_label, Reading};

pub(super) fn branch_switcher<'a>(
    app: &'a crate::app::GitApp,
    selection: &Selection,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let name = current_branch_name(&app.state.branches);
    let mut switcher = column![accent_button(
        &name,
        Message::ToggleBranchList,
        !app.state.loading,
        tokens,
        reading,
    )]
    .spacing(8)
    .padding([8, 12]);

    if app.branch_list_open {
        let query = app.branch_query.as_str();
        switcher = switcher.push(
            text_input("Switch branch…", &app.branch_query)
                .on_input(Message::BranchQueryChanged)
                .padding(8)
                .width(Length::Fill),
        );
        switcher = switcher.push(
            container(scrollable(branch_list(
                &app.state.branches,
                &app.state.stashes,
                &app.state.tags,
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
            local = local.push(row_button(
                branch.name.clone(),
                Message::ChooseBranch(branch.name.clone()),
                branch.is_head
                    || matches!(selection, Selection::Branch { name, .. } if name == &branch.name),
                branch.is_head,
                tokens,
                reading,
                false,
            ));
        }
        list = list.push(local);
    }
    if !remotes.is_empty() {
        let mut remote = column![section_label(tokens, reading, "Remotes")].spacing(2);
        for branch in remotes {
            remote = remote.push(row_button(
                branch.name.clone(),
                Message::Select(Selection::Remote {
                    name: branch.name.clone(),
                }),
                matches!(selection, Selection::Remote { name } if name == &branch.name),
                false,
                tokens,
                reading,
                false,
            ));
        }
        list = list.push(remote);
    }
    if !shown_tags.is_empty() {
        let mut tagged = column![section_label(tokens, reading, "Tags")].spacing(2);
        for tag in shown_tags {
            tagged = tagged.push(row_button(
                tag.name.clone(),
                Message::Select(Selection::Tag {
                    name: tag.name.clone(),
                    commit_id: tag.commit_id.clone(),
                }),
                matches!(selection, Selection::Tag { name, .. } if name == &tag.name),
                false,
                tokens,
                reading,
                false,
            ));
        }
        list = list.push(tagged);
    }
    if !shown_stashes.is_empty() {
        let mut saved = column![section_label(tokens, reading, "Stashes")].spacing(2);
        for stash in shown_stashes {
            saved = saved.push(row_button(
                stash.message.clone(),
                Message::Select(Selection::Stash {
                    index: stash.index,
                    message: stash.message.clone(),
                }),
                matches!(
                    selection,
                    Selection::Stash { index, message }
                        if *index == stash.index && message == &stash.message
                ),
                false,
                tokens,
                reading,
                false,
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
