//! One open worktree tab: its adapter, [`AppState`], and UI bag.
//!
//! `GitApp` owns a `Vec<WorktreeSession>` and an active index. Everything
//! repository-scoped lives here so switching tabs is a pointer move, never a reload.

use std::collections::HashSet;

use core_domain::{AppSettings, AppState, Branch, FileDisposition, Hunk, Patch, ViewEvent};
use core_domain::WorktreeTabRetention;
use iced::widget::text_editor;

use crate::changes::{ChangeFilter, MarkedFile};
use crate::jobs::{PatchQuery, SharedRepository, WorktreeEdit};
use crate::selection::{visible_selection, Selection};

/// The message a commit records: the summary, then one blank line and the body when the body
/// has any text. A blank summary gives an empty message, whatever the body says: a body alone
/// is never a commit. Every way of committing builds its message here.
pub(crate) fn full_commit_message(summary: &str, body: &str) -> String {
    let summary = summary.trim();
    if summary.is_empty() {
        String::new()
    } else if body.trim().is_empty() {
        summary.to_string()
    } else {
        format!("{summary}\n\n{}", body.trim_end())
    }
}

/// Split a message into summary and body: the first line, then what follows once blank lines
/// are skipped.
pub(crate) fn split_summary_and_body(message: &str) -> (String, String) {
    let message = message.trim_start();
    let (summary, rest) = message.split_once('\n').unwrap_or((message, ""));
    let body = rest.trim_start_matches(['\r', '\n']).trim_end();
    (summary.trim_end().to_string(), body.to_string())
}

/// Diff lines drawn per "Show more" step. One widget per line, so this bounds layout cost.
pub(crate) const DIFF_LINES_PER_PAGE: usize = 400;

/// True when two worktree paths name the same folder. Git prints `D:/a/b`, the folder dialog
/// gives `D:\a\b`, and Windows paths ignore case, so compare a normalised form.
pub(crate) fn same_path(a: &str, b: &str) -> bool {
    fn normal(path: &str) -> String {
        let flat = path.trim().replace('\\', "/");
        let trimmed = flat.trim_end_matches('/');
        if cfg!(windows) {
            trimmed.to_lowercase()
        } else {
            trimmed.to_string()
        }
    }
    normal(a) == normal(b)
}

/// Next session number. Process-wide, because opening a repository rebuilds `GitApp` and a
/// counter kept there would start again and hand an old number to a new session.
static NEXT_SESSION_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Names one session for its whole life. Never reused, so a result for a session that has
/// closed — even one reopened on the same folder — cannot reach its successor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct SessionId(u64);

impl SessionId {
    fn next() -> Self {
        Self(NEXT_SESSION_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
    }

    /// Matches no session: for tests and for work not started by one.
    #[cfg(test)]
    pub(crate) const NONE: Self = Self(0);
}

/// What a background job remembers about the session that started it: who (to route the
/// result) and where (for the activity log and messages).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionKey {
    pub(crate) id: SessionId,
    pub(crate) path: String,
}

/// Last folder name of a path, for tab and explorer labels. Falls back to the whole path.
pub(crate) fn basename(path: &str) -> &str {
    let trimmed = path.trim_end_matches(['/', '\\']);
    trimmed
        .rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(path)
}

/// Which accordion section the user can collapse in the Explorer rail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExplorerSection {
    Local,
    Remote,
    Worktrees,
    Stashes,
    Tags,
}

/// Collapsed state for each Explorer accordion section.
///
/// All fields default to `false` (open), matching the spec requirement that every
/// section starts visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct ExplorerCollapsed {
    pub local: bool,
    pub remote: bool,
    pub worktrees: bool,
    pub stashes: bool,
    pub tags: bool,
}


/// Where each commit row of a history list sits, in scroll px from the top of the list.
#[derive(Debug, Clone, Default)]
pub(crate) struct HistoryLayout {
    /// Commit id and top of each row, in drawing order (tops increase).
    pub(crate) rows: Vec<(String, f32)>,
}

impl HistoryLayout {
    /// The row under `offset`: its commit and top.
    pub(crate) fn row_at(&self, offset: f32) -> Option<(&str, f32)> {
        let after = self.rows.partition_point(|(_, top)| *top <= offset);
        after.checked_sub(1).map(|at| (self.rows[at].0.as_str(), self.rows[at].1))
    }

