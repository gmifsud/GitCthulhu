//! History table — Refs | Graph | Description | Author | When | Hash, one line per commit and
//! every row the same height. A commit's branches and tags sit in the Refs column, joined to its
//! node by a dotted line; the graph is only as wide as its lanes. A name too long for the column
//! is cut there, but shown whole on hover, and every name is in the row's right-click menu.
//! The top-bar search narrows the table to matching loaded commits.

use crate::app::{GitApp, Message};
use crate::commit_graph::{graph_rows, graph_width, lane_colour, row_inks, GraphCell, GraphRow, LeaderLine, NodeMark, RowInks, ROW_HEIGHT};
use crate::worktree_ink::WorktreeInks;
use crate::context_menu::ContextTarget;
use crate::palette::commit_matches;
use crate::reachability::{filter_dag_nodes, session_head_tip};
use crate::ref_pills::{pill_tooltip, primary_node_kind, refs_at, tip_worktree, RefPill, RefPillKind};
use crate::selection::{short_hash, Selection};
use crate::session::{same_path, WorktreeSession};
use crate::wip::WipRow;
use crate::theme::DesignTokens;
use crate::history_index::{Columns, AUTHOR_WIDTH, COL_GAP, HASH_WIDTH, WHEN_WIDTH};
use core_domain::{Commit, DagNode};
use iced::widget::button::Status;
use iced::widget::text::Wrapping;
use iced::widget::scrollable::{Direction, Scrollbar};
use iced::widget::{button, canvas, column, container, mouse_area, row, scrollable, text, tooltip, Space};

use super::history_scrollbar::{HistoryScrollbar, BAR_WIDTH};
use iced::{Alignment, Background, Border, Color, Element, Length, Shadow, Theme};
use std::collections::HashMap;

use super::chrome::{glyph, panel_style, Reading};
use crate::dates::log_datetime;
use crate::icons::Icon;

