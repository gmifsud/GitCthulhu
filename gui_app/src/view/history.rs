//! History table — Graph | Description | Author | When | Hash, one dense line per commit.
//! The graph is only as wide as its lanes. Ref pills sit at the right end of the
//! Description cell, after the subject, so rows without refs give the subject the room.
//! The top-bar search narrows the table to matching loaded commits.

use crate::app::{GitApp, Message};
use crate::commit_graph::{graph_rows, graph_width, GraphCell, GraphRow, NodeMark, ROW_HEIGHT};
use crate::context_menu::ContextTarget;
use crate::jobs::COMMIT_PAGE_SIZE;
use crate::palette::commit_matches;
use crate::reachability::{filter_dag_nodes, session_head_tip};
use crate::ref_pills::{pill_mark, primary_node_kind, refs_at, RefPill, RefPillKind};
use crate::selection::{short_hash, Selection};
use crate::session::{basename, WorktreeSession};
use crate::theme::DesignTokens;
use core_domain::{Commit, DagNode};
use iced::widget::button::Status;
use iced::widget::text::Wrapping;
use iced::widget::{button, canvas, column, container, mouse_area, row, scrollable, text};
use iced::{Alignment, Background, Border, Color, Element, Length, Shadow, Theme};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use super::chrome::{glyph, panel_style, Reading};
use crate::dates::log_date;
use crate::icons::Icon;