    pub(crate) fn top_of(&self, commit: &str) -> Option<f32> {
        self.rows.iter().find(|(id, _)| id == commit).map(|(_, top)| *top)
    }
}

/// Everything bound to one open worktree path.
pub(crate) struct WorktreeSession {
    /// This session's identity; background results are routed by it, never by path alone.
    pub(crate) id: SessionId,
    /// Absolute working directory this session opened.
    pub(crate) path: String,
    pub(crate) adapter: SharedRepository,
    pub(crate) state: AppState,
    pub(crate) selection: Option<Selection>,
    /// The commit's summary line. The rest of its message is `commit_body`.
    pub(crate) commit_message: String,
    pub(crate) commit_body: text_editor::Content,
    /// The Amend box is ticked, holding the HEAD id it was ticked on: committing replaces that
    /// commit instead of adding one, and is refused if HEAD is no longer it.
    pub(crate) amending: Option<String>,
    pub(crate) marked_files: Vec<MarkedFile>,
    /// What the changes list is narrowed to. Kept across refreshes.
    pub(crate) change_filter: ChangeFilter,
    /// Folder prefixes collapsed in Tree mode, keyed with their disposition.
    pub(crate) collapsed_dirs: HashSet<(FileDisposition, String)>,
    /// Staged / Unstaged / Untracked headers the user has folded shut.
    pub(crate) collapsed_sections: HashSet<FileDisposition>,
    pub(crate) patches: Vec<Patch>,
    pub(crate) patch_error: Option<String>,
    pub(crate) patch_pending: bool,
    pub(crate) patch_generation: u64,
    /// When set, the Diff pane shows only this path (from a commit/tag file click).
    pub(crate) focused_diff_path: Option<String>,
    /// How many diff lines the diff pane draws before offering "Show more".
    pub(crate) diff_lines_limit: usize,
    /// After a successful create, check out this local branch name in this session.
    pub(crate) pending_checkout: Option<String>,
    /// When true, the history graph shows only commits reachable from this session's HEAD.
    pub(crate) focus_current_branch: bool,
    /// Which Explorer accordion sections are currently collapsed.
    pub(crate) explorer_collapsed: ExplorerCollapsed,
    /// Branch-tree folders the user opened or closed, relative to their default
    /// (local folders and each remote start open; folders inside a remote start closed).
    pub(crate) ref_folders_toggled: HashSet<String>,
    /// Log filtered to this branch (local or remote name) after a click in the branch tree.
    /// `focus_current_branch` takes precedence.
    pub(crate) branch_filter: Option<String>,
    /// Log narrowed to commits by this author (the filter bar's User ▾).
    pub(crate) author_filter: Option<String>,
    /// The difference last jumped to in the Diff pane (‹ › arrows).
    pub(crate) diff_change: Option<usize>,
    /// Where a Shift-click range in the working tree starts: the last plain or Ctrl click.
    pub(crate) selection_anchor: Option<MarkedFile>,
    /// Commits asked for in the history: one page to start, one more each time the list is
    /// scrolled near its end. Kept across refreshes so a reload never cuts the history short.
    pub(crate) history_limit: usize,
    /// A next page of history is being read.
    pub(crate) history_more_pending: bool,
    /// The walk older pages come from. Replaced by [`Self::restart_history`] when the log is
    /// reloaded from the top, so the next page continues from what is then loaded.
    pub(crate) history_cursor: crate::jobs::HistoryCursor,
    /// When the page under way was asked for, for the perf trace.
    pub(crate) history_asked_at: Option<std::time::Instant>,
    /// The user reached the end of what has arrived while more was on its way: read bigger
    /// chunks until the history catches up.
    pub(crate) history_urgent: bool,
    /// Bumped whenever this tab's commits, branches, tags or status change, so the history
    /// row index knows to rebuild (and only then).
    pub(crate) history_generation: u64,
    /// The history list's row index, built from the state above. See `history_index`.
    pub(crate) history_index: std::cell::RefCell<Option<crate::history_index::HistoryIndex>>,
    /// Where the history list was scrolled to, in px, and the commit at the top of the view
    /// then. The commit is the anchor: when newer commits arrive above it, returning scrolls
    /// to it rather than to a pixel offset that now shows something else.
    pub(crate) history_offset: f32,
    pub(crate) history_anchor: Option<String>,
    /// How far below the anchor commit's top the view was.
    pub(crate) history_anchor_within: f32,
    /// Where each commit row sat the last time this tab's history was drawn. Rows are as tall
    /// as their text, so this, not a fixed row height, maps a scroll offset to a commit.
    pub(crate) history_layout: std::cell::RefCell<HistoryLayout>,
    /// Where the Diff pane was scrolled to, in px.
    pub(crate) diff_offset: f32,
    /// The text selected in the Diff pane, or its bare caret.
    pub(crate) diff_selection: Option<crate::diff_select::DiffSelection>,
    /// How far the Diff pane's text is scrolled sideways, in characters, and how many fit.
    pub(crate) diff_columns: crate::diff_select::Columns,
    /// Put the Diff pane back at `diff_offset` once the diff loads (after a return to this tab).
    pub(crate) diff_restore_pending: bool,
}