/// Zed's graph column: never narrower than this, however few the lanes.
const MIN_GRAPH_COL_WIDTH: f32 = 160.0;
/// Fifteen lanes. Wider histories clip at the right edge of the graph instead of taking the
/// Description column: the subject is what is read, lanes beyond that rarely are.
const MAX_GRAPH_COL_WIDTH: f32 = 306.0;
/// Inside every cell, left and right of its text.
const CELL_PAD: f32 = 6.0;
const HEADER_SIZE: f32 = 11.0;
/// iced 0.13 still wraps `Wrapping::None` text once a cell is squeezed, which grew the
/// header row until it pushed every commit below the fold. Header cells are clamped.
const HEADER_HEIGHT: f32 = 26.0;
const PILL_PAD_Y: f32 = 0.0;
const PILL_PAD_X: f32 = 5.0;
const PILL_RADIUS: f32 = 3.0;
const PILL_GAP: f32 = 3.0;
/// One text line inside a pill. iced 0.13 can still wrap `Wrapping::None` text when
/// squeezed, so pills are clamped to this height and clipped.
const PILL_HEIGHT: f32 = 18.0;
/// Room kept after the pills for the dotted line to the commit.
const LEADER_MIN: f32 = 10.0;
/// How strongly the dotted line shows: a quiet grey, as in Zed.
const LEADER_ALPHA: f32 = 0.6;
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
    // The history's own scrollbar stands beside the list, so the rows are that much narrower.
    let width = (width - BAR_WIDTH).max(0.0);
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
    // One width for the whole list, so the columns never shift as it scrolls.
    let graph_w = index.graph_width;
    let columns = index.columns;
    // What the subjects have to fit in, to end the long ones with "…".
    let subject_room = columns.description_width(width - LIST_PAD_X * 2.0 - graph_w) - CELL_PAD * 2.0;

    let mut commit_rows = column![column_headers(graph_w, columns, tokens, reading)].width(Length::Fill);
    if let Some(name) = &index.filter {
        commit_rows = commit_rows.push(filter_summary(name, index.filter_unloaded, index.shown, tokens, reading));
    }
    if index.narrowing {
        commit_rows = commit_rows.push(search_summary(index.rows.len(), session.state.dag_nodes.len(), tokens, reading));
    }

    let wip_rows = app.wip_rows();
    let inspecting = app.inspected_path().is_some();
    let worktree_inks = WorktreeInks::for_catalogue(&app.worktree_catalogue);
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
            wip_refs_cell(wip, worktree_inks.colour(&wip.path), columns.refs_width, tokens, reading),
            graph_cell_empty(graph_w),
            wip_summary(wip, tokens, reading),
            "",
            "",
            "",
            false,
            false,
            Length::Fixed(ROW_HEIGHT),
            columns,
            tokens,
            reading,
        ))
        .width(Length::Fill)
        .padding(0)
        .on_press(Message::WorktreeClicked(wip.path.clone()))
        .style(move |_theme, status| commit_style(tokens, status, selected));
        // The worktree's menu: open it, copy its path or branch.
        let line: Element<'a, Message> = match app.worktree_catalogue.iter().find(|entry| same_path(&entry.path, &wip.path)) {
            Some(entry) => mouse_area(line)
                .on_right_press(Message::OpenContextMenu {
                    target: ContextTarget::Worktree { path: entry.path.clone(), branch: entry.branch.clone(), is_main: entry.is_main },
                })
                .into(),
            None => line.into(),
        };
        commit_rows = commit_rows.push(hover_tip(line, wip.tooltip.clone(), tokens, reading));
    }

    if first < index.rows.len() && index.rows[first].top > 0.0 {
        commit_rows = commit_rows.push(Space::with_height(Length::Fixed(index.rows[first].top)));
    }
    for row in &index.rows[first..last] {
        let selected = matches!(selection, Selection::Commit { id } if id == &row.commit_id);
        let age = log_datetime(row.timestamp);
        let mark = node_mark_for(&row.pills);
        // Pills without a worktree of their own match the line their commit sits on.
        let line = row.inks.dot.unwrap_or_else(|| lane_colour(row.graph.lane));
        let leader = (!row.pills.is_empty()).then_some(Color { a: LEADER_ALPHA, ..tokens.text_muted });
        let label = history_row(
            refs_cell(app, row, line, leader, &worktree_inks, columns.refs_width, tokens, reading),
            canvas(GraphCell { row: row.graph.clone(), mark, inks: row.inks.clone(), leader })
                .width(Length::Fixed(graph_w))
                .height(Length::Fixed(row.height))
                .into(),
            description_cell(&row.subject, subject_room, tokens, reading),
            &row.author,
            &age,
            short_hash(&row.commit_id),
            row.unpushed,
            row.parents > 1,
            Length::Fixed(row.height),
            columns,
            tokens,
            reading,
        );
        let id = row.commit_id.clone();
        let right_target = ContextTarget::Commit { id: row.commit_id.clone(), subject: subject_of(&row.subject).to_string(), parents: row.parents };
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
    // Everything in the list: the headers, the rows, and the list's own padding.
    let content = PREAMBLE_HEIGHT + index.total + LIST_PAD_Y * 2.0;
    drop(guard);

    // iced's thumb shrinks to a hairline on a long history, so it is hidden (no width) and
    // the bar beside the list is the one to grab. The wheel and the keys still scroll the list.
    let list = scrollable(commit_rows.padding([LIST_PAD_Y, LIST_PAD_X]))
        .id(scrollable::Id::new(crate::app::HISTORY_SCROLL_ID))
        .direction(Direction::Vertical(Scrollbar::new().width(0).scroller_width(0)))
        .width(Length::Fill)
        .height(Length::Fill)
        .on_scroll(|viewport| Message::HistoryScrolled {
            offset: viewport.absolute_offset().y,
            height: viewport.bounds().height,
            content: viewport.content_bounds().height,
        });
    let bar = canvas(HistoryScrollbar { content, scroll, tokens })
        .width(Length::Fixed(BAR_WIDTH))
        .height(Length::Fill);

    container(row![list, bar])
    .width(Length::Fill)
    .height(Length::Fill)
    .style(move |_theme: &Theme| panel_style(tokens, false))
    .into()
}

