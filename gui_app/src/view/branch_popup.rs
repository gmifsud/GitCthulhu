//! The branches popup under the top-bar branch chip, after the JetBrains "Git branches"
//! popup: a filter, New Branch and Merge at the top, then local and remote branches. A
//! branch opens its menu (checkout, new branch from it, merge, rebase, rename, delete…).

use crate::app::{GitApp, Message, BRANCH_POPUP_ID};
use crate::icons::Icon;
use crate::theme::DesignTokens;
use core_domain::Branch;
use iced::widget::button::Status;
use iced::widget::{button, column, container, horizontal_space, mouse_area, row, scrollable, text, text_input};
use iced::{Alignment, Background, Border, Color, Element, Length, Padding, Shadow, Theme};

use super::chrome::{field_style, glyph, one_line, Reading};

const POPUP_WIDTH: f32 = 420.0;
const POPUP_TOP: f32 = 44.0;
const LIST_HEIGHT: f32 = 460.0;
/// Most rows a filter draws per group; typing narrows further.
const MAX_ROWS: usize = 300;

/// True when `name` contains every whitespace-separated term of `filter`, ignoring case.
pub(crate) fn branch_matches(name: &str, filter: &str) -> bool {
    let name = name.to_lowercase();
    filter.split_whitespace().all(|term| name.contains(&term.to_lowercase()))
}

pub(super) fn branch_popup<'a>(app: &'a GitApp, tokens: DesignTokens, reading: Reading) -> Option<Element<'a, Message>> {
    if !app.branch_list_open {
        return None;
    }
    let session = app.session();
    let filter = app.branch_popup_filter.trim();
    let busy = session.state.loading;
    let mut list = column![
        action_row(Icon::Add, "New Branch…", Message::AskCreateBranchFrom("HEAD".to_string()), !busy, tokens, reading),
        action_row(Icon::Merge, "Merge into Current…", Message::AskMerge, !busy, tokens, reading),
    ]
    .spacing(1);
    for (title, remote) in [("Local", false), ("Remote", true)] {
        let branches: Vec<&Branch> = session
            .state
            .branches
            .iter()
            .filter(|branch| branch.is_remote == remote && branch_matches(&branch.name, filter))
            .collect();
        if branches.is_empty() {
            continue;
        }
        list = list.push(
            container(label(&format!("{title} · {}", branches.len()), tokens.text_muted, reading)).padding([6, 8]),
        );
        for branch in branches.into_iter().take(MAX_ROWS) {
            list = list.push(branch_row(branch, tokens, reading));
        }
    }

    let panel = container(
        column![
            text_input("Search branches", &app.branch_popup_filter)
                .id(BRANCH_POPUP_ID)
                .on_input(Message::BranchPopupFilterChanged)
                .font(reading.font)
                .size(reading.size)
                .padding([6, 10])
                .style(field_style(tokens)),
            container(scrollable(list).height(Length::Shrink)).max_height(LIST_HEIGHT),
        ]
        .spacing(8),
    )
    .padding(8)
    .width(Length::Fixed(POPUP_WIDTH))
    .style(move |_theme: &Theme| container::Style {
        background: Some(Background::Color(tokens.bg_chrome)),
        text_color: Some(tokens.text_primary),
        border: Border { color: tokens.chip_border, width: 1.0, radius: 6.0.into() },
        shadow: Shadow { color: Color::from_rgba(0.0, 0.0, 0.0, 0.5), offset: iced::Vector::new(0.0, 6.0), blur_radius: 18.0 },
    });
    // A click on the panel's padding must not reach the backdrop, which closes the popup.
    let panel = mouse_area(panel).on_press(Message::BranchPopupFilterChanged(app.branch_popup_filter.clone()));
    let backdrop = mouse_area(
        container(column![panel])
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(Padding { top: POPUP_TOP, ..Padding::ZERO })
            .center_x(Length::Fill)
            .style(move |_theme: &Theme| container::Style {
                background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.25))),
                ..container::Style::default()
            }),
    )
    .on_press(Message::ToggleBranchList);
    Some(backdrop.into())
}

fn label<'a>(value: &str, color: Color, reading: Reading) -> Element<'a, Message> {
    one_line(
        text(value.to_string()).font(reading.font).size(reading.size).style(move |_| text::Style { color: Some(color) }),
        reading.size,
        Length::Shrink,
    )
}

fn row_style(tokens: DesignTokens, status: Status, enabled: bool) -> iced::widget::button::Style {
    let hot = enabled && matches!(status, Status::Hovered | Status::Pressed);
    iced::widget::button::Style {
        background: Some(Background::Color(if hot { tokens.bg_raised } else { Color::TRANSPARENT })),
        text_color: tokens.text_primary,
        border: Border { radius: 3.0.into(), ..Border::default() },
        shadow: Shadow::default(),
    }
}

fn action_row<'a>(icon: Icon, title: &'static str, message: Message, enabled: bool, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    let ink = if enabled { tokens.text_primary } else { tokens.text_muted };
    button(row![glyph(icon, reading.size, tokens.accent_primary, &reading), label(title, ink, reading)].spacing(8).align_y(Alignment::Center))
        .padding([4, 8])
        .width(Length::Fill)
        .on_press_maybe(enabled.then_some(message))
        .style(move |_theme, status| row_style(tokens, status, enabled))
        .into()
}

fn branch_row<'a>(branch: &Branch, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    let current = branch.is_head && !branch.is_remote;
    let ink = if current { tokens.accent_primary } else { tokens.text_primary };
    let mut line = row![
        glyph(if branch.is_remote { Icon::Cloud } else { Icon::Branch }, reading.size, ink, &reading),
        label(&branch.name, ink, reading),
        horizontal_space(),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    if branch.ahead > 0 {
        line = line.push(label(&format!("↑{}", branch.ahead), tokens.vcs_added, reading));
    }
    if branch.behind > 0 {
        line = line.push(label(&format!("↓{}", branch.behind), tokens.vcs_modified, reading));
    }
    if current {
        line = line.push(glyph(Icon::Check, reading.size, tokens.accent_primary, &reading));
    }
    button(line)
        .padding([3, 8])
        .width(Length::Fill)
        .on_press(Message::BranchPopupPick { name: branch.name.clone(), remote: branch.is_remote })
        .style(move |_theme, status| row_style(tokens, status, true))
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_term_must_match_ignoring_case() {
        assert!(branch_matches("feature/24433-MTL-counter", "mtl 244"));
        assert!(!branch_matches("feature/24433-MTL-counter", "mtl eyc"));
        assert!(branch_matches("main", ""));
    }
}