const COL_GAP: f32 = 10.0;
/// Space between the subject and the first ref pill.
const SUBJECT_PILL_GAP: f32 = 8.0;
const HASH_WIDTH: f32 = 64.0;
/// Fits "Yesterday 23:59" and "28 Sep 2026".
const WHEN_WIDTH: f32 = 112.0;
const AUTHOR_WIDTH: f32 = 120.0;
const MIN_GRAPH_COL_WIDTH: f32 = 24.0;
/// Ten lanes. Wider histories clip at the right edge of the graph instead of taking the
/// Description column: the subject is what is read, lanes beyond ten rarely are.
const MAX_GRAPH_COL_WIDTH: f32 = 160.0;
const HEADER_SIZE: f32 = 11.0;
/// iced 0.13 still wraps `Wrapping::None` text once a cell is squeezed, which grew the
/// header row until it pushed every commit below the fold. Header cells are clamped.
const HEADER_HEIGHT: f32 = 20.0;
const PILL_PAD_Y: f32 = 0.0;
const PILL_PAD_X: f32 = 5.0;
const PILL_RADIUS: f32 = 3.0;
const PILL_GAP: f32 = 3.0;
/// One pill (the most important ref: HEAD, worktree, local, remote, tag) and a "+N" count.
const MAX_PILLS_SHOWN: usize = 1;
/// Widest the pill strip may grow; the subject keeps the rest of the cell.
const PILL_STRIP_MAX: f32 = 180.0;
/// Description column split between the subject and the ref pills.
const SUBJECT_PORTION: u16 = 3;
const PILL_PORTION: u16 = 1;
/// One text line inside a pill. iced 0.13 can still wrap `Wrapping::None` text when
/// squeezed, so pills are clamped to this height and clipped.
const PILL_HEIGHT: f32 = 16.0;
/// Most rows a history search draws.
const MAX_SEARCH_ROWS: usize = 200;

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
    // The log shows what one tip reaches: HEAD under Focus, else the branch clicked in the
    // branch tree. `None` shows every loaded branch.
    let filter: Option<(String, Option<String>)> = if focus {
        Some(("HEAD".to_string(), head_tip.clone()))
    } else {
        session.branch_filter.as_ref().map(|name| {
            let tip = session
                .local_branch(name)
                .or_else(|| session.remote_branch(name))
                .map(|branch| branch.tip.clone());
            (name.clone(), tip)
        })
    };
    let filtered_owned: Vec<DagNode> = filter
        .as_ref()
        .and_then(|(_, tip)| tip.as_deref())
        .map(|tip| {
            filter_dag_nodes(tip, &session.state.commits, &session.state.dag_nodes)
                .into_iter()
                .take(COMMIT_PAGE_SIZE)
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let page_end = session.state.dag_nodes.len().min(COMMIT_PAGE_SIZE);
    // A filter must never blank the graph: when its tip is not among the loaded commits
    // (remote-only history, or pushed off the page), say so and keep the full page.
    let filter_unloaded = filter.is_some() && filtered_owned.is_empty();
    let shown_nodes: &[DagNode] = if filter.is_some() && !filtered_owned.is_empty() {
        &filtered_owned
    } else {
        &session.state.dag_nodes[..page_end]
    };
    let query = app.history_query.trim();
    let searching = !query.is_empty();
    let author = session.author_filter.as_deref();
    // Search and User ▾ pick rows out of the history, so lanes no longer join them up.
    let narrowing = searching || author.is_some();
    let rows = graph_rows(shown_nodes);
    let graph_w = graph_width(&rows).clamp(MIN_GRAPH_COL_WIDTH, MAX_GRAPH_COL_WIDTH);
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

    // A search runs over every loaded commit, not only the first page. Lane lines mean
    // nothing between non-adjacent rows, so matches draw their dot alone.
    let table: Vec<(&DagNode, GraphRow)> = if narrowing {
        // A search reads every loaded commit; the author filter alone keeps the branch filter.
        let source: &[DagNode] = if searching { &session.state.dag_nodes } else { shown_nodes };
        source
            .iter()
            .filter(|node| {
                commits_by_id.get(node.commit_id.as_str()).is_some_and(|commit| {
                    author.is_none_or(|author| commit.author == author)
                        && (!searching || commit_matches(commit, &ref_names_at(&commit.id, session), query))
                })
            })
            .take(MAX_SEARCH_ROWS)
            .map(|node| {
                (
                    node,
                    GraphRow {
                        lane: node.lane,
                        incoming: Vec::new(),
                        passing: Vec::new(),
                        outgoing: Vec::new(),
                    },
                )
            })
            .collect()
    } else {
        shown_nodes.iter().zip(rows).collect()
    };
    let graph_w = if narrowing {
        table
            .iter()
            .map(|(_, row)| graph_width(std::slice::from_ref(row)))
            .fold(MIN_GRAPH_COL_WIDTH, f32::max)
            .min(MAX_GRAPH_COL_WIDTH)
    } else {
        graph_w
    };

    let mut commit_rows =
        column![column_headers(graph_w, tokens, reading)].width(Length::Fill);
    if let Some((name, _)) = &filter {
        commit_rows = commit_rows.push(filter_summary(name, filter_unloaded, shown_nodes.len(), tokens, reading));
    }
    if narrowing {
        commit_rows = commit_rows.push(search_summary(
            table.len(),
            session.state.dag_nodes.len(),
            tokens,
            reading,
        ));
    }

    for (index, session) in app.sessions.iter().enumerate() {
        if narrowing {
            break;
        }
        // Working changes belong to a session's HEAD: hide other sessions under any filter,
        // and this one too unless the filter is HEAD or its own branch.
        if filter.is_some() && index != app.active {
            continue;
        }
        if index == app.active
            && !focus
            && session
                .branch_filter
                .as_ref()
                .is_some_and(|name| session.local_branch(name).is_none_or(|branch| !branch.is_head))
        {
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
            button(history_row(
                graph_cell_empty(graph_w),
                wip_description_cell(&head, tokens, reading),
                "",
                "",
                "",
                false,
                tokens,
                reading,
            ))
            .width(Length::Fill)
            .padding(0)
            .on_press(Message::ShowSessionWorkingTree(index))
            .style(move |_theme, status| commit_style(tokens, status, selected)),
        );
    }

    for (node, graph_row) in table {
        let Some(&commit) = commits_by_id.get(node.commit_id.as_str()) else {
            continue;
        };
        let selected = matches!(selection, Selection::Commit { id } if id == &commit.id);
        let age = log_date(now, commit.timestamp);
        // Merge commits recede, as in the JetBrains log: the work is in their parents.
        let merge = commit.parent_ids.len() > 1;
        let pills = refs_at(
            &commit.id,
            &app.session().state.branches,
            &app.session().state.tags,
            &app.worktree_catalogue,
        );
        let mark = node_mark_for(&pills);
        let label = history_row(
            canvas(GraphCell {
                row: graph_row,
                mark,
            })
            .width(Length::Fixed(graph_w))
            .height(Length::Fixed(ROW_HEIGHT))
            .into(),
            description_cell(&commit.message, &pills, merge, tokens, reading),
            &commit.author,
            &age,
            short_hash(&commit.id),
            merge,
            tokens,
            reading,
        );
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
        app.sessions.get(app.active).is_some_and(session_dirty)
    } else {
        app.sessions.iter().any(session_dirty)
    };
    if narrowing {
        // The summary line already says how many matched.
    } else if shown_nodes.is_empty() && !any_wip {
        let empty = if session.state.commits.is_empty() {
            "No commits yet"
        } else {
            "No commits on this view — try turning off Focus"
        };
        commit_rows = commit_rows.push(
            text(empty)
                .font(reading.font)
                .size(reading.size)
                .style(move |_| text::Style {
                    color: Some(tokens.text_muted),
                }),
        );
    }

    container(scrollable(commit_rows.padding([2, 8])).width(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_theme: &Theme| panel_style(tokens, false))
        .into()
}

#[allow(clippy::too_many_arguments)]
fn history_row<'a>(
    graph: Element<'a, Message>,
    description: Element<'a, Message>,
    author: &str,
    when: &str,
    hash: &str,
    muted: bool,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    row![
        graph,
        description,
        single_line(author, Length::Fixed(AUTHOR_WIDTH), reading, tokens, false, muted),
        single_line(when, Length::Fixed(WHEN_WIDTH), reading, tokens, false, muted),
        single_line(hash, Length::Fixed(HASH_WIDTH), reading, tokens, true, muted),
    ]
    .spacing(COL_GAP)
    .height(Length::Fixed(ROW_HEIGHT))
    .align_y(Alignment::Center)
    .width(Length::Fill)
    .into()
}

/// Mirrors [`history_row`] cell for cell, so every header sits over its column. The focus
/// toggle lives in the History toolbar, not here: an extra header cell shifted the columns.
fn column_headers<'a>(graph_w: f32, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    row![
        // The graph column has no title: "Description" lines up with the subjects.
        container(text("")).width(Length::Fixed(graph_w)),
        header_cell("Description", Length::Fill, tokens, reading),
        header_cell("Author", Length::Fixed(AUTHOR_WIDTH), tokens, reading),
        header_cell("Date (UTC)", Length::Fixed(WHEN_WIDTH), tokens, reading),
        header_cell("Hash", Length::Fixed(HASH_WIDTH), tokens, reading),
    ]
    .spacing(COL_GAP)
    .padding([2, 0])
    .height(Length::Fixed(HEADER_HEIGHT))
    .align_y(Alignment::Center)
    .width(Length::Fill)
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
    .height(Length::Fixed(HEADER_HEIGHT))
    .clip(true)
    .into()
}