impl WorktreeSession {
    /// Build a fresh session for `path`, seeding its state with the current settings.
    pub(crate) fn new(path: String, adapter: SharedRepository, settings: AppSettings) -> Self {
        let mut state = AppState::new();
        state.apply_event(ViewEvent::SettingsUpdated(settings));
        Self {
            id: SessionId::next(),
            path,
            adapter,
            state,
            selection: None,
            commit_message: String::new(),
            commit_body: text_editor::Content::new(),
            amending: None,
            marked_files: Vec::new(),
            change_filter: ChangeFilter::default(),
            collapsed_dirs: HashSet::new(),
            collapsed_sections: HashSet::new(),
            patches: Vec::new(),
            patch_error: None,
            patch_pending: false,
            patch_generation: 0,
            focused_diff_path: None,
            diff_lines_limit: DIFF_LINES_PER_PAGE,
            pending_checkout: None,
            focus_current_branch: false,
            explorer_collapsed: ExplorerCollapsed::default(),
            ref_folders_toggled: HashSet::new(),
            branch_filter: None,
            author_filter: None,
            diff_change: None,
            selection_anchor: None,
            history_limit: crate::jobs::COMMIT_PAGE_SIZE,
            history_more_pending: false,
            history_cursor: crate::jobs::HistoryCursor::default(),
            history_asked_at: None,
            history_urgent: false,
            history_generation: 0,
            history_index: std::cell::RefCell::new(None),
            history_offset: 0.0,
            history_anchor: None,
            history_anchor_within: 0.0,
            history_layout: std::cell::RefCell::new(HistoryLayout::default()),
            diff_offset: 0.0,
            diff_selection: None,
            diff_columns: Default::default(),
            diff_restore_pending: false,
        }
    }

    /// Outgoing and incoming counts for `branch`. The checked-out branch reads the same
    /// tracking state as the header (`git rev-list @{upstream}...HEAD`), so the two can never
    /// disagree; other branches read git's `for-each-ref` tracking.
    pub(crate) fn branch_counts(&self, branch: &core_domain::Branch) -> (usize, usize) {
        if branch.is_head && !branch.is_remote {
            if let Some(tracking) = self.state.current_status.as_ref().and_then(|status| status.tracking.as_ref()) {
                return (tracking.ahead, tracking.behind);
            }
        }
        (branch.ahead, branch.behind)
    }

    /// Remember a history scroll: the offset and the commit then at the top of the view.
    pub(crate) fn note_history_scroll(&mut self, offset: f32) {
        self.history_offset = offset.max(0.0);
        let layout = self.history_layout.borrow();
        if let Some((id, top)) = layout.row_at(self.history_offset) {
            self.history_anchor = Some(id.to_string());
            self.history_anchor_within = self.history_offset - top;
            return;
        }
        drop(layout);
        // Not drawn yet: fall back to whole rows of the base height.
        let row_height = crate::commit_graph::ROW_HEIGHT;
        let row = (self.history_offset / row_height) as usize;
        self.history_anchor = self.state.dag_nodes.get(row).map(|node| node.commit_id.clone());
        self.history_anchor_within = self.history_offset % row_height;
    }

