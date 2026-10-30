//! The bar under the tabs while a merge, rebase, cherry-pick or revert is stopped: what is in
//! progress, how many files still conflict, and Continue or Abort.

use crate::app::{GitApp, Message};
use crate::icons::Icon;
use crate::theme::DesignTokens;
use core_domain::RepoOperation;
use iced::widget::{container, horizontal_space, row, text, Space};
use iced::{Alignment, Background, Border, Element, Length, Shadow, Theme};

use super::chrome::{glyph, labelled_button, one_line, Reading};

/// What the bar says, and whether Continue is ready. Pure, for the tests.
pub(crate) fn banner_text(operation: RepoOperation, conflicts: usize) -> (String, String, bool) {
    let what = match operation {
        RepoOperation::Merge => "Merge in progress",
        RepoOperation::Rebase => "Rebase in progress",
        RepoOperation::CherryPick => "Cherry-pick in progress",
        RepoOperation::Revert => "Revert in progress",
    };
    let state = match conflicts {
        0 => "all conflicts resolved".to_string(),
        1 => "1 conflicted file: resolve it in the working tree".to_string(),
        n => format!("{n} conflicted files: resolve them in the working tree"),
    };
    let finish = match operation {
        RepoOperation::Merge => "Commit merge".to_string(),
        other => format!("Continue {}", other.noun()),
    };
    (format!("{what} · {state}"), finish, conflicts == 0)
}

/// The bar, or an empty strip. Always one widget, so the tree under it keeps its state.
pub(super) fn operation_banner<'a>(app: &'a GitApp, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    let status = app.session().state.current_status.as_ref();
    let Some((operation, conflicts)) = status.and_then(|status| status.operation.map(|op| (op, status.conflicted_files.len()))) else {
        return Space::new(Length::Fill, Length::Fixed(0.0)).into();
    };
    let (message, finish, ready) = banner_text(operation, conflicts);
    let busy = app.session().state.loading;
    let ink = if ready { tokens.vcs_added } else { tokens.vcs_conflicted };
    container(
        row![
            glyph(Icon::Merge, reading.size + 1.0, ink, &reading),
            one_line(
                text(message).font(reading.font).size(reading.size).style(move |_| text::Style { color: Some(tokens.text_primary) }),
                reading.size,
                Length::Shrink,
            ),
            horizontal_space(),
            labelled_button(Icon::Check, finish, Message::ContinueOperation, ready && !busy, true, tokens, reading),
            labelled_button(Icon::Discard, format!("Abort {}", operation.noun()), Message::AbortOperation, !busy, false, tokens, reading),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding([4, 12])
    .width(Length::Fill)
    .style(move |_theme: &Theme| container::Style {
        background: Some(Background::Color(tokens.bg_raised)),
        text_color: Some(tokens.text_primary),
        border: Border { color: ink, width: 1.0, radius: 0.0.into() },
        shadow: Shadow::default(),
    })
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn continue_waits_for_every_conflict() {
        let (text, finish, ready) = banner_text(RepoOperation::Merge, 2);
        assert_eq!(text, "Merge in progress · 2 conflicted files: resolve them in the working tree");
        assert_eq!(finish, "Commit merge");
        assert!(!ready);
        let (_, finish, ready) = banner_text(RepoOperation::Rebase, 0);
        assert_eq!(finish, "Continue rebase");
        assert!(ready);
    }
}
