use crate::app::{GitApp, Message};
use crate::commit_graph::{graph_rows, graph_width, GraphCell, ROW_HEIGHT};
use crate::context_menu::ContextTarget;
use crate::jobs::COMMIT_PAGE_SIZE;
use crate::reachability::{filter_dag_nodes, session_head_tip};
use crate::ref_pills::{refs_at, RefPill, RefPillKind};
use crate::selection::{short_hash, Selection};
use crate::session::{basename, WorktreeSession};
use crate::theme::DesignTokens;
use core_domain::{Commit, DagNode};
use iced::widget::button::Status;
use iced::widget::text::Wrapping;
use iced::widget::{button, canvas, column, container, mouse_area, row, scrollable, text, Space};
use iced::{Alignment, Background, Border, Color, Element, Length, Shadow, Theme};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use super::chrome::{icon_button, panel_style, Reading};

const COL_GAP: f32 = 10.0;
const HASH_WIDTH: f32 = 72.0;
const WHEN_WIDTH: f32 = 56.0;
const AUTHOR_WIDTH: f32 = 100.0;
const MIN_MESSAGE_WIDTH: f32 = 120.0;
const PILLS_MIN_WIDTH: f32 = 48.0;
const MESSAGE_FILL: u16 = 3;
const PILLS_FILL: u16 = 1;
/// Floor for the lane canvas so Graph never collapses into Description on resize.
const MIN_GRAPH_COL_WIDTH: f32 = 40.0;
const HEADER_SIZE: f32 = 11.0;
const PILL_PAD_Y: f32 = 1.0;
const PILL_PAD_X: f32 = 6.0;
const PILL_RADIUS: f32 = 3.0;
const PILL_GAP: f32 = 4.0;