    /// The offset that puts the anchor commit back at the top of the view, keeping how far
    /// into its row the view was. Falls back to the plain offset when the anchor is gone
    /// (rewritten history, a reset) or was never set.
    pub(crate) fn anchored_history_offset(&self) -> f32 {
        let Some(anchor) = self.history_anchor.as_ref() else {
            return self.history_offset;
        };
        if let Some(top) = self.history_layout.borrow().top_of(anchor) {
            return top + self.history_anchor_within;
        }
        let row_height = crate::commit_graph::ROW_HEIGHT;
        self.state
            .dag_nodes
            .iter()
            .position(|node| &node.commit_id == anchor)
            .map(|row| row as f32 * row_height + self.history_anchor_within)
            .unwrap_or(self.history_offset)
    }

    /// True when the last read filled the page, so older commits may exist.
    pub(crate) fn history_may_have_more(&self) -> bool {
        self.state.commits.len() >= self.history_limit
    }

    /// Forget the walk. The log was reloaded from the top, so the next page starts a new walk
    /// that leaves out everything loaded by then. A page still on its way from the old walk
    /// is harmless: what it brings is appended once and never twice.
    pub(crate) fn restart_history(&mut self) {
        self.history_cursor = crate::jobs::HistoryCursor::default();
    }

    /// Ids of every loaded commit, for a new walk to leave out.
    pub(crate) fn loaded_commit_ids(&self) -> Vec<String> {
        self.state.commits.iter().map(|commit| commit.id.clone()).collect()
    }

    /// Add the commits of `page` that are not loaded yet, in their order, after the loaded
    /// ones. True when something was added (the graph and the row index then rebuild).
    pub(crate) fn append_history(&mut self, page: Vec<core_domain::Commit>) -> bool {
        let known: HashSet<&str> = self.state.commits.iter().map(|commit| commit.id.as_str()).collect();
        let fresh: Vec<core_domain::Commit> = page.into_iter().filter(|commit| !known.contains(commit.id.as_str())).collect();
        if fresh.is_empty() {
            return false;
        }
        let mut all = std::mem::take(&mut self.state.commits);
        all.extend(fresh);
        self.state.apply_event(ViewEvent::LogUpdated(all));
        true
    }

    /// Identity plus path, for a job this session starts.
    pub(crate) fn key(&self) -> SessionKey {
        SessionKey {
            id: self.id,
            path: self.path.clone(),
        }
    }

    /// The query the diff pane should load for the current selection, or `None` when the
    /// selection draws no patch (a branch, remote, or non-file row).
    pub(crate) fn patch_query(&self) -> Option<PatchQuery> {
        match self.selection.clone() {
            Some(Selection::Commit { id } | Selection::Tag { commit_id: id, .. }) => {
                Some(PatchQuery::Commit(id))
            }
            Some(Selection::File { path, place }) => Some(PatchQuery::File { path, place }),
            // A stash is a commit; its first-parent diff is the stashed change.
            Some(Selection::Stash { index, message }) => self
                .state
                .stashes
                .iter()
                .find(|stash| stash.index == index && stash.message == message)
                .map(|stash| PatchQuery::Commit(stash.commit_id.clone())),
            _ => None,
        }
    }

    pub(crate) fn hunk_at(&self, path: &str, index: usize) -> Option<Hunk> {
        self.patches
            .iter()
            .find(|patch| patch.path == path)
            .and_then(|patch| patch.hunks.get(index))
            .cloned()
    }

    pub(crate) fn reconcile_selection(&mut self) {
        let Some(stored) = self.selection.clone() else {
            return;
        };
        let visible = visible_selection(
            Some(&stored),
            &self.state.branches,
            &self.state.commits,
            &self.state.stashes,
            &self.state.tags,
            self.state.current_status.as_ref(),
        );
        if matches!(
            stored,
            Selection::File { .. }
                | Selection::Commit { .. }
                | Selection::Stash { .. }
                | Selection::Tag { .. }
        ) && matches!(visible, Selection::Branch { .. })
        {
            self.selection = None;
            return;
        }
        self.selection = Some(visible);
    }

    /// Every changed file the filter shows.
    pub(crate) fn all_changed_files(&self) -> Vec<MarkedFile> {
        let Some(status) = &self.state.current_status else {
            return Vec::new();
        };
        let mut marks = Vec::new();
        for (files, place) in [
            (&status.staged_files, FileDisposition::Staged),
            (&status.unstaged_files, FileDisposition::Unstaged),
            (&status.untracked_files, FileDisposition::Untracked),
        ] {
            for path in files.iter().filter(|path| self.shows(place, path)) {
                marks.push(MarkedFile {
                    path: path.clone(),
                    place,
                });
            }
        }
        marks
    }

