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

/// Relative height shares for open Explorer sections (`FillPortion` weights).
///
/// Dragging the grip under a section raises or lowers that section's weight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExplorerWeights {
    pub local: u16,
    pub remote: u16,
    pub worktrees: u16,
    pub stashes: u16,
    pub tags: u16,
}

impl Default for ExplorerWeights {
    fn default() -> Self {
        Self {
            local: 2,
            remote: 2,
            worktrees: 2,
            stashes: 1,
            tags: 1,
        }
    }
}

impl ExplorerWeights {
    const MIN: u16 = 1;
    const MAX: u16 = 12;

    pub(crate) fn get(self, section: ExplorerSection) -> u16 {
        match section {
            ExplorerSection::Local => self.local,
            ExplorerSection::Remote => self.remote,
            ExplorerSection::Worktrees => self.worktrees,
            ExplorerSection::Stashes => self.stashes,
            ExplorerSection::Tags => self.tags,
        }
        .clamp(Self::MIN, Self::MAX)
    }

    pub(crate) fn set(&mut self, section: ExplorerSection, weight: u16) {
        let weight = weight.clamp(Self::MIN, Self::MAX);
        match section {
            ExplorerSection::Local => self.local = weight,
            ExplorerSection::Remote => self.remote = weight,
            ExplorerSection::Worktrees => self.worktrees = weight,
            ExplorerSection::Stashes => self.stashes = weight,
            ExplorerSection::Tags => self.tags = weight,
        }
    }
}

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
    /// When true, the history graph shows only commits reachable from this session's HEAD.
    pub(crate) focus_current_branch: bool,
    /// Which Explorer accordion sections are currently collapsed.
    pub(crate) explorer_collapsed: ExplorerCollapsed,
    /// Height shares for open Explorer sections (user-adjustable).
    pub(crate) explorer_weights: ExplorerWeights,
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
            focus_current_branch: false,
            explorer_collapsed: ExplorerCollapsed::default(),
            explorer_weights: ExplorerWeights::default(),
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
            session.commit_message.clear();
            session.marked_files.clear();
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
    use crate::changes::MarkedFile;
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
    fn explorer_weights_clamp_when_set() {
        let mut weights = ExplorerWeights::default();
        weights.set(ExplorerSection::Local, 0);
        assert_eq!(weights.get(ExplorerSection::Local), 1);
        weights.set(ExplorerSection::Worktrees, 99);
        assert_eq!(weights.get(ExplorerSection::Worktrees), 12);
    }

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
    fn path_only_clears_message_marks_and_selection() {
        let mut session = dirty_session();
        apply_retention(&mut session, WorktreeTabRetention::PathOnly);
        assert!(session.commit_message.is_empty());
        assert!(session.marked_files.is_empty());
        assert!(session.selection.is_none());
        assert!(session.focused_diff_path.is_none());
    }
}
