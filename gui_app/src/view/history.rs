//! History table — Graph | Description | Author | When | Hash, one dense line per commit.
//! The graph is only as wide as its lanes. Ref pills sit at the right end of the
//! Description cell, after the subject, so rows without refs give the subject the room.
//! The top-bar search narrows the table to matching loaded commits.

use crate::app::{GitApp, Message};
use crate::commit_graph::{graph_rows, graph_width, lane_colour, GraphCell, GraphRow, NodeMark, ROW_HEIGHT};
use crate::context_menu::ContextTarget;
use crate::palette::commit_matches;
use crate::reachability::{filter_dag_nodes, session_head_tip};
use crate::ref_pills::{pill_mark, pill_tooltip, primary_node_kind, refs_at, RefPill, RefPillKind};
use crate::selection::{short_hash, Selection};
use crate::session::WorktreeSession;
use crate::wip::WipRow;
use crate::theme::DesignTokens;
use crate::history_index::{Columns, AUTHOR_WIDTH, COL_GAP, HASH_WIDTH, WHEN_WIDTH};
use core_domain::{Commit, DagNode};
use iced::widget::button::Status;
use iced::widget::text::Wrapping;
use iced::widget::{button, canvas, column, container, mouse_area, row, scrollable, text, tooltip, Space};
use iced::{Alignment, Background, Border, Color, Element, Length, Shadow, Theme};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use super::chrome::{glyph, panel_style, Reading};
use crate::dates::log_date;
use crate::icons::Icon;

/// Space between the subject and the first ref pill.
const SUBJECT_PILL_GAP: f32 = 8.0;
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
/// One text line inside a pill. iced 0.13 can still wrap `Wrapping::None` text when
/// squeezed, so pills are clamped to this height and clipped.
const PILL_HEIGHT: f32 = 16.0;
/// Between wrapped lines of pills.
const PILL_LINE_GAP: f32 = 3.0;
/// Between the names line and the subject.
const STACK_GAP: f32 = 2.0;
/// Above and below a commit row's text.
const ROW_PAD_Y: f32 = 3.0;
/// Left and right of the history list.
const LIST_PAD_X: f32 = 8.0;
/// Rows built above and below the view, in px, so a fast scroll never shows a gap.
const OVERSCAN_PX: f32 = 900.0;
/// Most rows a history search draws.
const MAX_SEARCH_ROWS: usize = 200;

pub(super) fn history_pane<'a>(
    app: &'a GitApp,
    selection: &Selection,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    // The table is laid out for the width it really has: rows are as tall as their wrapped
    // text, and columns step aside when the pane is narrow.
    let selection = selection.clone();
    iced::widget::responsive(move |size| history_table(app, &selection, size.width, tokens, reading)).into()
}

