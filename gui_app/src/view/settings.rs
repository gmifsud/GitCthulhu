use crate::app::{GitApp, Message};
use crate::appearance::UI_FONTS;
use crate::theme::DesignTokens;
use core_domain::{AppSettings, WorktreeTabRetention};

/// Wraps [`WorktreeTabRetention`] so the Settings pick list can show friendly labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RetentionChoice(WorktreeTabRetention);

const RETENTION_CHOICES: [RetentionChoice; 3] = [
    RetentionChoice(WorktreeTabRetention::Full),
    RetentionChoice(WorktreeTabRetention::Partial),
    RetentionChoice(WorktreeTabRetention::PathOnly),
];

impl std::fmt::Display for RetentionChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self.0 {
            WorktreeTabRetention::Full => "Everything",
            WorktreeTabRetention::Partial => "Message and marks",
            WorktreeTabRetention::PathOnly => "Path only",
        })
    }
}
/// Wraps [`core_domain::HeaderMark`] so the pick list shows readable names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MarkChoice(core_domain::HeaderMark);

const MARK_CHOICES: [MarkChoice; 5] = [
    MarkChoice(core_domain::HeaderMark::ALL[0]),
    MarkChoice(core_domain::HeaderMark::ALL[1]),
    MarkChoice(core_domain::HeaderMark::ALL[2]),
    MarkChoice(core_domain::HeaderMark::ALL[3]),
    MarkChoice(core_domain::HeaderMark::ALL[4]),
];

impl std::fmt::Display for MarkChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(super::wordmark::mark_label(self.0))
    }
}

use iced::widget::{column, container, pick_list, row, slider, text, text_input};
use iced::{Alignment, Background, Border, Element, Length, Shadow, Theme};

use super::chrome::{accent_button, command_button, field_style, reading};

pub(super) fn settings_dialog<'a>(app: &'a GitApp, tokens: DesignTokens) -> Element<'a, Message> {
    let settings = &app.settings;
    let reading = reading(settings);
    let selected = UI_FONTS
        .iter()
        .copied()
        .find(|font| *font == settings.ui_font);
    let size = settings
        .ui_size
        .clamp(AppSettings::MIN_UI_SIZE, AppSettings::MAX_UI_SIZE);
    let card = column![
        text("Settings")
            .font(reading.font)
            .size(reading.size + 4.0)
            .style(move |_| text::Style {
                color: Some(tokens.text_primary),
            }),
        text("Appearance")
            .font(reading.font)
            .size(reading.size)
            .style(move |_| text::Style {
                color: Some(tokens.accent_primary),
            }),
        text("Font")
            .font(reading.font)
            .size(reading.size)
            .style(move |_| text::Style {
                color: Some(tokens.text_secondary),
            }),
        pick_list(UI_FONTS, selected, |font| Message::UiFontChosen(
            font.to_string()
        ))
        .width(Length::Fill),
        row![
            text("Size")
                .font(reading.font)
                .size(reading.size)
                .style(move |_| text::Style {
                    color: Some(tokens.text_secondary),
                }),
            slider(
                AppSettings::MIN_UI_SIZE..=AppSettings::MAX_UI_SIZE,
                size,
                Message::UiSizeChosen
            ),
            text(size.to_string())
                .font(reading.mono)
                .size(reading.size)
                .style(move |_| text::Style {
                    color: Some(tokens.text_primary),
                }),
        ]
        .spacing(12)
        .align_y(Alignment::Center),
        text("Header mark")
            .font(reading.font)
            .size(reading.size)
            .style(move |_| text::Style {
                color: Some(tokens.text_secondary),
            }),
        pick_list(
            MARK_CHOICES,
            Some(MarkChoice(settings.header_mark)),
            |choice| Message::HeaderMarkChosen(choice.0),
        )
        .width(Length::Fill),
        text("Agents and Models")
            .font(reading.font)
            .size(reading.size)
            .style(move |_| text::Style {
                color: Some(tokens.accent_primary),
            }),
        text("Kilo API key")
            .font(reading.font)
            .size(reading.size)
            .style(move |_| text::Style {
                color: Some(tokens.text_secondary),
            }),
        text("From kilo.ai → API keys. Used by Suggest commit message.")
            .font(reading.font)
            .size((reading.size - 2.0).max(11.0))
            .style(move |_| text::Style {
                color: Some(tokens.text_muted),
            }),
        text_input("Paste your Kilo API key", &settings.agent_token)
            .secure(true)
            .on_input(Message::AgentTokenChanged)
            .padding(6)
            .width(Length::Fill)
            .style(field_style(tokens)),
        text("Model")
            .font(reading.font)
            .size(reading.size)
            .style(move |_| text::Style {
                color: Some(tokens.text_secondary),
            }),
        pick_list(
            model_choices(app),
            model_choices(app)
                .into_iter()
                .find(|model| model == &settings.agent_model),
            Message::AgentModelChosen,
        )
        .width(Length::Fill),
        command_button(
            "Load models",
            Message::RefreshAgentModels,
            !app.agent_busy,
            tokens,
            reading,
        ),
        text("Worktree Tabs")
            .font(reading.font)
            .size(reading.size)
            .style(move |_| text::Style {
                color: Some(tokens.accent_primary),
            }),
        text("Keep when leaving a tab")
            .font(reading.font)
            .size(reading.size)
            .style(move |_| text::Style {
                color: Some(tokens.text_secondary),
            }),
        pick_list(
            RETENTION_CHOICES,
            Some(RetentionChoice(settings.worktree_tab_retention)),
            |choice| Message::WorktreeRetentionChosen(choice.0),
        )
        .width(Length::Fill),
        accent_button("Close", Message::CloseSettings, true, tokens, reading),
    ]
    .spacing(12)
    .padding(20)
    .width(Length::Fixed(420.0));

    let mut card = card;
    if let Some(error) = &app.agent_models_error {
        card = card.push(
            text(error.clone())
                .font(reading.font)
                .size(reading.size)
                .style(move |_| text::Style {
                    color: Some(tokens.accent_primary),
                }),
        );
    }

    container(
        container(card).style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(tokens.bg_sidebar)),
            text_color: Some(tokens.text_primary),
            border: Border {
                color: tokens.border,
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
        background: Some(Background::Color(tokens.bg_app)),
        text_color: Some(tokens.text_primary),
        border: Border::default(),
        shadow: Shadow::default(),
    })
    .into()
}

fn model_choices(app: &GitApp) -> Vec<String> {
    if !app.agent_models.is_empty() {
        return app.agent_models.clone();
    }
    let saved = app.settings.agent_model.trim();
    vec![if saved.is_empty() {
        AppSettings::DEFAULT_AGENT_MODEL.to_string()
    } else {
        saved.to_string()
    }]
}
