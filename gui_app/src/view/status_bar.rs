use crate::app::{GitApp, Message};
use crate::theme::DesignTokens;
use core_domain::RepoStatus;
use iced::widget::text::Wrapping;
use iced::widget::{column, container, horizontal_space, row, text};
use iced::{Alignment, Background, Border, Element, Length, Shadow, Theme};

use super::chrome::Reading;

const STATUS_BAR_HEIGHT: f32 = 24.0;
const STATUS_PADDING: f32 = 12.0;
const STATUS_GAP: f32 = 14.0;
const DOT_SIZE: f32 = 7.0;
const NO_UPSTREAM: &str = "No upstream";

pub(super) fn status_bar<'a>(
    app: &'a GitApp,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let quiet = (reading.size - 2.0).max(11.0);
    let mut bar = row![loading_dot(app.state.loading, tokens)]
        .spacing(STATUS_GAP)
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .height(Length::Fixed(STATUS_BAR_HEIGHT));

    if let Some(error) = &app.error {
        bar = bar.push(
            container(
                text(error.clone())
                    .font(reading.font)
                    .size(quiet)
                    .wrapping(Wrapping::None)
                    .style(move |_| text::Style {
                        color: Some(tokens.accent_primary),
                    }),
            )
            .width(Length::Fill)
            .clip(true),
        );
    }

    bar = bar.push(horizontal_space());
    bar = bar.push(status_text(
        commit_count_label(app.state.commits.len()),
        quiet,
        tokens,
        reading,
    ));
    bar = bar.push(status_text(
        upstream_name(app.state.current_status.as_ref()),
        quiet,
        tokens,
        reading,
    ));

    container(bar)
        .width(Length::Fill)
        .height(Length::Fixed(STATUS_BAR_HEIGHT))
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
        1 => "1 commit".to_string(),
        count => format!("{count} commits"),
    }
}

pub(super) fn upstream_name(status: Option<&RepoStatus>) -> String {
    match status {
        None => String::new(),
        Some(RepoStatus { tracking: None, .. }) => NO_UPSTREAM.to_string(),
        Some(RepoStatus {
            tracking: Some(tracking),
            ..
        }) => tracking.upstream.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_domain::TrackingStatus;

    #[test]
    fn commit_count_label_uses_the_singular_for_one() {
        assert_eq!(commit_count_label(0), "0 commits");
        assert_eq!(commit_count_label(1), "1 commit");
        assert_eq!(commit_count_label(50), "50 commits");
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
            })),
            "origin/dev"
        );
    }
}