pub(super) fn history_table<'a>(
    app: &'a GitApp,
    selection: &Selection,
    width: f32,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let session = app.session();
    // Rebuild the row index only when what it depends on changed: never for a mouse move,
    // a hover or a scroll. This is what keeps a long history fast.
    // Rows are measured at the width rounded down to a step, so dragging a divider rebuilds
    // the index every step, not every pixel. Narrower than real is safe: text fits.
    let measured_width = (width / WIDTH_STEP).floor() * WIDTH_STEP;
    let key = index_key(app, session, measured_width, reading);
    let stale = session.history_index.borrow().as_ref().is_none_or(|index| index.key != key);
    if stale {
        let index = build_index(app, session, key, measured_width, reading);
        *session.history_layout.borrow_mut() = crate::session::HistoryLayout {
            rows: index.rows.iter().map(|row| (row.commit_id.clone(), PREAMBLE_HEIGHT + row.top)).collect(),
        };
        *session.history_index.borrow_mut() = Some(index);
    }
    let guard = session.history_index.borrow();
    let Some(index) = guard.as_ref() else {
        return Space::new(Length::Fill, Length::Fill).into();
    };
    let focus = session.focus_current_branch;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    // The sliding window: rows touching the view plus a margin each side for fast scrolls.
    // The offset is clamped to this list: one left from a longer history (another tab or
    // repository, deep in thousands of commits) pointed past the end of a short list, which
    // drew nothing and, too short to scroll, never sent the event that would have fixed it.
    let viewport = if app.history_viewport > 0.0 { app.history_viewport } else { 1_200.0 };
    let scroll = app.history_scroll.min((index.total + PREAMBLE_HEIGHT - viewport).max(0.0)).max(0.0);
    let (first, last) = index.window(
        scroll - PREAMBLE_HEIGHT - OVERSCAN_PX,
        scroll + viewport + OVERSCAN_PX,
    );
    // As wide as the lanes in view need: a narrow history drawn narrow.
    let graph_w = if index.narrowing {
        MIN_GRAPH_COL_WIDTH
    } else {
        graph_width(&index.rows[first..last].iter().map(|row| row.graph.clone()).collect::<Vec<_>>())
            .clamp(MIN_GRAPH_COL_WIDTH, MAX_GRAPH_COL_WIDTH)
            .min(index.graph_width)
    };
    let columns = index.columns;

    let mut commit_rows = column![column_headers(graph_w, columns, tokens, reading)].width(Length::Fill);
    if let Some(name) = &index.filter {
        commit_rows = commit_rows.push(filter_summary(name, index.filter_unloaded, index.shown, tokens, reading));
    }
    if index.narrowing {
        commit_rows = commit_rows.push(search_summary(index.rows.len(), session.state.dag_nodes.len(), tokens, reading));
    }

    let wip_rows = app.wip_rows();
    let inspecting = app.inspected_path().is_some();
    for wip in &wip_rows {
        // The inspected worktree is always drawn, so inspecting never needs a filter cleared.
        // Otherwise working changes belong to a worktree's HEAD: hide other worktrees under
        // any filter or search, and the active one too unless the filter is HEAD or its own branch.
        if !wip.inspected {
            if index.narrowing || (index.filter.is_some() && !wip.active) {
                continue;
            }
            if wip.active
                && !focus
                && session
                    .branch_filter
                    .as_ref()
                    .is_some_and(|name| session.local_branch(name).is_none_or(|branch| !branch.is_head))
            {
                continue;
            }
        }
        let selected = wip.inspected
            || (wip.active
                && !inspecting
                && matches!(selection, Selection::Branch { current: true, .. } | Selection::File { .. }));
        let line = button(history_row(
            graph_cell_empty(graph_w),
            wip_description_cell(wip, tokens, reading),
            "",
            "",
            "",
            false,
            // A WIP row names a worktree, so it grows to show the name whole.
            Length::Shrink,
            columns,
            tokens,
            reading,
        ))
        .width(Length::Fill)
        .padding(0)
        .on_press(Message::WorktreeClicked(wip.path.clone()))
        .style(move |_theme, status| commit_style(tokens, status, selected));
        commit_rows = commit_rows.push(hover_tip(line.into(), wip.tooltip.clone(), tokens, reading));
    }

    if first < index.rows.len() && index.rows[first].top > 0.0 {
        commit_rows = commit_rows.push(Space::with_height(Length::Fixed(index.rows[first].top)));
    }
    for row in &index.rows[first..last] {
        let selected = matches!(selection, Selection::Commit { id } if id == &row.commit_id);
        let age = log_date(now, row.timestamp);
        let mark = node_mark_for(&row.pills);
        let label = history_row(
            canvas(GraphCell { row: row.graph.clone(), mark })
                .width(Length::Fixed(graph_w))
                .height(Length::Fixed(row.height))
                .into(),
            description_cell(app, &row.subject, &row.pills, row.merge, row.height, lane_colour(row.graph.lane), tokens, reading),
            &row.author,
            &age,
            short_hash(&row.commit_id),
            row.merge,
            Length::Fixed(row.height),
            columns,
            tokens,
            reading,
        );
        let id = row.commit_id.clone();
        let right_target = ContextTarget::Commit { id: row.commit_id.clone() };
        commit_rows = commit_rows.push(
            mouse_area(
                button(label)
                    .width(Length::Fill)
                    .padding(0)
                    .on_press(Message::Select(Selection::Commit { id }))
                    .style(move |_theme, status| commit_style(tokens, status, selected)),
            )
            .on_right_press(Message::OpenContextMenu { target: right_target }),
        );
    }
    let drawn_to = index.rows.get(last).map_or(index.total, |row| row.top);
    if index.total > drawn_to {
        commit_rows = commit_rows.push(Space::with_height(Length::Fixed(index.total - drawn_to)));
    }

    // Streaming: older history keeps arriving in the background. When the view reaches the
    // end of what has arrived, say so with progress rather than leaving a blank end.
    let at_end = scroll + viewport + PREAMBLE_HEIGHT >= index.total;
    if session.history_more_pending && (at_end || index.rows.is_empty()) {
        let loaded = session.state.commits.len();
        let asked = session.history_limit.max(1);
        commit_rows = commit_rows.push(
            column![
                text(format!("Loading history… {loaded} commits so far"))
                    .font(reading.font)
                    .size(reading.size)
                    .style(move |_| text::Style { color: Some(tokens.text_muted) }),
                iced::widget::progress_bar(0.0..=asked as f32, loaded as f32).height(Length::Fixed(4.0)),
            ]
            .spacing(4)
            .padding([8, 0]),
        );
    }

    let any_wip = if focus { wip_rows.iter().any(|wip| wip.active || wip.inspected) } else { !wip_rows.is_empty() };
    if !index.narrowing && index.rows.is_empty() && !any_wip && !session.history_more_pending {
        let empty = if session.state.commits.is_empty() {
            "No commits yet"
        } else {
            "No commits on this view — try turning off Focus"
        };
        commit_rows = commit_rows.push(
            text(empty)
                .font(reading.font)
                .size(reading.size)
                .style(move |_| text::Style { color: Some(tokens.text_muted) }),
        );
    }
    drop(guard);

    container(
        scrollable(commit_rows.padding([2.0, LIST_PAD_X]))
            .id(scrollable::Id::new(crate::app::HISTORY_SCROLL_ID))
            .width(Length::Fill)
            .on_scroll(|viewport| Message::HistoryScrolled {
                offset: viewport.absolute_offset().y,
                height: viewport.bounds().height,
                content: viewport.content_bounds().height,
            }),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(move |_theme: &Theme| panel_style(tokens, false))
    .into()
}