fn graph_cell_empty(graph_w: f32) -> Element<'static, Message> {
    container(text(""))
        .width(Length::Fixed(graph_w))
        .height(Length::Fixed(ROW_HEIGHT))
        .into()
}

/// Subject first, taking what is left; ref pills after it at their natural width.
fn description_cell<'a>(
    message: &str,
    pills: &[RefPill],
    muted: bool,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let subject_colour = if muted { tokens.text_muted } else { tokens.text_primary };
    let subject = container(
        text(message.lines().next().unwrap_or_default().to_string())
            .font(reading.font)
            .size(reading.size)
            .wrapping(Wrapping::None)
            .style(move |_| text::Style {
                color: Some(subject_colour),
            }),
    )
    .width(Length::FillPortion(SUBJECT_PORTION))
    .clip(true);
    let mut cell = row![subject]
        .spacing(SUBJECT_PILL_GAP)
        .align_y(Alignment::Center);
    if !pills.is_empty() {
        // At most a quarter of the column: on a narrow log the subject is what must stay.
        cell = cell.push(
            container(pill_strip(pills, tokens, reading))
                .width(Length::FillPortion(PILL_PORTION))
                .max_width(PILL_STRIP_MAX)
                .align_x(Alignment::End)
                .clip(true),
        );
    }
    container(cell)
        .width(Length::Fill)
        .height(Length::Fixed(ROW_HEIGHT))
        .clip(true)
        .align_y(Alignment::Center)
        .into()
}