pub(super) fn history_pane<'a>(
    app: &'a GitApp,
    selection: &Selection,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let session = app.session();
    let focus = session.focus_current_branch;
    let head_tip = session_head_tip(
        &session.state.branches,
        session
            .state
            .current_status
            .as_ref()
            .and_then(|status| status.head_commit_id.as_deref()),
    );
    let filtered_owned: Vec<DagNode> = if focus {
        head_tip
            .as_deref()
            .map(|tip| {
                filter_dag_nodes(tip, &session.state.commits, &session.state.dag_nodes)
                    .into_iter()
                    .take(COMMIT_PAGE_SIZE)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let page_end = session.state.dag_nodes.len().min(COMMIT_PAGE_SIZE);
    let shown_nodes: &[DagNode] = if focus {
        &filtered_owned
    } else {
        &session.state.dag_nodes[..page_end]
    };
    let rows = graph_rows(shown_nodes);
    let lanes_width = Length::Fixed(graph_width(&rows).max(MIN_GRAPH_COL_WIDTH));
    let commits_by_id: HashMap<&str, &Commit> = session
        .state
        .commits
        .iter()
        .map(|commit| (commit.id.as_str(), commit))
        .collect();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let mut commit_rows = column![column_headers(app, lanes_width, focus, tokens, reading)];

    // One WIP row per open session with changes, in tab order. Only the active tab's row can
    // be selected: the selection belongs to that session. Focus-current hides other sessions.
    for (index, session) in app.sessions.iter().enumerate() {
        if focus && index != app.active {
            continue;
        }
        if !session_dirty(session) {
            continue;
        }
        let selected = index == app.active
            && matches!(
                selection,
                Selection::Branch { current: true, .. } | Selection::File { .. }
            );
        let head = wip_label(session);
        commit_rows = commit_rows.push(
            button(
                row![
                    pills_cell(
                        &[RefPill {
                            label: format!("wip · {head}"),
                            kind: RefPillKind::Head,
                        }],
                        tokens,
                        reading,
                    ),
                    container(text(""))
                        .width(lanes_width)
                        .height(Length::Fixed(ROW_HEIGHT)),
                    message_cell("Working changes", tokens, reading),
                    single_line("", Length::Fixed(AUTHOR_WIDTH), reading, tokens, false),
                    single_line("", Length::Fixed(WHEN_WIDTH), reading, tokens, false),
                    single_line("", Length::Fixed(HASH_WIDTH), reading, tokens, true),
                ]
                .spacing(COL_GAP)
                .height(Length::Fixed(ROW_HEIGHT))
                .align_y(Alignment::Center),
            )
            .width(Length::Fill)
            .padding(0)
            .on_press(Message::ShowSessionWorkingTree(index))
            .style(move |_theme, status| commit_style(tokens, status, selected)),
        );
    }

    for (node, graph_row) in shown_nodes.iter().zip(rows) {
        let Some(&commit) = commits_by_id.get(node.commit_id.as_str()) else {
            continue;
        };
        let selected = matches!(selection, Selection::Commit { id } if id == &commit.id);
        let age = relative_age_compact(now, commit.timestamp);
        let pills = refs_at(&commit.id, &app.session().state.branches, &app.session().state.tags);
        let label = row![
            pills_cell(&pills, tokens, reading),
            canvas(GraphCell(graph_row))
                .width(lanes_width)
                .height(Length::Fixed(ROW_HEIGHT)),
            message_cell(&commit.message, tokens, reading),
            single_line(
                &commit.author,
                Length::Fixed(AUTHOR_WIDTH),
                reading,
                tokens,
                false
            ),
            single_line(&age, Length::Fixed(WHEN_WIDTH), reading, tokens, false),
            single_line(
                short_hash(&commit.id),
                Length::Fixed(HASH_WIDTH),
                reading,
                tokens,
                true
            ),
        ]
        .spacing(COL_GAP)
        .height(Length::Fixed(ROW_HEIGHT))
        .align_y(Alignment::Center);
        let id = commit.id.clone();
        let right_target = ContextTarget::Commit {
            id: commit.id.clone(),
        };
        commit_rows = commit_rows.push(
            mouse_area(
                button(label)
                    .width(Length::Fill)
                    .padding(0)
                    .on_press(Message::Select(Selection::Commit { id }))
                    .style(move |_theme, status| commit_style(tokens, status, selected)),
            )
            .on_right_press(Message::OpenContextMenu {
                target: right_target,
            }),
        );
    }

    let any_wip = if focus {
        app.sessions
            .get(app.active)
            .is_some_and(session_dirty)
    } else {
        app.sessions.iter().any(session_dirty)
    };
    if shown_nodes.is_empty() && !any_wip {
        commit_rows = commit_rows.push(
            text("No commits yet")
                .font(reading.font)
                .size(reading.size)
                .style(move |_| text::Style {
                    color: Some(tokens.text_muted),
                }),
        );
    }

    container(scrollable(commit_rows.padding([2, 8])))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_theme: &Theme| panel_style(tokens, false))
        .into()
}

fn column_headers<'a>(
    app: &'a GitApp,
    lanes_width: Length,
    focus: bool,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    row![
        header_cell("Refs", Length::FillPortion(PILLS_FILL), tokens, reading),
        container(
            text("Graph")
                .font(reading.font)
                .size(HEADER_SIZE)
                .wrapping(Wrapping::None)
                .style(move |_| text::Style {
                    color: Some(tokens.text_muted),
                }),
        )
        .width(lanes_width),
        header_cell(
            "Description",
            Length::FillPortion(MESSAGE_FILL),
            tokens,
            reading,
        ),
        header_cell("Author", Length::Fixed(AUTHOR_WIDTH), tokens, reading),
        header_cell("When", Length::Fixed(WHEN_WIDTH), tokens, reading),
        header_cell("Hash", Length::Fixed(HASH_WIDTH), tokens, reading),
        icon_button(
            "◎",
            if focus {
                "Show all branches"
            } else {
                "Focus current branch"
            },
            "graph-focus",
            app.tip_ready("graph-focus"),
            Message::ToggleFocusCurrentBranch,
            true,
            focus,
            tokens,
            reading,
        ),
    ]
    .spacing(COL_GAP)
    .padding([4, 0])
    .align_y(Alignment::Center)
    .into()
}