/// Width step the row index is measured at.
const WIDTH_STEP: f32 = 24.0;

/// Height of what sits above the commit rows: the column headers. Summaries and WIP rows add
/// a little more, which the overscan margin covers.
const PREAMBLE_HEIGHT: f32 = HEADER_HEIGHT + 4.0;

/// What the index for this view depends on.
fn index_key(app: &GitApp, session: &WorktreeSession, width: f32, reading: Reading) -> crate::history_index::IndexKey {
    let head_counts = session
        .state
        .branches
        .iter()
        .find(|branch| branch.is_head && !branch.is_remote)
        .map(|branch| session.branch_counts(branch));
    crate::history_index::IndexKey {
        generation: session.history_generation,
        width_px: width.max(0.0) as u32,
        size_tenths: (reading.size * 10.0) as u32,
        query: app.history_query.trim().to_string(),
        author: session.author_filter.clone(),
        branch_filter: session.branch_filter.clone(),
        focus: session.focus_current_branch,
        worktrees: app.worktree_catalogue.len(),
        head_counts,
    }
}

/// Filter, search, lanes, refs and measured heights for every row, once.
fn build_index(
    app: &GitApp,
    session: &WorktreeSession,
    key: crate::history_index::IndexKey,
    width: f32,
    reading: Reading,
) -> crate::history_index::HistoryIndex {
    let focus = session.focus_current_branch;
    let head_tip = session_head_tip(
        &session.state.branches,
        session.state.current_status.as_ref().and_then(|status| status.head_commit_id.as_deref()),
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
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    // A filter must never blank the graph: when its tip is not among the loaded commits
    // (remote-only history, or pushed off the page), say so and keep the full page.
    let filter_unloaded = filter.is_some() && filtered_owned.is_empty();
    let shown_nodes: &[DagNode] = if filter.is_some() && !filtered_owned.is_empty() {
        &filtered_owned
    } else {
        &session.state.dag_nodes
    };
    let query = key.query.as_str();
    let searching = !query.is_empty();
    let author = session.author_filter.as_deref();
    // Search and User ▾ pick rows out of the history, so lanes no longer join them up.
    let narrowing = searching || author.is_some();
    let commits_by_id: HashMap<&str, &Commit> =
        session.state.commits.iter().map(|commit| (commit.id.as_str(), commit)).collect();
    let rows = graph_rows(shown_nodes);
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
            .map(|node| (node, GraphRow { lane: node.lane, incoming: Vec::new(), passing: Vec::new(), outgoing: Vec::new() }))
            .collect()
    } else {
        shown_nodes.iter().zip(rows).collect()
    };
    let graph_w = if narrowing {
        MIN_GRAPH_COL_WIDTH
    } else {
        graph_width(&table.iter().map(|(_, row)| row.clone()).collect::<Vec<_>>())
            .clamp(MIN_GRAPH_COL_WIDTH, MAX_GRAPH_COL_WIDTH)
    };
    let columns = crate::history_index::Columns::for_width(width - LIST_PAD_X * 2.0 - graph_w);
    // Measured against the widest lane column: text never needs more room than measured.
    let description_w = columns.description_width(width - LIST_PAD_X * 2.0 - graph_w);

    // Refs grouped by the commit they point at, once: looking each row up in every branch
    // was the other half of the slowness with a thousand remote branches.
    let mut tips: HashMap<&str, (Vec<core_domain::Branch>, Vec<core_domain::Tag>)> = HashMap::new();
    for branch in &session.state.branches {
        tips.entry(branch.tip.as_str()).or_default().0.push(branch.clone());
    }
    for tag in &session.state.tags {
        tips.entry(tag.commit_id.as_str()).or_default().1.push(tag.clone());
    }
    let head = session.state.branches.iter().find(|branch| branch.is_head && !branch.is_remote);
    let head_counts = head.map(|branch| (branch.name.clone(), session.branch_counts(branch)));

    let mut out = Vec::with_capacity(table.len());
    let mut top = 0.0_f32;
    for (node, graph) in table {
        let Some(commit) = commits_by_id.get(node.commit_id.as_str()) else {
            continue;
        };
        let mut pills = tips
            .get(commit.id.as_str())
            .map(|(branches, tags)| refs_at(&commit.id, branches, tags, &app.worktree_catalogue))
            .unwrap_or_default();
        // The checked-out branch's counts come from the header's tracking state, like every
        // other place they are shown.
        if let Some((name, (ahead, behind))) = &head_counts {
            for pill in pills.iter_mut().filter(|pill| &pill.name == name) {
                pill.ahead = *ahead;
                pill.behind = *behind;
            }
        }
        let subject = subject_of(&commit.message).to_string();
        let height = row_height(&subject, &pills, description_w, reading);
        out.push(crate::history_index::IndexRow {
            commit_id: commit.id.clone(),
            subject,
            author: commit.author.clone(),
            timestamp: commit.timestamp,
            merge: commit.parent_ids.len() > 1,
            graph,
            pills,
            height,
            top,
        });
        top += height;
    }
    crate::history_index::HistoryIndex {
        key,
        rows: out,
        total: top,
        filter: filter.map(|(name, _)| name),
        filter_unloaded,
        shown: shown_nodes.len(),
        narrowing,
        columns,
        graph_width: graph_w,
    }
}