/// Padding above and below the list inside its scrollable.
const LIST_PAD_Y: f32 = 2.0;

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
        worktrees: crate::worktree_ink::catalogue_fingerprint(&app.worktree_catalogue),
        head_counts,
    }
}

/// Filter, search, lanes, refs and measured heights for every row, once.
fn build_index(
    app: &GitApp,
    session: &WorktreeSession,
    key: crate::history_index::IndexKey,
    width: f32,
    _reading: Reading,
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
            .map(|(branches, tags)| refs_at(&commit.id, branches, tags, &app.worktree_catalogue, &session.path))
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
        // One line each: nothing is measured, so building the index is cheap at any size.
        let height = ROW_HEIGHT;
        out.push(crate::history_index::IndexRow {
            commit_id: commit.id.clone(),
            subject,
            author: commit.author.clone(),
            timestamp: commit.timestamp,
            parents: commit.parent_ids.len(),
            unpushed: session.state.unpushed.contains(&commit.id),
            graph,
            inks: RowInks::default(),
            pills,
            height,
            top,
        });
        top += height;
    }
    // Each worktree's colour from its tip down its lane, worked out once with the rest.
    let inks = WorktreeInks::for_catalogue(&app.worktree_catalogue);
    let tip_colours: Vec<Option<Color>> = out
        .iter()
        .map(|row| tip_worktree(&row.pills).map(|path| inks.colour(path)))
        .collect();
    let graphs: Vec<GraphRow> = out.iter().map(|row| row.graph.clone()).collect();
    for (row, ink) in out.iter_mut().zip(row_inks(&graphs, &tip_colours)) {
        row.inks = ink;
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

/// A thin vertical line between two columns, as in Zed. Rows touch, so it runs unbroken.
fn divider<'a>(tokens: DesignTokens) -> Element<'a, Message> {
    container(Space::new(Length::Fixed(COL_GAP), Length::Fill))
        .height(Length::Fill)
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(tokens.border)),
            ..container::Style::default()
        })
        .into()
}

/// A column's text: one line, ended with "…" when it does not fit `width`.
fn plain_cell<'a>(value: &str, width: f32, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    container(
        text(crate::text_measure::ellipsize(value, reading.font, reading.size, width - CELL_PAD * 2.0))
            .font(reading.font)
            .size(reading.size)
            .wrapping(Wrapping::None)
            .style(move |_| text::Style { color: Some(tokens.text_primary) }),
    )
    .width(Length::Fixed(width))
    .height(Length::Fill)
    .padding([0.0, CELL_PAD])
    .align_y(Alignment::Center)
    .clip(true)
    .into()
}

/// The short hash, with an up arrow before it when the commit is on no remote yet. The cell
/// keeps [`HASH_WIDTH`] either way: the arrow takes room from the hash's own.
fn hash_cell<'a>(hash: &str, unpushed: bool, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    if !unpushed {
        return plain_cell(hash, HASH_WIDTH, tokens, reading);
    }
    let arrow_width = reading.size;
    let room = HASH_WIDTH - CELL_PAD * 2.0 - arrow_width;
    container(
        row![
            container(
                text("\u{2191}")
                    .font(reading.font)
                    .size(reading.size)
                    .wrapping(Wrapping::None)
                    .style(move |_| text::Style { color: Some(tokens.accent_primary) }),
            )
            .width(Length::Fixed(arrow_width)),
            text(crate::text_measure::ellipsize(hash, reading.font, reading.size, room))
                .font(reading.font)
                .size(reading.size)
                .wrapping(Wrapping::None)
                .style(move |_| text::Style { color: Some(tokens.text_primary) }),
        ]
        .align_y(Alignment::Center),
    )
    .width(Length::Fixed(HASH_WIDTH))
    .height(Length::Fill)
    .padding([0.0, CELL_PAD])
    .align_y(Alignment::Center)
    .clip(true)
    .into()
}