    /// Whether the changes filter shows `path` in `place`.
    fn shows(&self, place: FileDisposition, path: &str) -> bool {
        let Some(status) = self.state.current_status.as_ref() else {
            return true;
        };
        crate::changes::filter_matches(&self.change_filter, path, place, status.kinds.of(place, path))
    }

    /// Drops the marks, and the Shift anchor, that the filter now hides, so no action on the
    /// selection can reach a file the user cannot see.
    pub(crate) fn drop_hidden_marks(&mut self) {
        if !self.change_filter.is_active() {
            return;
        }
        let marks = std::mem::take(&mut self.marked_files);
        self.marked_files = marks.into_iter().filter(|mark| self.shows(mark.place, &mark.path)).collect();
        if self.selection_anchor.as_ref().is_some_and(|anchor| !self.shows(anchor.place, &anchor.path)) {
            self.selection_anchor = None;
        }
    }

    pub(crate) fn retain_marks(&mut self) {
        let Some(status) = &self.state.current_status else {
            self.marked_files.clear();
            return;
        };
        self.marked_files.retain(|mark| {
            let files = match mark.place {
                FileDisposition::Staged => &status.staged_files,
                FileDisposition::Unstaged => &status.unstaged_files,
                FileDisposition::Untracked => &status.untracked_files,
            };
            files.iter().any(|path| path == &mark.path)
        });
        // A refresh can turn a marked file into one the filter hides (Modified to Deleted).
        self.drop_hidden_marks();
    }

    pub(crate) fn shown_selection(&self) -> Selection {
        visible_selection(
            self.selection.as_ref(),
            &self.state.branches,
            &self.state.commits,
            &self.state.stashes,
            &self.state.tags,
            self.state.current_status.as_ref(),
        )
    }

    /// Whether a branch-tree folder is open: its default, flipped when the user toggled it.
    pub(crate) fn ref_folder_open(&self, key: &str) -> bool {
        // "remote/<name>" is one segment deep; anything deeper inside a remote starts closed.
        let default_open = !key.starts_with("remote/") || key.matches('/').count() <= 1;
        default_open != self.ref_folders_toggled.contains(key)
    }

    /// The diff file one step after (or before) the focused one, in patch order. With no file
    /// focused, forward is the first and back the last. `None` at either end.
    pub(crate) fn diff_step(&self, forward: bool) -> Option<String> {
        let paths: Vec<&str> = self.patches.iter().map(|patch| patch.path.as_str()).collect();
        let at = self
            .focused_diff_path
            .as_deref()
            .and_then(|focused| paths.iter().position(|path| *path == focused));
        let next = match (at, forward) {
            (None, true) => Some(0),
            (None, false) => paths.len().checked_sub(1),
            (Some(index), true) => Some(index + 1),
            (Some(index), false) => index.checked_sub(1),
        };
        next.and_then(|index| paths.get(index)).map(|path| (*path).to_string())
    }

    pub(crate) fn toggle_ref_folder(&mut self, key: &str) {
        if !self.ref_folders_toggled.remove(key) {
            self.ref_folders_toggled.insert(key.to_string());
        }
    }

    pub(crate) fn local_branch(&self, name: &str) -> Option<&Branch> {
        self.state
            .branches
            .iter()
            .find(|branch| !branch.is_remote && branch.name == name)
    }

    pub(crate) fn remote_branch(&self, name: &str) -> Option<&Branch> {
        self.state
            .branches
            .iter()
            .find(|branch| branch.is_remote && branch.name == name)
    }

    pub(crate) fn commit_loaded(&self, id: &str) -> bool {
        self.state.commits.iter().any(|commit| commit.id == id)
    }

    /// Flip the collapsed flag for one Explorer accordion section.
    pub(crate) fn toggle_explorer(&mut self, section: ExplorerSection) {
        match section {
            ExplorerSection::Local => {
                self.explorer_collapsed.local = !self.explorer_collapsed.local;
            }
            ExplorerSection::Remote => {
                self.explorer_collapsed.remote = !self.explorer_collapsed.remote;
            }
            ExplorerSection::Worktrees => {
                self.explorer_collapsed.worktrees = !self.explorer_collapsed.worktrees;
            }
            ExplorerSection::Stashes => {
                self.explorer_collapsed.stashes = !self.explorer_collapsed.stashes;
            }
            ExplorerSection::Tags => {
                self.explorer_collapsed.tags = !self.explorer_collapsed.tags;
            }
        }
    }