fn header_cell<'a>(
    label: &'static str,
    width: Length,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    container(
        text(label)
            .font(reading.font)
            .size(HEADER_SIZE)
            .wrapping(Wrapping::None)
            .style(move |_| text::Style {
                color: Some(tokens.text_muted),
            }),
    )
    .width(width)
    .into()
}

fn pills_cell<'a>(
    pills: &[RefPill],
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let mut cell = row![].spacing(PILL_GAP).align_y(Alignment::Center);
    for pill in pills.iter().take(4) {
        cell = cell.push(ref_pill(pill, tokens, reading));
    }
    if pills.len() > 4 {
        cell = cell.push(
            text(format!("+{}", pills.len() - 4))
                .font(reading.font)
                .size((reading.size - 2.0).max(11.0))
                .wrapping(Wrapping::None)
                .style(move |_| text::Style {
                    color: Some(tokens.text_muted),
                }),
        );
    }
    fill_column_with_min(PILLS_MIN_WIDTH, PILLS_FILL, cell)
}

fn message_cell<'a>(
    message: &str,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let body = text(message.to_string())
        .font(reading.font)
        .size(reading.size)
        .wrapping(Wrapping::None)
        .style(move |_| text::Style {
            color: Some(tokens.text_primary),
        });
    fill_column_with_min(MIN_MESSAGE_WIDTH, MESSAGE_FILL, body)
}

fn fill_column_with_min<'a>(
    min_width: f32,
    portion: u16,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    container(
        row![
            Space::new(Length::Fixed(min_width), Length::Fixed(0.0)),
            content.into(),
        ]
        .spacing(0.0)
        .align_y(Alignment::Center),
    )
    .width(Length::FillPortion(portion))
    .clip(true)
    .into()
}

fn ref_pill<'a>(pill: &RefPill, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    let (fg, bg, border) = match pill.kind {
        RefPillKind::Head => (tokens.bg_app, tokens.accent_primary, tokens.accent_primary),
        RefPillKind::Local => (tokens.text_secondary, tokens.chip, tokens.chip_border),
        RefPillKind::Remote => (tokens.text_muted, tokens.bg_raised, tokens.border),
        RefPillKind::Tag => (tokens.added, tokens.chip, tokens.added),
    };
    container(
        text(pill.label.clone())
            .font(reading.font)
            .size((reading.size - 3.0).max(10.0))
            .wrapping(Wrapping::None)
            .style(move |_| text::Style { color: Some(fg) }),
    )
    .padding([PILL_PAD_Y, PILL_PAD_X])
    .style(move |_theme: &Theme| container::Style {
        background: Some(Background::Color(bg)),
        text_color: Some(fg),
        border: Border {
            color: border,
            width: 1.0,
            radius: PILL_RADIUS.into(),
        },
        shadow: Shadow::default(),
    })
    .into()
}

/// Human-readable age of a commit from `now` (unix seconds) to `then`.
pub(crate) fn relative_age(now: i64, then: i64) -> String {
    let compact = relative_age_compact(now, then);
    if compact == "now" {
        "just now".to_string()
    } else {
        format!("{compact} ago")
    }
}

/// Compact age for the graph When column (`21h`, `3d`).
pub(crate) fn relative_age_compact(now: i64, then: i64) -> String {
    let secs = (now - then).max(0);
    if secs < 60 {
        return "now".to_string();
    }
    let mins = secs / 60;
    if mins < 60 {
        return format!("{mins}m");
    }
    let hours = mins / 60;
    if hours < 48 {
        return format!("{hours}h");
    }
    let days = hours / 24;
    if days < 14 {
        return format!("{days}d");
    }
    let weeks = days / 7;
    if weeks < 9 {
        return format!("{weeks}w");
    }
    let months = days / 30;
    if months < 24 {
        return format!("{months}mo");
    }
    format!("{}y", days / 365)
}