#[allow(clippy::too_many_arguments)]
fn history_row<'a>(
    graph: Element<'a, Message>,
    description: Element<'a, Message>,
    author: &str,
    when: &str,
    hash: &str,
    muted: bool,
    height: Length,
    columns: Columns,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let mut line = row![graph, description].spacing(COL_GAP);
    if columns.author {
        line = line.push(single_line(author, Length::Fixed(AUTHOR_WIDTH), reading, tokens, false, muted));
    }
    if columns.when {
        line = line.push(single_line(when, Length::Fixed(WHEN_WIDTH), reading, tokens, false, muted));
    }
    if columns.hash {
        line = line.push(single_line(hash, Length::Fixed(HASH_WIDTH), reading, tokens, true, muted));
    }
    line.height(height)
    .align_y(Alignment::Center)
    .width(Length::Fill)
    .into()
}

/// Mirrors [`history_row`] cell for cell, so every header sits over its column. The focus
/// toggle lives in the History toolbar, not here: an extra header cell shifted the columns.
fn column_headers<'a>(graph_w: f32, columns: Columns, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    let mut line = row![
        // The graph column has no title: "Description" lines up with the subjects.
        container(text("")).width(Length::Fixed(graph_w)),
        header_cell("Description", Length::Fill, tokens, reading),
    ]
    .spacing(COL_GAP);
    if columns.author {
        line = line.push(header_cell("Author", Length::Fixed(AUTHOR_WIDTH), tokens, reading));
    }
    if columns.when {
        line = line.push(header_cell("Date (UTC)", Length::Fixed(WHEN_WIDTH), tokens, reading));
    }
    if columns.hash {
        line = line.push(header_cell("Hash", Length::Fixed(HASH_WIDTH), tokens, reading));
    }
    line
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