    /// The commit this tab's summary and body describe: a new commit, or, while Amend is
    /// ticked, a replacement for the HEAD the draft was prepared for (the one Amend was
    /// ticked on), never whatever HEAD has become since.
    pub(crate) fn commit_edit(&self) -> WorktreeEdit {
        let message = full_commit_message(&self.commit_message, &self.commit_body.text());
        if let Some(head) = &self.amending {
            WorktreeEdit::Amend {
                message,
                expected_head: head.clone(),
            }
        } else {
            WorktreeEdit::Commit(message)
        }
    }

    /// The HEAD commit as this tab last read it; `None` before the first commit or the first load.
    pub(crate) fn seen_head(&self) -> Option<&str> {
        self.state.current_status.as_ref()?.head_commit_id.as_deref()
    }

    /// The HEAD commit has more than one parent. False while it is not loaded.
    pub(crate) fn head_is_merge(&self) -> bool {
        let Some(head) = self.seen_head() else {
            return false;
        };
        // HEAD is among the newest commits, so this stops near the start of the list.
        self.state
            .commits
            .iter()
            .find(|commit| commit.id == head)
            .is_some_and(|commit| commit.parent_ids.len() > 1)
    }

    /// `head` is on a remote already, so rewriting it means the next push must force.
    pub(crate) fn head_is_on_a_remote(&self, head: &str) -> bool {
        // "The repository has a remote" is read from the remote-tracking branches until the
        // remotes themselves are loaded (ruling R9).
        let has_a_remote = self.state.branches.iter().any(|branch| branch.is_remote);
        has_a_remote && !self.state.unpushed.contains(head)
    }

    /// Empty the summary and the body.
    pub(crate) fn clear_commit_message(&mut self) {
        self.commit_message.clear();
        self.commit_body = text_editor::Content::new();
    }

    #[cfg(test)]
    pub(crate) fn test_stub() -> Self {
        use std::sync::Arc;
        Self::new(
            r"D:\repo".to_string(),
            Arc::new(test_support::FakeGitRepository::ok()),
            AppSettings::default(),
        )
    }
}

/// What a session keeps when the user switches away from its tab. Pure; unit-tested here.
pub(crate) fn apply_retention(session: &mut WorktreeSession, mode: WorktreeTabRetention) {
    match mode {
        WorktreeTabRetention::Full => {}
        WorktreeTabRetention::Partial => {
            clear_selection(session);
        }
        WorktreeTabRetention::PathOnly => {
            clear_selection(session);
            session.clear_commit_message();
            session.amending = None;
            session.marked_files.clear();
            session.change_filter = ChangeFilter::default();
        }
    }
}