/// "Branch: <name> · N commits  ✕" under the headers while the log is filtered to a branch.
fn filter_summary<'a>(
    name: &str,
    unloaded: bool,
    shown: usize,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let note = if unloaded {
        format!("Branch: {name} · not in the loaded history (the log reads local branches); showing all")
    } else {
        format!("Branch: {name} · {shown} commits")
    };
    row![
        text(note)
            .font(reading.font)
            .size(HEADER_SIZE)
            .wrapping(Wrapping::None)
            .style(move |_| text::Style {
                color: Some(if unloaded { tokens.accent_primary } else { tokens.text_muted }),
            }),
        button(
            row![
                glyph(Icon::Close, HEADER_SIZE, tokens.text_secondary, &reading),
                text("Show all").font(reading.font).size(HEADER_SIZE),
            ]
            .spacing(4)
            .align_y(Alignment::Center),
        )
            .padding([0, 6])
            .on_press(Message::ClearBranchFilter)
            .style(move |_theme, status| commit_style(tokens, status, false)),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .height(Length::Fixed(HEADER_HEIGHT))
    .into()
}

/// "N of M loaded commits match" under the headers while searching.
fn search_summary<'a>(
    matches: usize,
    loaded: usize,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let capped = if matches >= MAX_SEARCH_ROWS {
        format!(" (first {MAX_SEARCH_ROWS} shown)")
    } else {
        String::new()
    };
    container(
        text(format!("{matches} of {loaded} loaded commits match{capped}"))
            .font(reading.font)
            .size(HEADER_SIZE)
            .wrapping(Wrapping::None)
            .style(move |_| text::Style {
                color: Some(tokens.text_muted),
            }),
    )
    .padding([2, 0])
    .into()
}

/// Branch and tag names that tip at `commit_id`, for the history search.
fn ref_names_at<'s>(commit_id: &str, session: &'s WorktreeSession) -> Vec<&'s str> {
    session
        .state
        .branches
        .iter()
        .filter(|branch| branch.tip == commit_id)
        .map(|branch| branch.name.as_str())
        .chain(
            session
                .state
                .tags
                .iter()
                .filter(|tag| tag.commit_id == commit_id)
                .map(|tag| tag.name.as_str()),
        )
        .collect()
}

fn pill_strip<'a>(
    pills: &[RefPill],
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let mut pill_row = row![].spacing(PILL_GAP).align_y(Alignment::Center);
    for pill in pills.iter().take(MAX_PILLS_SHOWN) {
        pill_row = pill_row.push(ref_pill(pill, tokens, reading));
    }
    if pills.len() > MAX_PILLS_SHOWN {
        pill_row = pill_row.push(
            text(format!("+{}", pills.len() - MAX_PILLS_SHOWN))
                .font(reading.font)
                .size((reading.size - 2.0).max(10.0))
                .wrapping(Wrapping::None)
                .style(move |_| text::Style {
                    color: Some(tokens.text_muted),
                }),
        );
    }
    pill_row.into()
}

fn wip_description_cell<'a>(
    head: &str,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let pill = RefPill {
        label: format!("wip · {head}"),
        kind: RefPillKind::Head,
        ahead: 0,
        behind: 0,
    };
    let stack = row![
        text("Working changes")
            .font(reading.font)
            .size(reading.size)
            .wrapping(Wrapping::None)
            .style(move |_| text::Style {
                color: Some(tokens.text_secondary),
            }),
        ref_pill(&pill, tokens, reading),
    ]
    .spacing(SUBJECT_PILL_GAP)
    .align_y(Alignment::Center);
    container(stack)
        .width(Length::Fill)
        .height(Length::Fixed(ROW_HEIGHT))
        .clip(true)
        .align_y(Alignment::Center)
        .into()
}