fn session_dirty(session: &WorktreeSession) -> bool {
    session.state.current_status.as_ref().is_some_and(|status| {
        !status.staged_files.is_empty()
            || !status.unstaged_files.is_empty()
            || !status.untracked_files.is_empty()
    })
}

/// Name shown on a session's WIP pill: its checked-out branch, else the folder name (a
/// detached HEAD or a session whose branches have not loaded yet).
fn wip_label(session: &WorktreeSession) -> String {
    session
        .state
        .branches
        .iter()
        .find(|branch| branch.is_head && !branch.is_remote)
        .map(|branch| branch.name.clone())
        .unwrap_or_else(|| basename(&session.path).to_string())
}

fn single_line<'a>(
    value: &str,
    width: Length,
    reading: Reading,
    tokens: DesignTokens,
    mono: bool,
) -> Element<'a, Message> {
    let face = if mono { reading.mono } else { reading.font };
    container(
        text(value.to_string())
            .font(face)
            .size(reading.size)
            .wrapping(Wrapping::None)
            .style(move |_| text::Style {
                color: Some(if mono {
                    tokens.text_secondary
                } else {
                    tokens.text_primary
                }),
            }),
    )
    .width(width)
    .clip(true)
    .into()
}

fn commit_style(
    tokens: DesignTokens,
    status: Status,
    selected: bool,
) -> iced::widget::button::Style {
    let hot = matches!(status, Status::Hovered | Status::Pressed);
    iced::widget::button::Style {
        background: Some(Background::Color(if selected || hot {
            tokens.border
        } else {
            Color::TRANSPARENT
        })),
        text_color: tokens.text_primary,
        border: Border::default(),
        shadow: Shadow::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session_at(path: &str, dirty: bool, head: Option<&str>) -> WorktreeSession {
        let mut session = WorktreeSession::test_stub();
        session.path = path.to_string();
        session.state.current_status = Some(core_domain::RepoStatus {
            staged_files: Vec::new(),
            unstaged_files: if dirty { vec!["a.rs".to_string()] } else { Vec::new() },
            untracked_files: Vec::new(),
            tracking: None,
            head_commit_id: None,
        });
        session.state.branches = head
            .map(|name| {
                vec![core_domain::Branch {
                    name: name.to_string(),
                    is_remote: false,
                    is_head: true,
                    tip: "abc".to_string(),
                }]
            })
            .unwrap_or_default();
        session
    }

    #[test]
    fn only_sessions_with_changes_are_dirty() {
        assert!(session_dirty(&session_at(r"D:\a", true, Some("main"))));
        assert!(!session_dirty(&session_at(r"D:\a", false, Some("main"))));
        assert!(!session_dirty(&WorktreeSession::test_stub()));
    }

    #[test]
    fn wip_label_is_the_branch_else_the_folder_name() {
        assert_eq!(wip_label(&session_at(r"D:\repo\wt", true, Some("feature"))), "feature");
        assert_eq!(wip_label(&session_at(r"D:\repo\wt", true, None)), "wt");
    }

    #[test]
    fn relative_age_steps_up_units() {
        assert_eq!(relative_age(100, 100), "just now");
        assert_eq!(relative_age(160, 100), "1m ago");
        assert_eq!(relative_age(100 + 3 * 3600, 100), "3h ago");
        assert_eq!(relative_age(100 + 5 * 86_400, 100), "5d ago");
        assert_eq!(relative_age(100 + 21 * 86_400, 100), "3w ago");
        assert_eq!(relative_age(100 + 70 * 86_400, 100), "2mo ago");
    }

    #[test]
    fn relative_age_compact_matches_the_graph_column() {
        assert_eq!(relative_age_compact(100, 100), "now");
        assert_eq!(relative_age_compact(100 + 21 * 3600, 100), "21h");
        assert_eq!(relative_age_compact(100 + 3 * 86_400, 100), "3d");
    }
}
