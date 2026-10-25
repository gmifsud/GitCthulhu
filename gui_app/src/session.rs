//! One open worktree tab: its adapter, [`AppState`], and UI bag.
//!
//! `GitApp` owns a `Vec<WorktreeSession>` and an active index. Everything
//! repository-scoped lives here so switching tabs is a pointer move, never a reload.

use std::collections::HashSet;

use core_domain::{AppSettings, AppState, Branch, FileDisposition, Hunk, Patch, ViewEvent};
use core_domain::WorktreeTabRetention;

use crate::changes::MarkedFile;
use crate::jobs::{PatchQuery, SharedRepository};
use crate::selection::{visible_selection, Selection};

/// Diff lines drawn per "Show more" step. One widget per line, so this bounds layout cost.
pub(crate) const DIFF_LINES_PER_PAGE: usize = 400;

/// Everything bound to one open worktree path.
pub(crate) struct WorktreeSession {
    /// Absolute working directory this session opened.
    pub(crate) path: String,
    pub(crate) adapter: SharedRepository,
    pub(crate) state: AppState,
    pub(crate) selection: Option<Selection>,
    pub(crate) commit_message: String,
    pub(crate) marked_files: Vec<MarkedFile>,
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
}

impl WorktreeSession {
    /// Build a fresh session for `path`, seeding its state with the current settings.
    pub(crate) fn new(path: String, adapter: SharedRepository, settings: AppSettings) -> Self {
        let mut state = AppState::new();
        state.apply_event(ViewEvent::SettingsUpdated(settings));
        Self {
            path,
            adapter,
            state,
            selection: None,
            commit_message: String::new(),
            marked_files: Vec::new(),
            collapsed_dirs: HashSet::new(),
            collapsed_sections: HashSet::new(),
            patches: Vec::new(),
            patch_error: None,
            patch_pending: false,
            patch_generation: 0,
            focused_diff_path: None,
            diff_lines_limit: DIFF_LINES_PER_PAGE,
            pending_checkout: None,
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
            for path in files {
                marks.push(MarkedFile {
                    path: path.clone(),
                    place,
                });
            }
        }
        marks
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
            session.selection = None;
            session.focused_diff_path = None;
        }
        WorktreeTabRetention::PathOnly => {
            session.selection = None;
            session.focused_diff_path = None;
            session.commit_message.clear();
            session.marked_files.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::changes::MarkedFile;
    use crate::selection::Selection;
    use core_domain::{FileDisposition, WorktreeTabRetention};

    fn dirty_session() -> WorktreeSession {
        let mut session = WorktreeSession::test_stub();
        session.commit_message = "wip".into();
        session.marked_files = vec![MarkedFile {
            path: "a.rs".into(),
            place: FileDisposition::Unstaged,
        }];
        session.selection = Some(Selection::Commit { id: "abc".into() });
        session.focused_diff_path = Some("a.rs".into());
        session
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
    fn full_keeps_everything() {
        let mut session = dirty_session();
        apply_retention(&mut session, WorktreeTabRetention::Full);
        assert_eq!(session.commit_message, "wip");
        assert_eq!(session.marked_files.len(), 1);
        assert!(session.selection.is_some());
        assert_eq!(session.focused_diff_path.as_deref(), Some("a.rs"));
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