fn ref_pill<'a>(pill: &RefPill, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    let (fg, bg, border) = match pill.kind {
        RefPillKind::Head => (tokens.bg_app, tokens.accent_primary, tokens.accent_primary),
        RefPillKind::Worktree => (tokens.accent_primary, tokens.bg_raised, tokens.accent_primary),
        RefPillKind::Local => (tokens.text_secondary, tokens.chip, tokens.chip_border),
        RefPillKind::Remote => (tokens.text_muted, tokens.bg_raised, tokens.border),
        RefPillKind::Tag => (tokens.added, tokens.chip, tokens.added),
    };
    let mark = pill_mark(pill.kind);
    let mut body = row![
        glyph(mark, (reading.size - 3.0).max(9.0), fg, &reading),
        text(pill.label.clone())
            .font(reading.font)
            .size((reading.size - 3.0).max(10.0))
            .wrapping(Wrapping::None)
            .style(move |_| text::Style { color: Some(fg) }),
    ]
    .spacing(3)
    .align_y(Alignment::Center);
    if pill.ahead > 0 {
        let ahead = pill.ahead;
        body = body.push(
            text(format!("+{ahead}"))
                .font(reading.font)
                .size((reading.size - 4.0).max(9.0))
                .wrapping(Wrapping::None)
                .style(move |_| text::Style {
                    color: Some(tokens.added),
                }),
        );
    }
    if pill.behind > 0 {
        let behind = pill.behind;
        body = body.push(
            text(format!("-{behind}"))
                .font(reading.font)
                .size((reading.size - 4.0).max(9.0))
                .wrapping(Wrapping::None)
                .style(move |_| text::Style {
                    color: Some(tokens.removed),
                }),
        );
    }
    container(body)
        .height(Length::Fixed(PILL_HEIGHT))
        .clip(true)
        .align_y(Alignment::Center)
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

fn node_mark_for(pills: &[RefPill]) -> NodeMark {
    match primary_node_kind(pills) {
        Some(RefPillKind::Remote) => NodeMark::Remote,
        Some(RefPillKind::Worktree) => NodeMark::Worktree,
        Some(RefPillKind::Head) | Some(RefPillKind::Local) => NodeMark::Local,
        Some(RefPillKind::Tag) | None => NodeMark::Plain,
    }
}

fn session_dirty(session: &WorktreeSession) -> bool {
    session.state.current_status.as_ref().is_some_and(|status| {
        !status.staged_files.is_empty()
            || !status.unstaged_files.is_empty()
            || !status.untracked_files.is_empty()
    })
}

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
    muted: bool,
) -> Element<'a, Message> {
    let face = if mono { reading.mono } else { reading.font };
    container(
        text(value.to_string())
            .font(face)
            .size(reading.size)
            .wrapping(Wrapping::None)
            .style(move |_| text::Style {
                color: Some(if muted {
                    tokens.text_muted
                } else if mono {
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
            unstaged_files: if dirty {
                vec!["a.rs".to_string()]
            } else {
                Vec::new()
            },
            untracked_files: Vec::new(),
            tracking: None,
            head_commit_id: None,
            kinds: Default::default(),
        });
        session.state.branches = head
            .map(|name| {
                vec![core_domain::Branch {
                    name: name.to_string(),
                    is_remote: false,
                    is_head: true,
                    tip: "abc".to_string(),
                    ahead: 0,
                    behind: 0,
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
        assert_eq!(
            wip_label(&session_at(r"D:\repo\wt", true, Some("feature"))),
            "feature"
        );
        assert_eq!(wip_label(&session_at(r"D:\repo\wt", true, None)), "wt");
    }
}