/// Forget the selection and the diff drawn for it. Without clearing the patches the Diff pane
/// would keep showing the old selection's diff when the tab comes back; bumping the generation
/// drops a patch load still in flight.
fn clear_selection(session: &mut WorktreeSession) {
    session.selection = None;
    session.focused_diff_path = None;
    session.patches.clear();
    session.patch_error = None;
    session.patch_pending = false;
    session.patch_generation = session.patch_generation.wrapping_add(1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::changes::{ChangeChip, ChangeFilter, MarkedFile};
    use crate::selection::Selection;
    use core_domain::{FileDisposition, WorktreeTabRetention};

    #[test]
    fn explorer_collapsed_defaults_open_except_optional() {
        let session = WorktreeSession::test_stub();
        assert!(!session.explorer_collapsed.local);
        assert!(!session.explorer_collapsed.remote);
        assert!(!session.explorer_collapsed.worktrees);
        assert!(!session.explorer_collapsed.stashes);
        assert!(!session.explorer_collapsed.tags);
    }

    #[test]
    fn toggle_explorer_section_flips_one_flag() {
        let mut session = WorktreeSession::test_stub();
        session.toggle_explorer(ExplorerSection::Stashes);
        assert!(session.explorer_collapsed.stashes);
        assert!(!session.explorer_collapsed.local);
    }

    #[test]
    fn toggle_explorer_section_twice_restores_open() {
        let mut session = WorktreeSession::test_stub();
        session.toggle_explorer(ExplorerSection::Tags);
        session.toggle_explorer(ExplorerSection::Tags);
        assert!(!session.explorer_collapsed.tags);
    }

    #[test]
    fn ref_folders_default_open_locally_and_closed_inside_a_remote() {
        let mut session = WorktreeSession::test_stub();
        assert!(session.ref_folder_open("local/feature"));
        assert!(session.ref_folder_open("remote/origin"));
        assert!(!session.ref_folder_open("remote/origin/EYC"));
        session.toggle_ref_folder("remote/origin/EYC");
        assert!(session.ref_folder_open("remote/origin/EYC"));
        session.toggle_ref_folder("local/feature");
        assert!(!session.ref_folder_open("local/feature"));
    }

    #[test]
    fn diff_step_walks_the_files_and_stops_at_the_ends() {
        let mut session = WorktreeSession::test_stub();
        session.patches = ["a", "b", "c"]
            .into_iter()
            .map(|path| core_domain::Patch { path: path.into(), hunks: vec![], binary: false })
            .collect();
        assert_eq!(session.diff_step(true).as_deref(), Some("a"));
        assert_eq!(session.diff_step(false).as_deref(), Some("c"));
        session.focused_diff_path = Some("b".into());
        assert_eq!(session.diff_step(true).as_deref(), Some("c"));
        assert_eq!(session.diff_step(false).as_deref(), Some("a"));
        session.focused_diff_path = Some("c".into());
        assert_eq!(session.diff_step(true), None);
    }

    fn dirty_session() -> WorktreeSession {
        let mut session = WorktreeSession::test_stub();
        session.commit_message = "wip".into();
        session.commit_body = text_editor::Content::with_text("details");
        session.marked_files = vec![MarkedFile {
            path: "a.rs".into(),
            place: FileDisposition::Unstaged,
        }];
        session.selection = Some(Selection::Commit { id: "abc".into() });
        session.focused_diff_path = Some("a.rs".into());
        session
    }

    #[test]
    fn same_path_ignores_slash_direction_and_trailing_separators() {
        assert!(same_path(r"D:\repo\wt", "D:/repo/wt/"));
        assert!(!same_path(r"D:\repo\wt", r"D:\repo\other"));
    }

    #[test]
    fn basename_is_the_last_folder() {
        assert_eq!(basename(r"D:\repo\wt"), "wt");
        assert_eq!(basename("D:/repo/wt/"), "wt");
        assert_eq!(basename("solo"), "solo");
    }

    #[test]
    fn partial_clears_selection_keeps_message_and_marks() {
        let mut session = dirty_session();
        apply_retention(&mut session, WorktreeTabRetention::Partial);
        assert_eq!(session.commit_message, "wip");
        assert_eq!(session.marked_files.len(), 1);
        assert!(session.selection.is_none());
        assert!(session.focused_diff_path.is_none());
    }

    #[test]
    fn leaving_with_a_cleared_selection_also_clears_its_diff() {
        let mut session = dirty_session();
        session.patches = vec![core_domain::Patch {
            path: "a.rs".into(),
            hunks: vec![],
            binary: false,
        }];
        session.patch_pending = true;
        let generation = session.patch_generation;
        apply_retention(&mut session, WorktreeTabRetention::Partial);
        assert!(session.patches.is_empty());
        assert!(!session.patch_pending);
        assert_ne!(session.patch_generation, generation);
    }

    #[test]
    fn full_keeps_the_diff() {
        let mut session = dirty_session();
        session.patches = vec![core_domain::Patch {
            path: "a.rs".into(),
            hunks: vec![],
            binary: false,
        }];
        apply_retention(&mut session, WorktreeTabRetention::Full);
        assert_eq!(session.patches.len(), 1);
    }

    #[test]
    fn full_keeps_everything() {
        let mut session = dirty_session();
        apply_retention(&mut session, WorktreeTabRetention::Full);
        assert_eq!(session.commit_message, "wip");
        assert_eq!(session.marked_files.len(), 1);
        assert!(session.selection.is_some());
        assert_eq!(session.focused_diff_path.as_deref(), Some("a.rs"));
    }

    #[test]
    fn a_body_follows_the_summary_after_one_blank_line() {
        assert_eq!(full_commit_message(" Fix parser ", "Long story.\n\n"), "Fix parser\n\nLong story.");
        assert_eq!(full_commit_message("Fix parser", "  \n"), "Fix parser");
        assert_eq!(full_commit_message("  ", "why"), "");
    }

    #[test]
    fn a_suggestion_splits_into_summary_and_body() {
        assert_eq!(split_summary_and_body("Fix parser"), ("Fix parser".to_string(), String::new()));
        assert_eq!(
            split_summary_and_body("Fix parser\n\n\nLong story.\nMore.\n"),
            ("Fix parser".to_string(), "Long story.\nMore.".to_string())
        );
        assert_eq!(
            split_summary_and_body("Fix parser\r\n\r\nBody"),
            ("Fix parser".to_string(), "Body".to_string())
        );
    }

    #[test]
    fn the_commit_edit_carries_summary_and_body() {
        let mut session = WorktreeSession::test_stub();
        session.commit_message = "Fix parser".into();
        session.commit_body = text_editor::Content::with_text("Long story.\n");
        assert!(matches!(
            session.commit_edit(),
            WorktreeEdit::Commit(message) if message == "Fix parser\n\nLong story."
        ));
    }

    #[test]
    fn partial_and_full_keep_the_body() {
        for mode in [WorktreeTabRetention::Partial, WorktreeTabRetention::Full] {
            let mut session = dirty_session();
            apply_retention(&mut session, mode);
            assert_eq!(session.commit_body.text().trim_end(), "details");
        }
    }

    #[test]
    fn path_only_unticks_amend_and_the_others_keep_it() {
        let mut session = dirty_session();
        session.amending = Some("abc".into());
        apply_retention(&mut session, WorktreeTabRetention::Partial);
        assert_eq!(session.amending.as_deref(), Some("abc"));
        apply_retention(&mut session, WorktreeTabRetention::PathOnly);
        assert_eq!(session.amending, None);
    }

    #[test]
    fn path_only_clears_summary_and_body() {
        let mut session = dirty_session();
        apply_retention(&mut session, WorktreeTabRetention::PathOnly);
        assert!(session.commit_message.is_empty());
        assert!(session.commit_body.text().trim().is_empty());
    }

    #[test]
    fn the_filter_survives_partial_and_full_and_path_only_clears_it() {
        for (mode, kept) in [
            (WorktreeTabRetention::Full, true),
            (WorktreeTabRetention::Partial, true),
            (WorktreeTabRetention::PathOnly, false),
        ] {
            let mut session = dirty_session();
            session.change_filter.text = "rs".into();
            session.change_filter.chips.insert(ChangeChip::New);
            apply_retention(&mut session, mode);
            assert_eq!(session.change_filter != ChangeFilter::default(), kept, "{mode:?}");
        }
    }

    #[test]
    fn a_refresh_drops_marks_the_filter_now_hides() {
        use core_domain::{ChangeKind, RepoStatus};
        let status = |kind| {
            let mut status = RepoStatus {
                staged_files: vec![],
                unstaged_files: vec!["a.rs".into()],
                untracked_files: vec![],
                tracking: None,
                head_commit_id: None,
                kinds: Default::default(),
                operation: None,
                conflicted_files: vec![],
            };
            status.kinds.unstaged.insert("a.rs".into(), kind);
            status
        };
        let mut session = WorktreeSession::test_stub();
        session.change_filter.chips.insert(ChangeChip::Modified);
        session.state.current_status = Some(status(ChangeKind::Modified));
        session.marked_files = vec![MarkedFile { path: "a.rs".into(), place: FileDisposition::Unstaged }];
        session.retain_marks();
        assert_eq!(session.marked_files.len(), 1);

        session.state.current_status = Some(status(ChangeKind::Deleted));
        session.retain_marks();
        assert!(session.marked_files.is_empty());
    }

    #[test]
    fn path_only_clears_message_marks_and_selection() {
        let mut session = dirty_session();
        apply_retention(&mut session, WorktreeTabRetention::PathOnly);
        assert!(session.commit_message.is_empty());
        assert!(session.marked_files.is_empty());
        assert!(session.selection.is_none());
        assert!(session.focused_diff_path.is_none());
    }
}