/// The worktree and branch names on top, then the subject, both whole and wrapping. Hovering
/// the names lists every ref at the commit by its full name.
fn description_cell<'a>(
    app: &GitApp,
    message: &str,
    pills: &[RefPill],
    muted: bool,
    height: f32,
    lane: Color,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let subject_colour = if muted { tokens.text_muted } else { tokens.text_primary };
    let subject = text(subject_of(message).to_string())
        .font(reading.font)
        .size(reading.size)
        .wrapping(Wrapping::WordOrGlyph)
        .style(move |_| text::Style { color: Some(subject_colour) });
    let mut cell = column![].spacing(STACK_GAP).width(Length::Fill);
    if !pills.is_empty() {
        let folder_of = |path: &str| app.worktree_folder(path);
        let names = pill_tooltip(pills, &app.worktree_catalogue, &folder_of);
        cell = cell.push(hover_tip(pill_strip(pills, lane, tokens, reading), names, tokens, reading));
    }
    cell = cell.push(subject);
    container(cell)
        .width(Length::Fill)
        .height(Length::Fixed(height))
        .padding([ROW_PAD_Y, 0.0])
        .align_y(Alignment::Center)
        .into()
}

/// The first line of a commit message.
fn subject_of(message: &str) -> &str {
    message.lines().next().unwrap_or_default()
}

/// How tall a commit row is: its names line (wrapped across the column) above its subject
/// (wrapped too), at least one base row.
fn row_height(subject: &str, pills: &[RefPill], width: f32, reading: Reading) -> f32 {
    let subject_h = crate::text_measure::height(subject, reading.font, reading.size, width);
    let names_h = if pills.is_empty() {
        0.0
    } else {
        pill_lines(pills, width, reading) as f32 * (PILL_HEIGHT + PILL_LINE_GAP) + STACK_GAP
    };
    (subject_h + names_h + ROW_PAD_Y * 2.0).max(ROW_HEIGHT)
}