/// The author with Zed's avatar mark before the name.
fn author_cell<'a>(author: &str, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    let mark = (reading.size - 1.0).max(10.0);
    let room = AUTHOR_WIDTH - CELL_PAD * 2.0 - mark - AUTHOR_MARK_GAP;
    container(
        row![
            glyph(Icon::Account, mark, tokens.text_muted, &reading),
            text(crate::text_measure::ellipsize(author, reading.font, reading.size, room))
                .font(reading.font)
                .size(reading.size)
                .wrapping(Wrapping::None)
                .style(move |_| text::Style { color: Some(tokens.text_primary) }),
        ]
        .spacing(AUTHOR_MARK_GAP)
        .align_y(Alignment::Center),
    )
    .width(Length::Fixed(AUTHOR_WIDTH))
    .height(Length::Fill)
    .padding([0.0, CELL_PAD])
    .align_y(Alignment::Center)
    .clip(true)
    .into()
}

/// Between the avatar mark and the author's name.
const AUTHOR_MARK_GAP: f32 = 6.0;

/// Refs | Graph | Description | Date | Author | Commit, divided as in Zed. The refs and the
/// graph meet at a divider the dotted line runs across.
#[allow(clippy::too_many_arguments)]
fn history_row<'a>(
    refs: Element<'a, Message>,
    graph: Element<'a, Message>,
    description: Element<'a, Message>,
    author: &str,
    when: &str,
    hash: &str,
    unpushed: bool,
    _muted: bool,
    height: Length,
    columns: Columns,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let mut line = row![refs, divider(tokens), graph, divider(tokens), description];
    if columns.when {
        line = line.push(divider(tokens)).push(plain_cell(when, WHEN_WIDTH, tokens, reading));
    }
    if columns.author {
        line = line.push(divider(tokens)).push(author_cell(author, tokens, reading));
    }
    if columns.hash {
        line = line.push(divider(tokens)).push(hash_cell(hash, unpushed, tokens, reading));
    }
    line.height(height).align_y(Alignment::Center).width(Length::Fill).into()
}

/// Mirrors [`history_row`] cell for cell, so every title sits over its column.
fn column_headers<'a>(graph_w: f32, columns: Columns, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    let mut line = row![
        header_cell("Refs", Length::Fixed(columns.refs_width), tokens, reading),
        divider(tokens),
        header_cell("Graph", Length::Fixed(graph_w), tokens, reading),
        divider(tokens),
        header_cell("Description", Length::Fill, tokens, reading),
    ];
    if columns.when {
        line = line.push(divider(tokens)).push(header_cell("Date (UTC)", Length::Fixed(WHEN_WIDTH), tokens, reading));
    }
    if columns.author {
        line = line.push(divider(tokens)).push(header_cell("Author", Length::Fixed(AUTHOR_WIDTH), tokens, reading));
    }
    if columns.hash {
        line = line.push(divider(tokens)).push(header_cell("Commit", Length::Fixed(HASH_WIDTH), tokens, reading));
    }
    line.height(Length::Fixed(HEADER_HEIGHT)).align_y(Alignment::Center).width(Length::Fill).into()
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
            .size((reading.size - 1.0).max(HEADER_SIZE))
            .wrapping(Wrapping::None)
            .style(move |_| text::Style {
                color: Some(tokens.text_secondary),
            }),
    )
    .width(width)
    .height(Length::Fixed(HEADER_HEIGHT))
    .padding([0.0, CELL_PAD])
    .align_y(Alignment::Center)
    .clip(true)
    .into()
}

fn graph_cell_empty(graph_w: f32) -> Element<'static, Message> {
    container(text(""))
        .width(Length::Fixed(graph_w))
        .height(Length::Fixed(ROW_HEIGHT))
        .into()
}

