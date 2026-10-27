use crate::app::{GitApp, Message};
use crate::theme::DesignTokens;
use core_domain::RepoStatus;
use iced::widget::button::Status;
use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, horizontal_space, row, text};
use iced::{Alignment, Background, Border, Color, Element, Length, Shadow, Theme};

use super::chrome::{fold_mark, glyph, Reading};

const STATUS_BAR_HEIGHT: f32 = 24.0;
const STATUS_BAR_EXPANDED: f32 = 72.0;
const STATUS_PADDING: f32 = 12.0;
const STATUS_GAP: f32 = 14.0;
const DOT_SIZE: f32 = 7.0;
const NO_UPSTREAM: &str = "No upstream";
const ENCODING: &str = "UTF-8";

pub(super) fn status_bar<'a>(
    app: &'a GitApp,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let quiet = (reading.size - 2.0).max(11.0);
    let expanded = app.error.is_some() && app.error_expanded;
    let height = if expanded {
        STATUS_BAR_EXPANDED
    } else {
        STATUS_BAR_HEIGHT
    };
    let busy = app.activity_busy();
    let mut bar = row![loading_dot(busy, tokens)]
        .spacing(STATUS_GAP)
        .align_y(if expanded {
            Alignment::Start
        } else {
            Alignment::Center
        })
        .width(Length::Fill)
        .height(Length::Fixed(height));

    if let Some(error) = &app.error {
        bar = bar.push(error_cluster(error, app.error_expanded, quiet, tokens, reading));
    } else {
        bar = bar.push(status_text(
            app.activity_label().to_string(),
            quiet,
            tokens,
            reading,
        ));
    }

    bar = bar.push(horizontal_space());
    bar = bar.push(status_text(
        commit_count_label(app.session().state.commits.len()),
        quiet,
        tokens,
        reading,
    ));
    bar = bar.push(status_text(
        upstream_name(app.session().state.current_status.as_ref()),
        quiet,
        tokens,
        reading,
    ));
    bar = bar.push(status_text(ENCODING.to_string(), quiet, tokens, reading));

    container(bar)
        .width(Length::Fill)
        .height(Length::Fixed(height))
        .padding([0.0, STATUS_PADDING])
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(tokens.bg_chrome)),
            text_color: Some(tokens.text_secondary),
            border: Border {
                color: tokens.border,
                width: 1.0,
                radius: 0.0.into(),
            },
            shadow: Shadow::default(),
        })
        .into()
}

fn error_cluster<'a>(
    error: &str,
    expanded: bool,
    quiet: f32,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let summary = error_summary(error);
    let body = if expanded {
        error.to_string()
    } else {
        summary
    };
    let wrapping = if expanded {
        Wrapping::Word
    } else {
        Wrapping::None
    };
    row![
        button(
            text(body)
                .font(reading.font)
                .size(quiet)
                .wrapping(wrapping)
                .style(move |_| text::Style {
                    color: Some(tokens.accent_primary),
                }),
        )
        .padding(0)
        .width(Length::Fill)
        .on_press(Message::ToggleErrorDetail)
        .style(move |_theme, status| {
            let hot = matches!(status, Status::Hovered | Status::Pressed);
            iced::widget::button::Style {
                background: Some(Background::Color(if hot {
                    tokens.border
                } else {
                    Color::TRANSPARENT
                })),
                text_color: tokens.accent_primary,
                border: Border::default(),
                shadow: Shadow::default(),
            }
        }),
        row![
            glyph(
                fold_mark(!expanded),
                quiet,
                tokens.text_muted,
                &reading,
            ),
            text("details")
                .font(reading.font)
                .size(quiet)
                .style(move |_| text::Style {
                    color: Some(tokens.text_muted),
                }),
        ]
        .spacing(4)
        .align_y(Alignment::Center),
        button(
            glyph("×", quiet, tokens.text_muted, &reading),
        )
        .padding([0, 4])
        .on_press(Message::ClearError)
        .style(move |_theme, status| {
            let hot = matches!(status, Status::Hovered | Status::Pressed);
            iced::widget::button::Style {
                background: Some(Background::Color(if hot {
                    tokens.border
                } else {
                    Color::TRANSPARENT
                })),
                text_color: tokens.text_muted,
                border: Border::default(),
                shadow: Shadow::default(),
            }
        }),
    ]
    .spacing(8)
    .align_y(Alignment::Start)
    .width(Length::Fill)
    .into()
}

/// First line of a multi-line git error, trimmed for the status strip.
pub(crate) fn error_summary(error: &str) -> String {
    error
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or(error)
        .trim()
        .to_string()
}

fn loading_dot(loading: bool, tokens: DesignTokens) -> Element<'static, Message> {
    let colour = if loading {
        tokens.accent_primary
    } else {
        tokens.added
    };
    container(column![])
        .width(Length::Fixed(DOT_SIZE))
        .height(Length::Fixed(DOT_SIZE))
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(colour)),
            text_color: None,
            border: Border {
                color: colour,
                width: 0.0,
                radius: (DOT_SIZE / 2.0).into(),
            },
            shadow: Shadow::default(),
        })
        .into()
}

fn status_text<'a>(
    label: String,
    size: f32,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    text(label)
        .font(reading.font)
        .size(size)
        .wrapping(Wrapping::None)
        .style(move |_| text::Style {
            color: Some(tokens.text_secondary),
        })
        .into()
}

pub(super) fn commit_count_label(count: usize) -> String {
    match count {
        1 => "1 commit loaded".to_string(),
        count => format!("{count} commits loaded"),
    }
}

pub(super) fn upstream_name(status: Option<&RepoStatus>) -> String {
    match status {
        None => String::new(),
        Some(RepoStatus { tracking: None, .. }) => NO_UPSTREAM.to_string(),
        Some(RepoStatus {
            tracking: Some(tracking),
            ..
        }) => match (tracking.ahead, tracking.behind) {
            (0, 0) => tracking.upstream.clone(),
            (ahead, 0) => format!("{} ↑{ahead}", tracking.upstream),
            (0, behind) => format!("{} ↓{behind}", tracking.upstream),
            (ahead, behind) => format!("{} ↑{ahead} ↓{behind}", tracking.upstream),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_domain::TrackingStatus;

    #[test]
    fn commit_count_label_uses_the_singular_for_one() {
        assert_eq!(commit_count_label(0), "0 commits loaded");
        assert_eq!(commit_count_label(1), "1 commit loaded");
        assert_eq!(commit_count_label(50), "50 commits loaded");
    }

    #[test]
    fn upstream_name_is_blank_until_status_arrives() {
        assert_eq!(upstream_name(None), "");
        assert_eq!(
            upstream_name(Some(&RepoStatus {
                staged_files: vec![],
                unstaged_files: vec![],
                untracked_files: vec![],
                tracking: None,
                head_commit_id: None,
            })),
            "No upstream"
        );
        assert_eq!(
            upstream_name(Some(&RepoStatus {
                staged_files: vec![],
                unstaged_files: vec![],
                untracked_files: vec![],
                tracking: Some(TrackingStatus {
                    upstream: "origin/dev".to_string(),
                    ahead: 1,
                    behind: 0,
                }),
                head_commit_id: None,
            })),
            "origin/dev ↑1"
        );
    }

    #[test]
    fn error_summary_takes_the_first_non_blank_line() {
        assert_eq!(
            error_summary("fatal: refusing to merge unrelated histories\nhint: …"),
            "fatal: refusing to merge unrelated histories"
        );
    }
}
