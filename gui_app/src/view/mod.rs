mod branch;
mod changes;
mod chrome;
mod files_nav;
mod commands;
mod confirm;
mod context_overlay;
mod explorer;
mod history;
mod patch;
mod refs;
mod settings;
mod status_bar;
mod tabs;
mod terminal;
mod top_bar;
mod welcome;
mod workspace;
mod worktrees;

use crate::app::{GitApp, Message};
use crate::selection::visible_selection;
use iced::widget::{column, container, stack};
use iced::{Background, Border, Element, Length, Shadow, Theme};

use branch::branch_prompt;
use chrome::{compact, hairline, reading};
use confirm::{confirm_detach, confirm_discard, confirm_drop_stash, history_prompt};
use context_overlay::context_menu_overlay;
use settings::settings_dialog;
use status_bar::status_bar;
use tabs::tab_strip;
use terminal::terminal_pane;
use top_bar::top_bar;
use welcome::welcome;
use workspace::workspace;
use worktrees::worktree_prompt;

impl GitApp {
    pub(crate) fn view(&self) -> Element<'_, Message> {
        let tokens = self.tokens;
        let reading = compact(reading(&self.settings));

        if self.sessions.is_empty() {
            return welcome(self, tokens, reading);
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
                return confirm_detach(commit_id, tokens, reading);
            }
        }
        if let Some((paths, _)) = &self.confirm_discard {
            return confirm_discard(paths, tokens, reading);
        }
        if let Some(stash) = &self.confirm_drop {
            return confirm_drop_stash(&stash.message, tokens, reading);
        }
        if let Some(prompt) = &self.branch_prompt {
            return branch_prompt(prompt, tokens, reading);
        }
        if let Some(prompt) = &self.history_prompt {
            return history_prompt(prompt, tokens, reading);
        }
        if let Some(prompt) = &self.worktree_prompt {
            return worktree_prompt(
                prompt,
                self.error.as_deref(),
                self.worktree_busy,
                tokens,
                reading,
            );
        }
        if self.settings_open {
            return settings_dialog(self, tokens);
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
        let main = container(column![
            top_bar(self, tokens, reading),
            hairline(tokens),
            tab_strip(self, tokens, reading),
            workspace(self, &selection, tokens, reading),
            terminal_pane(self, tokens, reading),
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
        if let Some(overlay) = context_menu_overlay(self, tokens, reading) {
            stack![main, overlay].into()
        } else {
            main.into()
        }
    }
}
