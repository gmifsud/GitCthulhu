//! Turns a [`RibbonCommand`] into the [`Message`] the window already handles.
//!
//! [`crate::selection::ribbon_commands`] decides which commands a selection
//! allows. This module only places them — as compact icon buttons.

use crate::app::{GitApp, Message};
use crate::changes::marked_paths;
use crate::jobs::WorktreeEdit;
use crate::selection::{ribbon_commands, RibbonCommand, RibbonItem, Selection};
use crate::theme::DesignTokens;
use core_domain::FileDisposition;
use iced::Element;

use super::chrome::{icon_button, Reading};

/// Where a command is drawn. The rules stay in `ribbon_commands`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CommandPlace {
    TopBar,
    Graph,
    Inspector,
}

pub(super) fn command_place(command: RibbonCommand) -> CommandPlace {
    match command {
        RibbonCommand::Fetch | RibbonCommand::Pull | RibbonCommand::Push | RibbonCommand::Stash => {
            CommandPlace::TopBar
        }
        RibbonCommand::Checkout
        | RibbonCommand::Refresh
        | RibbonCommand::CreateBranch
        | RibbonCommand::RenameBranch
        | RibbonCommand::DeleteBranch
        | RibbonCommand::Merge
        | RibbonCommand::Rebase => CommandPlace::Graph,
        RibbonCommand::Stage
        | RibbonCommand::Unstage
        | RibbonCommand::Discard
        | RibbonCommand::Commit
        | RibbonCommand::CherryPick
        | RibbonCommand::Reset
        | RibbonCommand::Apply
        | RibbonCommand::Pop
        | RibbonCommand::Drop => CommandPlace::Inspector,
    }
}

pub(super) fn can_commit(app: &GitApp) -> bool {
    !app.session().commit_message.trim().is_empty()
        && app
            .session()
            .state
            .current_status
            .as_ref()
            .is_some_and(|status| !status.staged_files.is_empty())
}

pub(super) fn can_stash(app: &GitApp) -> bool {
    app.session()
        .state
        .current_status
        .as_ref()
        .is_some_and(|status| !status.staged_files.is_empty() || !status.unstaged_files.is_empty())
}

pub(super) fn placed_commands(
    selection: &Selection,
    can_commit: bool,
    can_stash: bool,
    place: CommandPlace,
) -> Vec<RibbonItem> {
    ribbon_commands(selection, can_commit, can_stash)
        .into_iter()
        .filter(|item| command_place(item.command) == place)
        .collect()
}

/// Segoe UI Symbol marks for ribbon commands (recognisable at small size).
fn command_mark(command: RibbonCommand) -> &'static str {
    match command {
        RibbonCommand::Checkout => "⤵",
        RibbonCommand::Fetch => "↓",
        RibbonCommand::Pull => "⬇",
        RibbonCommand::Push => "⬆",
        RibbonCommand::Refresh => "↻",
        RibbonCommand::Stage => "+",
        RibbonCommand::Unstage => "−",
        RibbonCommand::Discard => "⌫",
        RibbonCommand::Commit => "✓",
        RibbonCommand::Stash => "≡",
        RibbonCommand::Apply => "⤵",
        RibbonCommand::Pop => "↥",
        RibbonCommand::Drop => "×",
        RibbonCommand::CreateBranch => "+",
        RibbonCommand::RenameBranch => "✎",
        RibbonCommand::DeleteBranch => "×",
        RibbonCommand::Merge => "⑂",
        RibbonCommand::Rebase => "⤴",
        RibbonCommand::CherryPick => "✂",
        RibbonCommand::Reset => "⟲",
    }
}

pub(super) fn command_buttons(
    items: &[RibbonItem],
    selection: &Selection,
    app: &GitApp,
    tokens: DesignTokens,
    reading: Reading,
) -> Vec<Element<'static, Message>> {
    items
        .iter()
        .map(|item| {
            let enabled = item.enabled && !app.session().state.loading;
            let message = command_message(item.command, selection, app);
            let tip_key = format!("cmd-{:?}", item.command);
            let tip_ready = app.tip_ready(&tip_key);
            icon_button(
                command_mark(item.command),
                item.label,
                tip_key,
                tip_ready,
                message,
                enabled,
                item.primary,
                tokens,
                reading,
            )
        })
        .collect()
}

fn command_message(command: RibbonCommand, selection: &Selection, app: &GitApp) -> Message {
    match command {
        RibbonCommand::Checkout => match selection {
            Selection::Branch {
                name,
                current: false,
            } => Message::CheckoutBranch(name.clone()),
            _ => Message::RefreshRepository,
        },
        RibbonCommand::Fetch => Message::Fetch,
        RibbonCommand::Pull => Message::Pull,
        RibbonCommand::Push => Message::Push,
        RibbonCommand::Refresh => Message::RefreshRepository,
        RibbonCommand::Stage => stage_message(app, selection, true),
        RibbonCommand::Unstage => stage_message(app, selection, false),
        RibbonCommand::Discard => Message::AskDiscard,
        RibbonCommand::Commit => {
            Message::EditWorktree(WorktreeEdit::Commit(app.session().commit_message.clone()))
        }
        RibbonCommand::Stash => Message::StashPush,
        RibbonCommand::Apply => match selection {
            Selection::Stash { index, .. } => Message::StashApply(*index),
            _ => Message::RefreshRepository,
        },
        RibbonCommand::Pop => match selection {
            Selection::Stash { index, .. } => Message::StashPop(*index),
            _ => Message::RefreshRepository,
        },
        RibbonCommand::Drop => Message::AskDropStash,
        RibbonCommand::CreateBranch => Message::AskCreateBranch,
        RibbonCommand::RenameBranch => Message::AskRenameBranch,
        RibbonCommand::DeleteBranch => Message::AskDeleteBranch,
        RibbonCommand::Merge => Message::AskMerge,
        RibbonCommand::Rebase => Message::AskRebase,
        RibbonCommand::CherryPick => Message::AskCherryPick,
        RibbonCommand::Reset => Message::AskReset,
    }
}

fn stage_message(app: &GitApp, selection: &Selection, stage: bool) -> Message {
    let marked = marked_paths(&app.session().marked_files, stage);
    if !marked.is_empty() {
        return if stage {
            Message::EditWorktree(WorktreeEdit::Stage(marked))
        } else {
            Message::EditWorktree(WorktreeEdit::Unstage(marked))
        };
    }
    match selection {
        Selection::File { path, place } if stage != (*place == FileDisposition::Staged) => {
            let paths = vec![path.clone()];
            if stage {
                Message::EditWorktree(WorktreeEdit::Stage(paths))
            } else {
                Message::EditWorktree(WorktreeEdit::Unstage(paths))
            }
        }
        _ => Message::RefreshRepository,
    }
}
