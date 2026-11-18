//! The history list's row index: everything needed to draw any row, worked out once per change
//! instead of on every redraw.
//!
//! iced rebuilds the view on every message, mouse moves included. Filtering, searching, laying
//! out lanes, finding each commit's refs and measuring every row on each of those rebuilds made
//! the window slow once the history grew. The index holds that work's result, keyed by what it
//! depends on; a redraw only finds the rows in view (a binary search over row tops) and builds
//! those. No widget outlives its frame, so memory holds this plain data and nothing else.

use crate::commit_graph::{GraphRow, RowInks};
use crate::ref_pills::RefPill;

/// The divider between two columns.
pub(crate) const COL_GAP: f32 = 1.0;
/// Seven hex digits.
pub(crate) const HASH_WIDTH: f32 = 76.0;
/// Fits "17 Nov 2026 11:03".
pub(crate) const WHEN_WIDTH: f32 = 128.0;
pub(crate) const AUTHOR_WIDTH: f32 = 110.0;
/// The narrowest the Description column may get before a column steps aside.
pub(crate) const MIN_DESCRIPTION_WIDTH: f32 = 380.0;
/// The Refs column, left of the graph, and the narrowest it gets when room is short.
pub(crate) const REFS_WIDTH: f32 = 160.0;
pub(crate) const MIN_REFS_WIDTH: f32 = 110.0;

/// What an index was built from. Any difference means rebuilding it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IndexKey {
    /// Bumped whenever the tab's commits, branches, tags or status change.
    pub(crate) generation: u64,
    pub(crate) width_px: u32,
    pub(crate) size_tenths: u32,
    pub(crate) query: String,
    pub(crate) author: Option<String>,
    pub(crate) branch_filter: Option<String>,
    pub(crate) focus: bool,
    /// Worktrees decide which pills read as worktrees, and their colours: see
    /// [`crate::worktree_ink::catalogue_fingerprint`].
    pub(crate) worktrees: u64,
    /// The checked-out branch's counts, drawn on its pill.
    pub(crate) head_counts: Option<(usize, usize)>,
}

/// One commit row, ready to draw.
#[derive(Debug, Clone)]
pub(crate) struct IndexRow {
    pub(crate) commit_id: String,
    pub(crate) subject: String,
    pub(crate) author: String,
    pub(crate) timestamp: i64,
    pub(crate) merge: bool,
    pub(crate) graph: GraphRow,
    /// Worktree colours on this row's dot and lines.
    pub(crate) inks: RowInks,
    pub(crate) pills: Vec<RefPill>,
    /// Height in px: every row is one line, [`crate::commit_graph::ROW_HEIGHT`].
    pub(crate) height: f32,
    /// Distance from the first commit row's top, in px.
    pub(crate) top: f32,
}

#[derive(Debug, Clone)]
pub(crate) struct HistoryIndex {
    pub(crate) key: IndexKey,
    pub(crate) rows: Vec<IndexRow>,
    /// Height of every row together.
    pub(crate) total: f32,
    /// The branch the log is filtered to, if any, and whether its tip is not loaded.
    pub(crate) filter: Option<String>,
    pub(crate) filter_unloaded: bool,
    /// Commits the filter kept (before search).
    pub(crate) shown: usize,
    /// A search or author filter picked rows out, so lanes do not join them up.
    pub(crate) narrowing: bool,
    pub(crate) columns: Columns,
    /// The lane column width the rows were measured against (the widest in the index), so a
    /// narrower lane column on screen only ever leaves the text more room than measured.
    pub(crate) graph_width: f32,
}

impl HistoryIndex {
    /// The `first..last` rows touching `top..bottom` (px from the first commit row).
    pub(crate) fn window(&self, top: f32, bottom: f32) -> (usize, usize) {
        let first = self.rows.partition_point(|row| row.top + row.height < top);
        let last = self.rows.partition_point(|row| row.top < bottom).max(first);
        (first, last)
    }
}