/// The subject, on one line, ended with "…" when it does not fit; whole in the Inspector.
fn description_cell<'a>(subject: &str, room: f32, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    container(
        text(crate::text_measure::ellipsize(subject, reading.font, reading.size, room))
            .font(reading.font)
            .size(reading.size)
            .wrapping(Wrapping::None)
            .style(move |_| text::Style { color: Some(tokens.text_primary) }),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .padding([0.0, CELL_PAD])
    .align_y(Alignment::Center)
    .clip(true)
    .into()
}

/// As in Zed: the commit's first ref, ended with "…" when long, then "+n" for the others, then
/// the dotted line out to the graph. Hovering lists every ref by its full name; the pill opens
/// its own menu and "+n" the commit's, which can copy every name.
#[allow(clippy::too_many_arguments)]
fn refs_cell<'a>(
    app: &GitApp,
    row: &crate::history_index::IndexRow,
    lane: Color,
    leader: Option<Color>,
    worktree_inks: &WorktreeInks,
    width: f32,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let (Some(leader), Some(first)) = (leader, row.pills.first()) else {
        return Space::new(Length::Fixed(width), Length::Fixed(ROW_HEIGHT)).into();
    };
    let others = row.pills.len() - 1;
    let more = if others > 0 { more_width(others, reading) + PILL_GAP } else { 0.0 };
    let pill_room = width - CELL_PAD - LEADER_MIN - more;
    // A branch checked out in a worktree wears that worktree's colour, as everywhere else.
    let ink = first.worktree.as_deref().map_or(lane, |path| worktree_inks.colour(path));
    let mut line = row![mouse_area(ref_pill(first, ink, pill_room, tokens, reading))
        .on_right_press(Message::OpenContextMenu { target: pill_target(first, &row.commit_id) })]
    .spacing(PILL_GAP)
    .align_y(Alignment::Center);
    if others > 0 {
        let target = ContextTarget::Commit { id: row.commit_id.clone(), subject: row.subject.clone(), parents: row.parents };
        line = line.push(mouse_area(more_chip(others, tokens, reading)).on_right_press(Message::OpenContextMenu { target }));
    }
    line = line.push(canvas(LeaderLine { colour: leader }).width(Length::Fill).height(Length::Fixed(ROW_HEIGHT)));
    let folder_of = |path: &str| app.worktree_folder(path);
    let names = pill_tooltip(&row.pills, &app.worktree_catalogue, &folder_of);
    hover_tip(
        container(line)
            .width(Length::Fixed(width))
            .height(Length::Fixed(ROW_HEIGHT))
            .padding(iced::Padding { left: CELL_PAD, ..iced::Padding::ZERO })
            .align_y(Alignment::Center)
            .clip(true)
            .into(),
        names,
        tokens,
        reading,
    )
}

/// The font size inside pills.
fn pill_size(reading: Reading) -> f32 {
    (reading.size - 2.0).max(10.0)
}

/// How wide the "+n" chip is.
fn more_width(others: usize, reading: Reading) -> f32 {
    crate::text_measure::width(&format!("+{others}"), reading.mono, pill_size(reading)) + PILL_PAD_X * 2.0 + 2.0
}

/// The "+1" after the first ref.
fn more_chip<'a>(others: usize, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    container(
        text(format!("+{others}"))
            .font(reading.mono)
            .size(pill_size(reading))
            .wrapping(Wrapping::None)
            .style(move |_| text::Style { color: Some(tokens.text_secondary) }),
    )
    .height(Length::Fixed(PILL_HEIGHT))
    .align_y(Alignment::Center)
    .padding([PILL_PAD_Y, PILL_PAD_X])
    .style(move |_theme: &Theme| container::Style {
        background: Some(Background::Color(tokens.bg_raised)),
        text_color: Some(tokens.text_secondary),
        border: Border { color: tokens.border, width: 1.0, radius: PILL_RADIUS.into() },
        shadow: Shadow::default(),
    })
    .into()
}