/// Lines the pills take when wrapped into `width`.
fn pill_lines(pills: &[RefPill], width: f32, reading: Reading) -> usize {
    let small = (reading.size - 3.0).max(10.0);
    let mut lines = 1;
    let mut used = 0.0;
    for pill in pills {
        let mut pill_w = crate::text_measure::width(&pill.label, reading.font, small) + small + 3.0 + PILL_PAD_X * 2.0 + 2.0;
        for count in [pill.behind, pill.ahead] {
            if count > 0 {
                pill_w += crate::text_measure::width(&format!("↓{count}"), reading.mono, (reading.size - 4.0).max(9.0)) + 3.0;
            }
        }
        // A pill wider than the line wraps inside itself, taking a line per line it wraps to.
        if pill_w > width {
            lines += (pill_w / width).ceil() as usize - 1;
        }
        let pill_w = pill_w.min(width);
        if used > 0.0 && used + PILL_GAP + pill_w > width {
            lines += 1;
            used = pill_w;
        } else {
            used += if used > 0.0 { PILL_GAP } else { 0.0 } + pill_w;
        }
    }
    lines
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
    lane: Color,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    // Every ref at the commit, whole, wrapping onto more lines when they do not fit. Nothing
    // hides behind a "+N": a hidden worktree is exactly the mix-up this row must prevent.
    let mut pill_row = row![].spacing(PILL_GAP).align_y(Alignment::Center);
    for pill in pills {
        pill_row = pill_row.push(ref_pill(pill, lane, tokens, reading));
    }
    pill_row.wrap().into()
}

/// "Working changes · n files" and the WIP pill. The pill is outlined, never filled, and
/// carries the edit mark, so it cannot pass for a branch or tag pill; the active
/// worktree's pill is in the accent colour and says so.
fn wip_description_cell<'a>(
    wip: &WipRow,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let files = if wip.changes == 1 { "1 file".to_string() } else { format!("{} files", wip.changes) };
    let ink = if wip.active { tokens.accent_primary } else { tokens.text_secondary };
    let edge = if wip.active { tokens.accent_primary } else { tokens.text_muted };
    let small = (reading.size - 3.0).max(10.0);
    wip_cell(wip.label.clone(), files, wip.active, ink, edge, small, tokens, reading)
}

#[allow(clippy::too_many_arguments)]
fn wip_cell<'a>(
    label: String,
    files: String,
    active: bool,
    ink: iced::Color,
    edge: iced::Color,
    small: f32,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let mut body = row![
        glyph(Icon::Edit, (reading.size - 3.0).max(9.0), ink, &reading),
        // Whole, wrapping if the column is narrow: never cut.
        text(label)
            .font(reading.font)
            .size(small)
            .wrapping(Wrapping::WordOrGlyph)
            .style(move |_| text::Style { color: Some(ink) }),
    ]
    .spacing(3)
    .align_y(Alignment::Center);
    if active {
        body = body.push(
            text("· active")
                .font(reading.font)
                .size(small)
                .wrapping(Wrapping::None)
                .style(move |_| text::Style { color: Some(tokens.text_muted) }),
        );
    }
    let pill = container(body)
        .align_y(Alignment::Center)
        .padding([PILL_PAD_Y + 2.0, PILL_PAD_X])
        .style(move |_theme: &Theme| container::Style {
            background: None,
            text_color: Some(ink),
            border: Border {
                color: edge,
                width: 1.0,
                radius: PILL_RADIUS.into(),
            },
            shadow: Shadow::default(),
        });
    // The pill first: it names the worktree. The count follows, muted.
    let stack = row![
        pill,
        text(files)
            .font(reading.font)
            .size(reading.size)
            .wrapping(Wrapping::None)
            .style(move |_| text::Style {
                color: Some(tokens.text_muted),
            }),
    ]
    .spacing(SUBJECT_PILL_GAP)
    .align_y(Alignment::Center);
    container(stack)
        .width(Length::Fill)
        .padding([ROW_PAD_Y, 0.0])
        .align_y(Alignment::Center)
        .into()
}