/// How wide the Refs column is, and which of Author, Date and Hash fit beside a readable
/// Description. When room is short they give way in order of least use: Hash, then Author,
/// then the Refs column narrows, then Date. All of them are in the Inspector.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Columns {
    pub(crate) refs_width: f32,
    pub(crate) author: bool,
    pub(crate) when: bool,
    pub(crate) hash: bool,
}

impl Columns {
    /// For `space` px beside the lane column.
    pub(crate) fn for_width(space: f32) -> Self {
        let mut columns = Columns { refs_width: REFS_WIDTH, author: true, when: true, hash: true };
        let drops: [fn(&mut Columns); 4] =
            [|c| c.hash = false, |c| c.author = false, |c| c.refs_width = MIN_REFS_WIDTH, |c| c.when = false];
        for drop in drops {
            if columns.description_width(space) >= MIN_DESCRIPTION_WIDTH {
                break;
            }
            drop(&mut columns);
        }
        columns
    }

    /// What Description gets of `space` once the shown columns and gaps are taken.
    pub(crate) fn description_width(self, space: f32) -> f32 {
        let mut used = self.refs_width + COL_GAP * 2.0;
        for (shown, width) in [(self.author, AUTHOR_WIDTH), (self.when, WHEN_WIDTH), (self.hash, HASH_WIDTH)] {
            if shown {
                used += width + COL_GAP;
            }
        }
        (space - used).max(80.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(top: f32, height: f32) -> IndexRow {
        IndexRow {
            commit_id: String::new(),
            subject: String::new(),
            author: String::new(),
            timestamp: 0,
            merge: false,
            graph: GraphRow { lane: 0, incoming: vec![], passing: vec![], outgoing: vec![] },
            inks: RowInks::default(),
            pills: vec![],
            height,
            top,
        }
    }

    #[test]
    fn the_window_is_the_rows_touching_the_range() {
        let index = HistoryIndex {
            key: IndexKey {
                generation: 0,
                width_px: 0,
                size_tenths: 0,
                query: String::new(),
                author: None,
                branch_filter: None,
                focus: false,
                worktrees: 0,
                head_counts: None,
            },
            rows: vec![row(0.0, 24.0), row(24.0, 48.0), row(72.0, 24.0), row(96.0, 24.0)],
            total: 120.0,
            filter: None,
            filter_unloaded: false,
            shown: 4,
            narrowing: false,
            columns: Columns { refs_width: REFS_WIDTH, author: true, when: true, hash: true },
            graph_width: 24.0,
        };
        assert_eq!(index.window(0.0, 30.0), (0, 2));
        assert_eq!(index.window(50.0, 80.0), (1, 3), "a tall row is kept while any of it shows");
        assert_eq!(index.window(500.0, 900.0), (4, 4));
    }

    #[test]
    fn columns_step_aside_hash_then_author_then_the_refs_narrow_then_date() {
        let all = |refs_width, author, when, hash| Columns { refs_width, author, when, hash };
        assert_eq!(Columns::for_width(2_000.0), all(REFS_WIDTH, true, true, true));
        let fixed = |refs: f32, widths: &[f32]| MIN_DESCRIPTION_WIDTH + refs + COL_GAP * 2.0 + widths.iter().map(|w| w + COL_GAP).sum::<f32>() + 5.0;
        assert_eq!(Columns::for_width(fixed(REFS_WIDTH, &[AUTHOR_WIDTH, WHEN_WIDTH])), all(REFS_WIDTH, true, true, false));
        assert_eq!(Columns::for_width(fixed(REFS_WIDTH, &[WHEN_WIDTH])), all(REFS_WIDTH, false, true, false));
        assert_eq!(Columns::for_width(fixed(MIN_REFS_WIDTH, &[WHEN_WIDTH])), all(MIN_REFS_WIDTH, false, true, false));
        assert_eq!(Columns::for_width(200.0), all(MIN_REFS_WIDTH, false, false, false));
    }
}