/// The first line of a commit message.
fn subject_of(message: &str) -> &str {
    message.lines().next().unwrap_or_default()
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

/// A worktree's working changes, in the Refs column: the outlined pill with the edit mark, in
/// the worktree's colour, so it cannot pass for a branch or tag. Cut at the column's edge if
/// it must be; the row's hover text names the worktree and its folder whole.
fn wip_refs_cell<'a>(wip: &WipRow, worktree: Color, width: f32, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    let size = pill_size(reading);
    let text_room = width - CELL_PAD * 2.0 - size - PILL_MARK_GAP - PILL_PAD_X * 2.0 - 2.0;
    let pill = container(
        row![
            glyph(Icon::Edit, size, worktree, &reading),
            text(crate::text_measure::ellipsize(&wip.label, reading.mono, size, text_room))
                .font(reading.mono)
                .size(size)
                .wrapping(Wrapping::None)
                .style(move |_| text::Style { color: Some(worktree) }),
        ]
        .spacing(PILL_MARK_GAP)
        .align_y(Alignment::Center),
    )
    .height(Length::Fixed(PILL_HEIGHT))
    .align_y(Alignment::Center)
    .padding([PILL_PAD_Y, PILL_PAD_X])
    .style(move |_theme: &Theme| container::Style {
        background: Some(Background::Color(tokens.bg_raised)),
        text_color: Some(worktree),
        border: Border { color: worktree, width: 1.0, radius: PILL_RADIUS.into() },
        shadow: Shadow::default(),
    });
    container(pill)
        .width(Length::Fixed(width))
        .height(Length::Fixed(ROW_HEIGHT))
        .padding(iced::Padding { left: CELL_PAD, ..iced::Padding::ZERO })
        .align_y(Alignment::Center)
        .clip(true)
        .into()
}

/// "Uncommitted changes · 78 files · active", in the Description column of a WIP row.
fn wip_summary<'a>(wip: &WipRow, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    let files = if wip.changes == 1 { "1 file".to_string() } else { format!("{} files", wip.changes) };
    let active = if wip.active { " · active" } else { "" };
    container(
        text(format!("Uncommitted changes · {files}{active}"))
            .font(reading.font)
            .size(reading.size)
            .wrapping(Wrapping::None)
            .style(move |_| text::Style { color: Some(tokens.text_muted) }),
    )
    .width(Length::Fill)
    .height(Length::Fixed(ROW_HEIGHT))
    .padding([0.0, CELL_PAD])
    .align_y(Alignment::Center)
    .clip(true)
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

/// A ref at a commit, in the colour of the line it sits on, so a coloured line in the graph
/// can be told by name: the blue line is the branch whose pill is blue. A branch checked out
/// in a worktree is in that worktree's colour, and so is its line below. The shape says
/// the kind: filled for the checkout, outlined for a worktree or branch, faint for a remote
/// (with its cloud mark), neutral for a tag.
/// The menu a pill opens: the branch's, the remote branch's or the tag's.
fn pill_target(pill: &RefPill, commit_id: &str) -> ContextTarget {
    let (name, tip) = (pill.name.clone(), commit_id.to_string());
    match pill.kind {
        RefPillKind::Head | RefPillKind::Worktree | RefPillKind::Local => ContextTarget::LocalBranch { name, tip },
        RefPillKind::Remote => ContextTarget::RemoteBranch { name, tip },
        RefPillKind::Tag => ContextTarget::Tag { name, commit_id: tip },
    }
}

