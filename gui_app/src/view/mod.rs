mod branch;
mod activity;
mod branch_popup;
mod changes;
mod chrome;
mod compare;
mod repositories;
mod files_nav;
mod commands;
mod confirm;
mod context_overlay;
mod explorer;
mod filter_bar;
mod history;
mod operation;
mod palette;
mod patch;
mod refs;
mod settings;
mod status_bar;
mod tabs;
mod terminal;
mod top_bar;
mod welcome;
pub(crate) mod wordmark;
mod workspace;
mod worktrees;

use crate::app::{GitApp, Message};
use crate::selection::visible_selection;
use iced::widget::{column, container, opaque, row, stack, Space};
use iced::{Background, Border, Element, Length, Shadow, Theme};

use branch::branch_prompt;
use chrome::{compact, hairline, reading, Reading};
use crate::theme::DesignTokens;
use confirm::{confirm_detach, confirm_discard, confirm_drop_stash, history_prompt, stash_dialog};
use context_overlay::context_menu_overlay;
use palette::palette_overlay;
use settings::settings_dialog;
use status_bar::status_bar;
use tabs::tab_strip;
use terminal::terminal_pane;
use top_bar::top_bar;
use welcome::welcome;
use workspace::workspace;
use worktrees::worktree_prompt;

/// Width the Repositories column takes from the window, pinned or folded to its rail.
pub(crate) fn repositories_width(pinned: bool) -> f32 {
    if pinned {
        repositories::COLUMN_WIDTH
    } else {
        repositories::RAIL_WIDTH
    }
}

/// The checked-out branch's name, or "Detached HEAD".
pub(crate) fn current_branch(branches: &[core_domain::Branch]) -> String {
    refs::current_branch_name(branches)
}

impl GitApp {
    pub(crate) fn view(&self) -> Element<'_, Message> {
        let tokens = self.tokens;
        let reading = compact(reading(&self.settings));

        if self.sessions.is_empty() {
            return welcome(self, tokens, reading);
        }
        let session = self.session();
        let selection = visible_selection(
            session.selection.as_ref(),
            &session.state.branches,
            &session.state.commits,
            &session.state.stashes,
            &session.state.tags,
            session.state.current_status.as_ref(),
        );
        // The Repositories column runs the full height between the header and the status bar,
        // beside the tabs, the workspace and the terminal.
        let main = container(column![
            top_bar(self, tokens, reading),
            hairline(tokens),
            row![
                repositories::repositories_column(self, tokens, reading),
                column![
                    tab_strip(self, tokens, reading),
                    operation::operation_banner(self, tokens, reading),
                    workspace(self, &selection, tokens, reading),
                    terminal_pane(self, tokens, reading),
                ]
                .width(Length::Fill)
                .height(Length::Fill),
            ]
            .height(Length::Fill),
            status_bar(self, tokens, reading),
        ])
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(tokens.bg_app)),
            text_color: Some(tokens.text_secondary),
            border: Border::default(),
            shadow: Shadow::default(),
        });
        // The root is always `stack![main, layer]`. iced keeps widget state (scroll offsets,
        // text cursors) by position in the tree, so swapping the root for a dialog or wrapping
        // it in a stack only while a menu is open reset every scrollable to the top.
        let layer: Element<'_, Message> = if let Some(dialog) = self.dialog(tokens, reading) {
            // `opaque` keeps clicks and hovers from reaching the screen underneath.
            opaque(dialog)
        } else if let Some(overlay) = palette_overlay(self, tokens, reading) {
            overlay
        } else if let Some(overlay) =
            repositories::repositories_flyout(self, top_bar::HEADER_HEIGHT, tokens, reading)
        {
            overlay
        } else if let Some(overlay) = branch_popup::branch_popup(self, tokens, reading) {
            overlay
        } else if let Some(overlay) = context_menu_overlay(self, tokens, reading) {
            overlay
        } else {
            Space::new(Length::Shrink, Length::Shrink).into()
        };
        stack![main, layer].into()
    }

    /// The modal dialog that is waiting for the user, if any.
    fn dialog(&self, tokens: DesignTokens, reading: Reading) -> Option<Element<'_, Message>> {
        if let Some(state) = &self.compare {
            return Some(compare::compare_window(self, state, tokens, reading));
        }
        // Only show the detach dialog while we are waiting for the user (dirty worktree).
        // Clean detach sets confirm_detach then immediately runs the job and clears it.
        if let Some(commit_id) = &self.confirm_detach {
            if self
                .session()
                .state
                .current_status
                .as_ref()
                .is_some_and(|status| {
                    !status.staged_files.is_empty()
                        || !status.unstaged_files.is_empty()
                        || !status.untracked_files.is_empty()
                })
            {
                return Some(confirm_detach(commit_id, tokens, reading));
            }
        }
        if let Some(files) = &self.confirm_discard {
            let paths: Vec<String> = files.iter().map(|file| file.path.clone()).collect();
            return Some(confirm_discard(&paths, tokens, reading));
        }
        if let Some(stash) = &self.confirm_drop {
            return Some(confirm_drop_stash(&stash.message, tokens, reading));
        }
        if let Some(prompt) = &self.branch_prompt {
            return Some(branch_prompt(prompt, tokens, reading));
        }
        if let Some(prompt) = &self.history_prompt {
            let branches = &self.session().state.branches;
            let current = refs::current_branch_name(branches);
            let mergeable: Vec<String> = branches
                .iter()
                .filter(|branch| !(branch.is_head && !branch.is_remote))
                .map(|branch| branch.name.clone())
                .collect();
            return Some(history_prompt(prompt, &mergeable, &current, tokens, reading));
        }
        if let Some(prompt) = &self.stash_prompt {
            return Some(stash_dialog(prompt, tokens, reading));
        }
        if let Some(prompt) = &self.worktree_prompt {
            return Some(worktree_prompt(
                prompt,
                self.error.as_deref(),
                self.worktree_busy,
                tokens,
                reading,
            ));
        }
        if self.settings_open {
            return Some(settings_dialog(self, tokens));
        }
        None
    }
}