/// `content` with `tip` shown under it on hover, in the tab strip's tooltip style.
fn hover_tip<'a>(
    content: Element<'a, Message>,
    tip: String,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    tooltip(
        content,
        container(text(tip).font(reading.mono).size((reading.size - 1.0).max(10.0)))
            .padding([4, 8])
            .style(move |_theme: &Theme| container::Style {
                background: Some(Background::Color(tokens.bg_app)),
                text_color: Some(tokens.text_primary),
                border: Border {
                    color: tokens.border,
                    width: 1.0,
                    radius: 4.0.into(),
                },
                shadow: Shadow::default(),
            }),
        tooltip::Position::Bottom,
    )
    .into()
}

/// A ref at a commit, in the colour of the lane it sits on, so a coloured line in the graph
/// can be told by name: the orange line is the branch whose pill is orange. The shape says
/// the kind: filled for the checkout, outlined for a worktree or branch, faint for a remote
/// (with its cloud mark), neutral for a tag.
fn ref_pill<'a>(pill: &RefPill, lane: Color, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    let faint = Color { a: 0.55, ..lane };
    let (fg, bg, border) = match pill.kind {
        RefPillKind::Head => (tokens.bg_app, lane, lane),
        RefPillKind::Worktree => (lane, tokens.bg_raised, lane),
        RefPillKind::Local => (lane, tokens.chip, lane),
        RefPillKind::Remote => (faint, tokens.bg_raised, faint),
        RefPillKind::Tag => (tokens.text_secondary, tokens.chip, tokens.chip_border),
    };
    let mark = pill_mark(pill.kind);
    let mut body = row![
        glyph(mark, (reading.size - 3.0).max(9.0), fg, &reading),
        // Whole: a long name wraps inside its pill.
        text(pill.label.clone())
            .font(reading.font)
            .size((reading.size - 3.0).max(10.0))
            .wrapping(Wrapping::WordOrGlyph)
            .style(move |_| text::Style { color: Some(fg) }),
    ]
    .spacing(3)
    .align_y(Alignment::Center);
    // ↓ incoming / ↑ outgoing, in the pill's own ink: the arrow says which, never a colour.
    for (count, arrow, colour) in [(pill.behind, '↓', fg), (pill.ahead, '↑', fg)] {
        if count > 0 {
            body = body.push(
                text(format!("{arrow}{count}"))
                    .font(reading.mono)
                    .size((reading.size - 4.0).max(9.0))
                    .wrapping(Wrapping::None)
                    .style(move |_| text::Style { color: Some(colour) }),
            );
        }
    }
    container(body)
        .height(Length::Shrink)
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
mod measured_rows_tests {
    use super::*;
    use crate::ref_pills::RefPillKind;

    fn reading() -> Reading {
        Reading { font: iced::Font::DEFAULT, mono: iced::Font::MONOSPACE, size: 13.0 }
    }

    fn pill(name: &str) -> RefPill {
        RefPill { label: name.into(), name: name.into(), kind: RefPillKind::Worktree, ahead: 0, behind: 0 }
    }

    #[test]
    fn a_row_grows_to_hold_its_names_and_its_whole_subject() {
        let plain = row_height("Update", &[], 400.0, reading());
        assert_eq!(plain, ROW_HEIGHT, "a short subject is one base row");
        let named = row_height("Update", &[pill("feature/tenant-db-isolation-shared-instance")], 400.0, reading());
        assert!(named > plain, "the names line sits above the subject: {named}");
        let long = "fix(plt): Correct tenant titles on first load and add notification page titles across every module";
        let wrapped = row_height(long, &[], 150.0, reading());
        assert!(wrapped > ROW_HEIGHT * 1.5, "a long subject wraps rather than being cut: {wrapped}");
    }

    #[test]
    fn many_names_wrap_onto_more_lines() {
        let names: Vec<RefPill> = (0..6).map(|i| pill(&format!("worktree-branch-number-{i}"))).collect();
        assert!(pill_lines(&names, 300.0, reading()) >= 2);
        assert_eq!(pill_lines(&names[..1], 3_000.0, reading()), 1);
    }
}