/// One ref, as Zed draws it: outlined in its line's colour on a dark fill, its name in
/// monospace, ended with "…" when it does not fit `room`. The checked-out branch reads
/// "✓ HEAD → name"; a worktree keeps its own mark, a tag its tag.
fn ref_pill<'a>(pill: &RefPill, lane: Color, room: f32, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    let size = pill_size(reading);
    let (mark, label, ink) = match pill.kind {
        RefPillKind::Head => (Icon::Check, format!("HEAD → {}", pill.label), lane),
        RefPillKind::Worktree => (Icon::Worktree, pill.label.clone(), tokens.text_primary),
        RefPillKind::Tag => (Icon::Tag, pill.label.clone(), tokens.text_primary),
        RefPillKind::Local | RefPillKind::Remote => (Icon::Branch, pill.label.clone(), tokens.text_primary),
    };
    let text_room = room - size - PILL_MARK_GAP - PILL_PAD_X * 2.0 - 2.0;
    container(
        row![
            glyph(mark, size, lane, &reading),
            text(crate::text_measure::ellipsize(&label, reading.mono, size, text_room))
                .font(reading.mono)
                .size(size)
                .wrapping(Wrapping::None)
                .style(move |_| text::Style { color: Some(ink) }),
        ]
        .spacing(PILL_MARK_GAP)
        .align_y(Alignment::Center),
    )
    .height(Length::Fixed(PILL_HEIGHT))
    .align_y(Alignment::Center)
    .padding([PILL_PAD_Y, PILL_PAD_X])
    .style(move |_theme: &Theme| container::Style {
        background: Some(Background::Color(tokens.bg_raised)),
        text_color: Some(ink),
        border: Border { color: lane, width: 1.0, radius: PILL_RADIUS.into() },
        shadow: Shadow::default(),
    })
    .into()
}

/// Between a pill's mark and its name.
const PILL_MARK_GAP: f32 = 4.0;

fn node_mark_for(pills: &[RefPill]) -> NodeMark {
    match primary_node_kind(pills) {
        Some(RefPillKind::Remote) => NodeMark::Remote,
        Some(RefPillKind::Worktree) => NodeMark::Worktree,
        Some(RefPillKind::Head) | Some(RefPillKind::Local) => NodeMark::Local,
        Some(RefPillKind::Tag) | None => NodeMark::Plain,
    }
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
        RefPill { label: name.into(), name: name.into(), kind: RefPillKind::Worktree, ahead: 0, behind: 0, worktree: None }
    }

    #[test]
    fn a_pill_opens_the_menu_of_the_ref_it_names() {
        let at = "abcdef0";
        let mut pill = pill("feature/x");
        assert_eq!(pill_target(&pill, at), ContextTarget::LocalBranch { name: "feature/x".into(), tip: at.into() });
        pill.kind = RefPillKind::Remote;
        pill.name = "origin/feature/x".into();
        assert_eq!(pill_target(&pill, at), ContextTarget::RemoteBranch { name: "origin/feature/x".into(), tip: at.into() });
        pill.kind = RefPillKind::Tag;
        pill.name = "v1.2".into();
        assert_eq!(pill_target(&pill, at), ContextTarget::Tag { name: "v1.2".into(), commit_id: at.into() });
    }

    #[test]
    fn rows_of_unpushed_commits_are_marked_when_the_index_is_built() {
        use core_domain::ViewEvent;
        let mut app = GitApp::with_repo(std::sync::Arc::new(test_support::FakeGitRepository::ok()));
        let key = app.active_key();
        let commit = |id: &str, parent: Option<&str>| Commit {
            id: id.into(),
            parent_ids: parent.map(str::to_string).into_iter().collect(),
            author: "a".into(),
            message: "m".into(),
            timestamp: 0,
        };
        let commits = vec![commit("abc1234", Some("def5678")), commit("def5678", None)];
        let _ = app.update(Message::RepositoryLoaded(
            key,
            vec![
                ViewEvent::LogUpdated(commits),
                ViewEvent::UnpushedUpdated(vec!["abc1234".into()]),
            ],
        ));
        let session = app.session();
        let index_key = index_key(&app, session, 1_200.0, reading());

        let index = build_index(&app, session, index_key, 1_200.0, reading());

        let marked: Vec<(&str, bool)> = index.rows.iter().map(|row| (row.commit_id.as_str(), row.unpushed)).collect();
        assert_eq!(marked, vec![("abc1234", true), ("def5678", false)]);
    }

    #[test]
    fn the_count_after_the_first_ref_grows_with_its_digits() {
        assert!(more_width(12, reading()) > more_width(3, reading()));
    }
}
