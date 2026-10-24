mod branch;
mod changes;
mod chrome;
mod commands;
mod confirm;
mod history;
mod patch;
mod refs;
mod settings;
mod status_bar;
mod terminal;
mod top_bar;
mod welcome;
mod workspace;

use crate::app::{GitApp, Message};
use crate::selection::visible_selection;
use iced::widget::{column, container};
use iced::{Background, Border, Element, Length, Shadow, Theme};

use branch::branch_prompt;
use chrome::{hairline, reading};
use confirm::{confirm_discard, confirm_drop_stash, history_prompt};
use settings::settings_dialog;
use status_bar::status_bar;
use terminal::terminal_pane;
use top_bar::top_bar;
use welcome::welcome;
use workspace::workspace;

impl GitApp {
    pub(crate) fn view(&self) -> Element<'_, Message> {
        let tokens = self.tokens;
        let reading = reading(&self.state.settings);

        if self.adapter.is_none() {
            return welcome(self, tokens, reading);
        }
        if let Some((path, _)) = &self.confirm_discard {
            return confirm_discard(path, tokens, reading);
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
        if self.settings_open {
            return settings_dialog(self, tokens);
        }

        let selection = visible_selection(
            self.selection.as_ref(),
            &self.state.branches,
            &self.state.commits,
            &self.state.stashes,
            &self.state.tags,
            self.state.current_status.as_ref(),
        );
        container(column![
            top_bar(self, tokens, reading),
            hairline(tokens),
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
        })
        .into()
    }
}
