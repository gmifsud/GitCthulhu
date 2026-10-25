use std::sync::Arc;

use core_domain::{
    AppSettings, DomainError, FileDisposition, Patch, ResetMode, Stash, ViewEvent, Worktree,
    WorktreeTabRetention,
};
use iced::widget::{pane_grid, text_editor};
use iced::Task;

use crate::appearance::{font_family, font_size};
use crate::changes::{marked_paths, MarkedFile};
use crate::jobs::{
    add_worktree_in_background, branch_in_background, checkout_in_background,
    choose_repository_folder, clone_in_background, discard_hunk_in_background,
    discard_in_background, edit_in_background, error_message, fetch_in_background,
    history_in_background, init_in_background, list_models_in_background, load_patch_in_background,
    load_repository_in_background, open_repository_in_background, pull_in_background,
    push_in_background, remove_worktree_in_background, save_settings_in_background,
    stage_hunk_in_background, stash_in_background, suggest_commit_in_background,
    worktrees_in_background, BranchAction, HistoryAction, SharedRepository, StashAction,
    WorktreeEdit,
};
use crate::layout::{self, LayoutPreset, RatioSlot, WorkspacePane};
use crate::selection::Selection;
use crate::session::{apply_retention, same_path, WorktreeSession, DIFF_LINES_PER_PAGE};
use crate::theme::DesignTokens;
use crate::{
    RepositoryCloner, RepositoryInitialiser, RepositoryOpener, SettingsSaver, WorktreeOpener,
};

/// Repository handed from a background open into [`Message::RepositoryOpened`].
///
/// Wraps the trait object so [`Message`] can stay `Debug`.
#[derive(Clone)]
pub(crate) struct OpenedRepository(pub SharedRepository);

impl std::fmt::Debug for OpenedRepository {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OpenedRepository")
    }
}

pub(crate) struct GitApp {
    /// One per open worktree tab. Empty on the welcome screen.
    pub(crate) sessions: Vec<WorktreeSession>,
    /// Index into `sessions` of the tab bound to the window.
    pub(crate) active: usize,
    /// Every worktree of the open repository, from `git worktree list`.
    pub(crate) worktree_catalogue: Vec<Worktree>,
    pub(crate) catalogue_error: Option<String>,
    /// The `+` picker under the tab strip is showing the worktrees that are not open yet.
    pub(crate) tab_picker_open: bool,
    /// A create or remove prompt waiting for the user to confirm.
    pub(crate) worktree_prompt: Option<WorktreePrompt>,
    /// A `git worktree add` / `remove` is in flight; further confirms are ignored.
    pub(crate) worktree_busy: bool,
    /// The path being opened into a tab; further opens are ignored until it lands.
    worktree_opening: Option<String>,
    /// Authoritative, global settings. Fanned into each session's `AppState`.
    pub(crate) settings: AppSettings,
    /// True while a welcome-screen open/init/clone is in flight (no session yet).
    pub(crate) opening: bool,
    pub(crate) panes: pane_grid::State<WorkspacePane>,
    pub(crate) layout: LayoutPreset,
    ratios: [layout::LayoutRatios; 3],
    /// Rebuilt whenever [`Self::install_layout`] constructs a new pane grid.
    split_slots: Vec<(pane_grid::Split, RatioSlot)>,
    pub(crate) files_rail_open: bool,
    pub(crate) branch_list_open: bool,
    pub(crate) branch_query: String,
    pub(crate) files_as_tree: bool,
    pub(crate) agent_models: Vec<String>,
    pub(crate) agent_models_error: Option<String>,
    pub(crate) agent_busy: bool,
    pub(crate) terminal: crate::terminal::Transcript,
    pub(crate) terminal_log: text_editor::Content,
    pub(crate) terminal_open: bool,
    pub(crate) list_models: crate::ModelLister,
    pub(crate) suggest_commit: crate::CommitSuggester,
    pub(crate) error: Option<String>,
    /// Status-bar error shows full text when true; first line only when false.
    pub(crate) error_expanded: bool,
    pub(crate) tokens: DesignTokens,
    pub(crate) settings_open: bool,
    pub(crate) confirm_discard: Option<(String, FileDisposition)>,
    pub(crate) confirm_drop: Option<Stash>,
    pub(crate) branch_prompt: Option<BranchPrompt>,
    pub(crate) history_prompt: Option<HistoryPrompt>,
    pub(crate) clone_url: String,
    open_repository: RepositoryOpener,
    /// Opens a linked worktree into a tab without remembering it.
    open_worktree: WorktreeOpener,
    initialise: RepositoryInitialiser,
    clone_repository: RepositoryCloner,
    save_settings: SettingsSaver,
}

#[derive(Debug, Clone)]
pub(crate) enum Message {
    /// Reload the active session (toolbar, shortcut, welcome-screen open).
    RefreshRepository,
    /// Reload the session at this path. Background finishes use this, never the pathless form.
    RefreshSession(String),
    CheckoutBranch(String),
    CommitMessageChanged(String),
    EditWorktree(WorktreeEdit),
    RepositoryLoaded(String, Vec<ViewEvent>),
    CheckoutFinished(String, Result<(), DomainError>),
    Fetch,
    FetchFinished(String, Result<(), DomainError>),
    Push,
    PushFinished(String, Result<(), DomainError>),
    Pull,
    PullFinished(String, Result<(), DomainError>),
    WorktreeEditFinished(String, WorktreeEdit, Result<(), DomainError>),
    /// Show the native folder dialog.
    ChooseRepositoryFolder,
    PaneResized(pane_grid::ResizeEvent),
    SetLayout(LayoutPreset),
    ToggleFilesRail,
    ToggleBranchList,
    BranchQueryChanged(String),
    ChooseBranch(String),
    ShowWorkingTree,
    FocusFile {
        path: String,
        place: FileDisposition,
    },
    ToggleFileMark {
        path: String,
        place: FileDisposition,
    },
    SelectAllFiles,
    ClearFileSelection,
    SetFileTree(bool),
    /// Fold or unfold a folder row in the changes tree.
    ToggleChangeDir {
        prefix: String,
        place: FileDisposition,
    },
    /// Fold or unfold a Staged / Unstaged / Untracked section.
    ToggleChangeSection(FileDisposition),
    /// Narrow the Diff pane to one path of the current commit/tag patch set.
    FocusDiffPath(String),
    /// Clear [`GitApp::focused_diff_path`] and show every file again.
    ClearDiffFocus,
    /// Dismiss the status-bar error.
    ClearError,
    /// Expand or collapse the status-bar error detail.
    ToggleErrorDetail,
    /// Cycle Columns → InspectorOverDiff → Stacked.
    CycleLayout,
    /// Escape: dismiss prompts, errors, overlays, then clear diff focus.
    Escape,
    StageMarked,
    UnstageMarked,
    /// `None` means the dialog was cancelled.
    RepositoryFolderChosen(Option<String>),
    /// Replace the current adapter with the repository at this path.
    OpenRepository(String),
    /// Result of [`Message::OpenRepository`] after the opener finishes off-thread.
    RepositoryOpened(String, Result<OpenedRepository, DomainError>),
    /// Select the row the ribbon describes.
    Select(Selection),
    OpenSettings,
    RefreshAgentModels,
    AgentTokenChanged(String),
    AgentModelChosen(String),
    AgentModelsLoaded(crate::ModelList),
    SuggestCommit,
    /// The session path is captured when the suggestion is spawned.
    CommitSuggested(String, crate::CommitSuggestion),
    ToggleTerminal,
    TerminalEdited(text_editor::Action),
    ClearTerminal,
    CloseSettings,
    /// Disk write finished. No UI effect; keeps the task from being discarded silently.
    SettingsSaved,
    UiFontChosen(String),
    UiSizeChosen(u8),
    /// The user picked a worktree tab retention mode in Settings.
    WorktreeRetentionChosen(WorktreeTabRetention),
    PatchLoaded {
        path: String,
        generation: u64,
        result: Result<Vec<Patch>, DomainError>,
    },
    /// Draw another page of diff lines in the diff pane.
    ShowMoreDiff,
    AskDiscard,
    CancelDiscard,
    ConfirmDiscard,
    DiscardFinished(String, Result<(), DomainError>),
    StageHunk {
        path: String,
        index: usize,
    },
    DiscardHunk {
        path: String,
        index: usize,
    },
    HunkFinished(String, Result<(), DomainError>),
    ChooseInitFolder,
    InitFolderChosen(Option<String>),
    InitRepository(String),
    InitFinished(String, Result<(), DomainError>),
    CloneUrlChanged(String),
    ChooseCloneFolder,
    /// The URL is captured when the dialog opens. `None` means the dialog was cancelled.
    CloneFolderChosen(String, Option<String>),
    CloneFinished(Result<String, DomainError>),
    StashPush,
    StashApply(usize),
    StashPop(usize),
    AskDropStash,
    CancelDropStash,
    ConfirmDropStash,
    StashFinished(String, Result<(), DomainError>),
    AskCreateBranch,
    AskRenameBranch,
    AskDeleteBranch,
    BranchNameChanged(String),
    ConfirmBranch,
    CancelBranch,
    BranchFinished(String, Result<(), DomainError>),
    AskCherryPick,
    AskReset,
    AskMerge,
    AskRebase,
    CancelHistory,
    ConfirmCherryPick,
    ConfirmReset(ResetMode),
    ConfirmMerge,
    ConfirmRebase,
    HistoryFinished(String, Result<(), DomainError>),
    /// Focus the tab at this index.
    SelectWorktreeTab(usize),
    /// A graph WIP row was pressed: activate that session, then show its working tree.
    ShowSessionWorkingTree(usize),
    /// Close the tab at this index. The worktree on disk is left alone.
    CloseWorktreeTab(usize),
    /// Show or hide the `+` picker of worktrees that are not open yet.
    ToggleTabPicker,
    /// Focus the tab for this path, opening it first when it is not open.
    OpenWorktree(String),
    /// A worktree adapter built off-thread by the `WorktreeOpener`.
    WorktreeOpened(String, Result<OpenedRepository, DomainError>),
    /// The path is the session that asked, so a result for a closed tab is dropped.
    WorktreesLoaded(String, Result<Vec<Worktree>, DomainError>),
    AskCreateWorktree,
    AskRemoveWorktree {
        path: String,
    },
    WorktreePathChanged(String),
    WorktreeBranchChanged(String),
    WorktreeCreateBranchToggled(bool),
    WorktreeForceToggled(bool),
    ChooseWorktreeFolder,
    WorktreeFolderChosen(Option<String>),
    ConfirmCreateWorktree,
    ConfirmRemoveWorktree {
        force: bool,
    },
    CancelWorktree,
    /// `git worktree add` finished. The first path is the session that ran it.
    WorktreeCreated {
        session: String,
        path: String,
        result: Result<(), DomainError>,
    },
    /// `git worktree remove` finished. The first path is the session that ran it.
    WorktreeRemoved {
        session: String,
        path: String,
        result: Result<(), DomainError>,
    },
}

/// A worktree create or remove waiting for the user to confirm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WorktreePrompt {
    Create {
        path: String,
        branch: String,
        /// `-b`: make `branch` at HEAD. Off checks out an existing local branch.
        create_branch: bool,
    },
    Remove {
        path: String,
        branch: String,
        force: bool,
    },
}

/// A cherry-pick, reset, merge, or rebase waiting for confirmation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HistoryPrompt {
    CherryPick { id: String },
    Reset { id: String },
    Merge { name: String },
    Rebase { name: String },
}

/// A create, rename, or delete waiting for the user to confirm the name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BranchPrompt {
    Create { start: String, name: String },
    Rename { from: String, to: String },
    Delete { name: String },
}

enum BranchAsk {
    Create,
    Rename,
    Delete,
}

enum HistoryAsk {
    CherryPick,
    Reset,
    Merge,
    Rebase,
}

impl GitApp {
    #[allow(clippy::too_many_arguments)]
    fn new(
        adapter: Option<SharedRepository>,
        repository_path: Option<String>,
        open_repository: RepositoryOpener,
        open_worktree: WorktreeOpener,
        initialise: RepositoryInitialiser,
        clone_repository: RepositoryCloner,
        settings: AppSettings,
        save_settings: SettingsSaver,
    ) -> Self {
        let sessions = match (adapter, repository_path) {
            (Some(adapter), path) => vec![WorktreeSession::new(
                path.unwrap_or_default(),
                adapter,
                settings.clone(),
            )],
            _ => Vec::new(),
        };
        let layout = LayoutPreset::DEFAULT;
        let ratios = layout::default_ratios();
        let panes = layout::new_panes(layout, &ratios[layout.index()]);
        let split_slots = layout::split_slots(&panes);
        Self {
            sessions,
            active: 0,
            worktree_catalogue: Vec::new(),
            catalogue_error: None,
            tab_picker_open: false,
            worktree_prompt: None,
            worktree_busy: false,
            worktree_opening: None,
            settings,
            opening: false,
            panes,
            layout,
            ratios,
            split_slots,
            files_rail_open: true,
            branch_list_open: false,
            branch_query: String::new(),
            files_as_tree: true,
            agent_models: Vec::new(),
            agent_models_error: None,
            agent_busy: false,
            terminal: crate::terminal::Transcript::new(),
            terminal_log: text_editor::Content::new(),
            terminal_open: true,
            list_models: unavailable_model_lister(),
            suggest_commit: unavailable_suggester(),
            error: None,
            error_expanded: false,
            tokens: DesignTokens::deep_focus(),
            settings_open: false,
            confirm_discard: None,
            confirm_drop: None,
            branch_prompt: None,
            history_prompt: None,
            clone_url: String::new(),
            open_repository,
            open_worktree,
            initialise,
            clone_repository,
            save_settings,
        }
    }

    /// The tab bound to the window. Panics on the welcome screen; call sites that can run
    /// there use [`Self::active_session`] instead.
    pub(crate) fn session(&self) -> &WorktreeSession {
        &self.sessions[self.active]
    }

    pub(crate) fn session_mut(&mut self) -> &mut WorktreeSession {
        &mut self.sessions[self.active]
    }

    fn active_session(&self) -> Option<&WorktreeSession> {
        self.sessions.get(self.active)
    }

    fn active_session_mut(&mut self) -> Option<&mut WorktreeSession> {
        self.sessions.get_mut(self.active)
    }

    /// The session whose path matches, used to route background finishes. An unknown path
    /// (a closed or foreign tab) resolves to `None` so the finish is dropped.
    fn session_mut_by_path(&mut self, path: &str) -> Option<&mut WorktreeSession> {
        self.sessions.iter_mut().find(|session| session.path == path)
    }

    /// Switch to another tab, applying the retention policy to the session left behind.
    fn activate(&mut self, index: usize) {
        if index == self.active || index >= self.sessions.len() {
            return;
        }
        let mode = self.settings.worktree_tab_retention;
        apply_retention(&mut self.sessions[self.active], mode);
        self.active = index;
        self.reset_transient_ui();
    }

    /// Modal prompts and pop-overs are global; clear them so a tab change never carries one across.
    fn reset_transient_ui(&mut self) {
        self.close_prompts();
        self.tab_picker_open = false;
        self.branch_list_open = false;
        self.branch_query.clear();
    }

    #[cfg(test)]
    fn active_path(&self) -> String {
        self.active_session()
            .map(|session| session.path.clone())
            .unwrap_or_default()
    }

    fn install_layout(&mut self) {
        let ratios = self.ratios[self.layout.index()];
        self.panes = if self.files_rail_open {
            layout::new_panes(self.layout, &ratios)
        } else {
            layout::new_panes_without_files(self.layout, &ratios)
        };
        self.split_slots = layout::split_slots(&self.panes);
    }

    fn start_patch(&mut self) -> Task<Message> {
        let Some(session) = self.active_session_mut() else {
            return Task::none();
        };
        session.diff_lines_limit = DIFF_LINES_PER_PAGE;
        let Some(query) = session.patch_query() else {
            session.patches.clear();
            session.patch_error = None;
            session.patch_pending = false;
            return Task::none();
        };
        let repo = session.adapter.clone();
        session.patch_generation = session.patch_generation.wrapping_add(1);
        let generation = session.patch_generation;
        let path = session.path.clone();
        session.patches.clear();
        session.patch_error = None;
        session.patch_pending = true;
        Task::perform(load_patch_in_background(repo, query), move |result| {
            Message::PatchLoaded {
                path: path.clone(),
                generation,
                result,
            }
        })
    }

    /// Fan the authoritative settings into every open session's `AppState`.
    fn apply_settings_to_sessions(&mut self) {
        let settings = self.settings.clone();
        for session in &mut self.sessions {
            session
                .state
                .apply_event(ViewEvent::SettingsUpdated(settings.clone()));
        }
    }

    fn store_agent(&mut self, token: String, model: String, persist: bool) -> Task<Message> {
        self.settings.agent_token = token;
        self.settings.agent_model = model;
        self.apply_settings_to_sessions();
        if persist {
            self.persist_settings(self.settings.clone())
        } else {
            Task::none()
        }
    }

    fn persist_settings(&self, settings: AppSettings) -> Task<Message> {
        let save = Arc::clone(&self.save_settings);
        Task::perform(save_settings_in_background(save, settings), |_| {
            Message::SettingsSaved
        })
    }

    fn refresh_models(&mut self) -> Task<Message> {
        if self.agent_busy {
            return Task::none();
        }
        self.agent_busy = true;
        self.agent_models_error = None;
        let token = self.settings.agent_token.clone();
        let list = std::sync::Arc::clone(&self.list_models);
        Task::perform(
            list_models_in_background(list, token),
            Message::AgentModelsLoaded,
        )
    }

    fn suggest_commit_message(&mut self) -> Task<Message> {
        let busy = self
            .active_session()
            .is_none_or(|session| session.state.loading);
        if self.agent_busy || busy {
            return Task::none();
        }
        let files = self.files_for_suggestion();
        if files.is_empty() {
            self.error = Some("Select a file first.".to_string());
            return Task::none();
        }
        let model = self.settings.agent_model.trim().to_string();
        if model.is_empty() {
            self.error = Some("Choose a model under Settings.".to_string());
            return Task::none();
        }
        let Some((repo, path)) = self
            .active_session()
            .map(|session| (session.adapter.clone(), session.path.clone()))
        else {
            return Task::none();
        };
        self.agent_busy = true;
        self.error = None;
        let token = self.settings.agent_token.clone();
        let suggest = std::sync::Arc::clone(&self.suggest_commit);
        Task::perform(
            suggest_commit_in_background(repo, files, token, model, suggest),
            move |result| Message::CommitSuggested(path.clone(), result),
        )
    }

    fn files_for_suggestion(&self) -> Vec<(String, FileDisposition)> {
        let Some(session) = self.active_session() else {
            return Vec::new();
        };
        if !session.marked_files.is_empty() {
            return session
                .marked_files
                .iter()
                .map(|mark| (mark.path.clone(), mark.place))
                .collect();
        }
        match &session.selection {
            Some(Selection::File { path, place }) => vec![(path.clone(), *place)],
            _ => Vec::new(),
        }
    }

    fn store_appearance(&mut self, font: &str, size: u8) -> Task<Message> {
        self.settings.ui_font = font_family(font).to_string();
        self.settings.ui_size = font_size(size);
        self.apply_settings_to_sessions();
        self.persist_settings(self.settings.clone())
    }

    #[cfg(test)]
    pub(crate) fn with_repo(repo: SharedRepository) -> Self {
        Self::with_repo_opener(
            repo,
            None,
            unavailable_opener(),
            unavailable_opener(),
            unavailable_initialiser(),
            unavailable_cloner(),
            AppSettings::default(),
            ignore_settings(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn with_repo_opener(
        repo: SharedRepository,
        repository_path: Option<String>,
        open_repository: RepositoryOpener,
        open_worktree: WorktreeOpener,
        initialise: RepositoryInitialiser,
        clone_repository: RepositoryCloner,
        settings: AppSettings,
        save_settings: SettingsSaver,
    ) -> Self {
        Self::new(
            Some(repo),
            repository_path,
            open_repository,
            open_worktree,
            initialise,
            clone_repository,
            settings,
            save_settings,
        )
    }

    #[cfg(test)]
    pub(crate) fn welcome() -> Self {
        Self::welcome_with(
            unavailable_opener(),
            unavailable_opener(),
            unavailable_initialiser(),
            unavailable_cloner(),
            AppSettings::default(),
            ignore_settings(),
        )
    }

    pub(crate) fn welcome_with(
        open_repository: RepositoryOpener,
        open_worktree: WorktreeOpener,
        initialise: RepositoryInitialiser,
        clone_repository: RepositoryCloner,
        settings: AppSettings,
        save_settings: SettingsSaver,
    ) -> Self {
        Self::new(
            None,
            None,
            open_repository,
            open_worktree,
            initialise,
            clone_repository,
            settings,
            save_settings,
        )
    }

    pub(crate) fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::RefreshRepository => {
                Task::batch([self.begin_refresh(true), self.load_catalogue()])
            }
            Message::RefreshSession(path) => self.begin_refresh_for(&path, true),
            Message::RepositoryLoaded(path, events) => {
                let mut saw_error = None;
                let Some(session) = self.session_mut_by_path(&path) else {
                    return Task::none();
                };
                for event in events {
                    if let ViewEvent::ErrorOccurred(error) = &event {
                        saw_error.get_or_insert_with(|| error_message(error));
                    }
                    session.state.apply_event(event);
                }
                session.reconcile_selection();
                session.retain_marks();
                if let Some(message) = saw_error {
                    self.error.get_or_insert(message);
                }
                if self.session().path == path {
                    self.start_patch()
                } else {
                    Task::none()
                }
            }
            Message::FocusFile { path, place } => {
                if let Some(session) = self.active_session_mut() {
                    session.marked_files = vec![MarkedFile {
                        path: path.clone(),
                        place,
                    }];
                }
                self.update(Message::Select(Selection::File { path, place }))
            }
            Message::ToggleFileMark { path, place } => {
                if let Some(session) = self.active_session_mut() {
                    if let Some(index) = session
                        .marked_files
                        .iter()
                        .position(|mark| mark.path == path && mark.place == place)
                    {
                        session.marked_files.remove(index);
                    } else {
                        session.marked_files.push(MarkedFile {
                            path: path.clone(),
                            place,
                        });
                    }
                }
                self.update(Message::Select(Selection::File { path, place }))
            }
            Message::SelectAllFiles => {
                let Some(session) = self.active_session_mut() else {
                    return Task::none();
                };
                session.marked_files = session.all_changed_files();
                if session.marked_files.is_empty() {
                    return Task::none();
                }
                let needs_file = !matches!(session.selection, Some(Selection::File { .. }));
                if needs_file {
                    let first = session.marked_files[0].clone();
                    return self.update(Message::Select(Selection::File {
                        path: first.path,
                        place: first.place,
                    }));
                }
                Task::none()
            }
            Message::ClearFileSelection => {
                if let Some(session) = self.active_session_mut() {
                    session.marked_files.clear();
                }
                self.update(Message::ShowWorkingTree)
            }
            Message::SetFileTree(as_tree) => {
                self.files_as_tree = as_tree;
                Task::none()
            }
            Message::ToggleChangeDir { prefix, place } => {
                if let Some(session) = self.active_session_mut() {
                    let key = (place, prefix);
                    if !session.collapsed_dirs.remove(&key) {
                        session.collapsed_dirs.insert(key);
                    }
                }
                Task::none()
            }
            Message::ToggleChangeSection(place) => {
                if let Some(session) = self.active_session_mut() {
                    if !session.collapsed_sections.remove(&place) {
                        session.collapsed_sections.insert(place);
                    }
                }
                Task::none()
            }
            Message::FocusDiffPath(path) => {
                if let Some(session) = self.active_session_mut() {
                    session.focused_diff_path = Some(path);
                }
                Task::none()
            }
            Message::ClearDiffFocus => {
                if let Some(session) = self.active_session_mut() {
                    session.focused_diff_path = None;
                }
                Task::none()
            }
            Message::ClearError => {
                self.error = None;
                self.error_expanded = false;
                Task::none()
            }
            Message::ToggleErrorDetail => {
                if self.error.is_some() {
                    self.error_expanded = !self.error_expanded;
                }
                Task::none()
            }
            Message::CycleLayout => {
                let next = self.layout.next();
                self.update(Message::SetLayout(next))
            }
            Message::Escape => self.handle_escape(),
            Message::StageMarked => self.edit_marked(true),
            Message::UnstageMarked => self.edit_marked(false),
            Message::CheckoutBranch(branch) => {
                let Some(path) = self.active_session().map(|session| session.path.clone()) else {
                    return Task::none();
                };
                self.checkout_for(&path, branch)
            }
            Message::CheckoutFinished(path, result) => {
                // A checkout moves this worktree's branch, which the explorer lists.
                let moved = result.is_ok();
                let reload = self.finish_reload(&path, result);
                if moved {
                    Task::batch([reload, self.load_catalogue_for(&path)])
                } else {
                    reload
                }
            }
            Message::Fetch => {
                let Some(session) = self.active_session() else {
                    return Task::none();
                };
                if session.state.loading {
                    return Task::none();
                }
                let repo = session.adapter.clone();
                let path = session.path.clone();
                self.session_mut().state.loading = true;
                self.error = None;
                Task::perform(fetch_in_background(repo), move |result| {
                    Message::FetchFinished(path.clone(), result)
                })
            }
            Message::FetchFinished(path, result) => self.finish_reload(&path, result),
            Message::Push => {
                let Some(session) = self.active_session() else {
                    return Task::none();
                };
                if session.state.loading {
                    return Task::none();
                }
                let repo = session.adapter.clone();
                let path = session.path.clone();
                self.session_mut().state.loading = true;
                self.error = None;
                Task::perform(push_in_background(repo), move |result| {
                    Message::PushFinished(path.clone(), result)
                })
            }
            Message::PushFinished(path, result) => self.finish_reload(&path, result),
            Message::Pull => {
                let Some(session) = self.active_session() else {
                    return Task::none();
                };
                if session.state.loading {
                    return Task::none();
                }
                let repo = session.adapter.clone();
                let path = session.path.clone();
                self.session_mut().state.loading = true;
                self.error = None;
                Task::perform(pull_in_background(repo), move |result| {
                    Message::PullFinished(path.clone(), result)
                })
            }
            Message::PullFinished(path, result) => self.finish_reload(&path, result),
            Message::CommitMessageChanged(message) => {
                if let Some(session) = self.active_session_mut() {
                    session.commit_message = message;
                }
                Task::none()
            }
            Message::EditWorktree(edit) => {
                let Some(session) = self.active_session() else {
                    return Task::none();
                };
                if session.state.loading {
                    return Task::none();
                }
                if let WorktreeEdit::Commit(message) = &edit {
                    if message.trim().is_empty() {
                        return Task::none();
                    }
                }
                let repo = session.adapter.clone();
                let path = session.path.clone();
                self.session_mut().state.loading = true;
                self.error = None;
                let report = edit.clone();
                Task::perform(edit_in_background(repo, edit), move |result| {
                    Message::WorktreeEditFinished(path.clone(), report.clone(), result)
                })
            }
            Message::WorktreeEditFinished(path, edit, result) => {
                let Some(session) = self.session_mut_by_path(&path) else {
                    return Task::none();
                };
                session.state.loading = false;
                match result {
                    Ok(()) => {
                        if matches!(edit, WorktreeEdit::Commit(_)) {
                            session.commit_message.clear();
                        }
                        Task::done(Message::RefreshSession(path))
                    }
                    Err(error) => {
                        self.error = Some(error_message(&error));
                        Task::none()
                    }
                }
            }
            Message::PaneResized(event) => {
                let slot = self
                    .split_slots
                    .iter()
                    .find(|(split, _)| *split == event.split)
                    .map(|(_, slot)| *slot);
                if let Some(slot) = slot {
                    let ratios = &mut self.ratios[self.layout.index()];
                    ratios.clamp_slot(slot, event.ratio);
                    let clamped = match slot {
                        RatioSlot::Graph => ratios.graph,
                        RatioSlot::Middle => ratios.middle,
                        RatioSlot::Files => ratios.files,
                    };
                    self.panes.resize(event.split, clamped);
                } else {
                    self.panes.resize(event.split, event.ratio);
                }
                Task::none()
            }
            Message::SetLayout(preset) => {
                if self.layout != preset {
                    self.layout = preset;
                    self.install_layout();
                }
                Task::none()
            }
            Message::ToggleFilesRail => {
                self.files_rail_open = !self.files_rail_open;
                self.install_layout();
                Task::none()
            }
            Message::ToggleBranchList => {
                self.branch_list_open = !self.branch_list_open;
                if !self.branch_list_open {
                    self.branch_query.clear();
                }
                Task::none()
            }
            Message::BranchQueryChanged(query) => {
                self.branch_query = query;
                Task::none()
            }
            Message::ChooseBranch(name) => {
                self.branch_list_open = false;
                self.branch_query.clear();
                let Some(session) = self.active_session() else {
                    return Task::none();
                };
                let current = session
                    .state
                    .branches
                    .iter()
                    .any(|branch| !branch.is_remote && branch.is_head && branch.name == name);
                if current {
                    self.session_mut().selection =
                        Some(Selection::Branch { name, current: true });
                    return Task::none();
                }
                if self.has_local_branch(&name) {
                    return self.update(Message::CheckoutBranch(name));
                }
                self.session_mut().selection = Some(Selection::Branch {
                    name,
                    current: false,
                });
                Task::none()
            }
            Message::ShowWorkingTree => {
                let Some(session) = self.active_session_mut() else {
                    return Task::none();
                };
                session.focused_diff_path = None;
                let name = session
                    .state
                    .branches
                    .iter()
                    .find(|branch| branch.is_head && !branch.is_remote)
                    .map(|branch| branch.name.clone())
                    .unwrap_or_else(|| "Detached HEAD".to_string());
                session.selection = Some(Selection::Branch {
                    name,
                    current: true,
                });
                self.start_patch()
            }
            Message::ChooseRepositoryFolder => {
                if self.opening {
                    return Task::none();
                }
                Task::perform(
                    choose_repository_folder("Open repository"),
                    Message::RepositoryFolderChosen,
                )
            }
            Message::RepositoryFolderChosen(None) => Task::none(),
            Message::RepositoryFolderChosen(Some(path)) => {
                self.update(Message::OpenRepository(path))
            }
            Message::OpenRepository(path) => {
                if self.opening {
                    return Task::none();
                }
                if path.trim().is_empty() {
                    self.error = Some("Choose a repository folder.".to_string());
                    return Task::none();
                }
                self.opening = true;
                self.error = None;
                let open = Arc::clone(&self.open_repository);
                Task::perform(
                    open_repository_in_background(open, path),
                    |(path, result)| Message::RepositoryOpened(path, result.map(OpenedRepository)),
                )
            }
            Message::RepositoryOpened(path, Ok(OpenedRepository(repo))) => {
                let open_repository = Arc::clone(&self.open_repository);
                let open_worktree = Arc::clone(&self.open_worktree);
                let initialise = Arc::clone(&self.initialise);
                let clone_repository = Arc::clone(&self.clone_repository);
                let save_settings = Arc::clone(&self.save_settings);
                let settings = self.settings.clone();
                let list_models = Arc::clone(&self.list_models);
                let suggest_commit = Arc::clone(&self.suggest_commit);
                *self = Self::new(
                    Some(repo),
                    Some(path),
                    open_repository,
                    open_worktree,
                    initialise,
                    clone_repository,
                    settings,
                    save_settings,
                );
                self.list_models = list_models;
                self.suggest_commit = suggest_commit;
                Task::done(Message::RefreshRepository)
            }
            Message::RepositoryOpened(_, Err(error)) => {
                self.opening = false;
                self.error = Some(error_message(&error));
                Task::none()
            }
            Message::ChooseInitFolder => {
                if self.opening {
                    return Task::none();
                }
                Task::perform(
                    choose_repository_folder("Init repository"),
                    Message::InitFolderChosen,
                )
            }
            Message::InitFolderChosen(None) => Task::none(),
            Message::InitFolderChosen(Some(path)) => self.update(Message::InitRepository(path)),
            Message::InitRepository(path) => {
                if self.opening {
                    return Task::none();
                }
                if path.trim().is_empty() {
                    self.error = Some("Choose a folder to init.".to_string());
                    return Task::none();
                }
                self.opening = true;
                self.error = None;
                let initialise = Arc::clone(&self.initialise);
                let finished = path.clone();
                Task::perform(init_in_background(initialise, path), move |result| {
                    Message::InitFinished(finished.clone(), result)
                })
            }
            Message::InitFinished(path, Ok(())) => {
                self.opening = false;
                self.update(Message::OpenRepository(path))
            }
            Message::InitFinished(_, Err(error)) => {
                self.opening = false;
                self.error = Some(error_message(&error));
                Task::none()
            }
            Message::CloneUrlChanged(url) => {
                self.clone_url = url;
                Task::none()
            }
            Message::ChooseCloneFolder => {
                if self.opening || self.clone_url.trim().is_empty() {
                    return Task::none();
                }
                let url = self.clone_url.clone();
                Task::perform(
                    choose_repository_folder("Clone into folder"),
                    move |parent| Message::CloneFolderChosen(url.clone(), parent),
                )
            }
            Message::CloneFolderChosen(_, None) => Task::none(),
            Message::CloneFolderChosen(url, Some(parent)) => {
                if self.opening {
                    return Task::none();
                }
                self.opening = true;
                self.error = None;
                let clone_repository = Arc::clone(&self.clone_repository);
                Task::perform(
                    clone_in_background(clone_repository, url, parent),
                    Message::CloneFinished,
                )
            }
            Message::CloneFinished(Ok(path)) => {
                self.opening = false;
                self.update(Message::OpenRepository(path))
            }
            Message::CloneFinished(Err(error)) => {
                self.opening = false;
                self.error = Some(error_message(&error));
                Task::none()
            }
            Message::Select(selection) => {
                self.branch_list_open = false;
                self.branch_query.clear();
                self.confirm_discard = None;
                self.confirm_drop = None;
                self.branch_prompt = None;
                if let Some(session) = self.active_session_mut() {
                    session.selection = Some(selection);
                    session.focused_diff_path = None;
                }
                self.start_patch()
            }
            Message::OpenSettings => {
                self.settings_open = true;
                if self.agent_models.is_empty() && !self.agent_busy {
                    self.refresh_models()
                } else {
                    Task::none()
                }
            }
            Message::RefreshAgentModels => self.refresh_models(),
            Message::AgentTokenChanged(token) => {
                // Keep typing responsive: update state only; persist on CloseSettings.
                self.store_agent(token, self.settings.agent_model.clone(), false)
            }
            Message::AgentModelChosen(model) => {
                self.store_agent(self.settings.agent_token.clone(), model, true)
            }
            Message::AgentModelsLoaded(result) => {
                self.agent_busy = false;
                self.terminal.record(result.exchange);
                self.sync_terminal_log();
                match result.ids {
                    Ok(mut ids) => {
                        self.agent_models_error = None;
                        let current = self.settings.agent_model.clone();
                        let persist = if !current.is_empty() && !ids.iter().any(|id| id == &current)
                        {
                            ids.insert(0, current);
                            Task::none()
                        } else if self.settings.agent_model.is_empty() {
                            let chosen = ids
                                .iter()
                                .find(|id| id.as_str() == AppSettings::DEFAULT_AGENT_MODEL)
                                .cloned()
                                .unwrap_or_else(|| {
                                    ids.first()
                                        .cloned()
                                        .unwrap_or_else(|| AppSettings::DEFAULT_AGENT_MODEL.into())
                                });
                            self.store_agent(self.settings.agent_token.clone(), chosen, true)
                        } else {
                            Task::none()
                        };
                        self.agent_models = ids;
                        persist
                    }
                    Err(error) => {
                        self.agent_models_error = Some(error_message(&error));
                        Task::none()
                    }
                }
            }
            Message::SuggestCommit => self.suggest_commit_message(),
            Message::CommitSuggested(path, result) => {
                // The agent is a global resource and the transcript a global log, so both
                // settle even when the originating tab is gone. Session state does not.
                self.agent_busy = false;
                self.terminal.record(result.exchange);
                self.sync_terminal_log();
                if self.session_mut_by_path(&path).is_none() {
                    return Task::none();
                }
                match result.subject {
                    Ok(subject) => {
                        if let Some(session) = self.session_mut_by_path(&path) {
                            session.commit_message = subject;
                        }
                    }
                    Err(error) => self.error = Some(error_message(&error)),
                }
                Task::none()
            }
            Message::ToggleTerminal => {
                self.terminal_open = !self.terminal_open;
                Task::none()
            }
            Message::TerminalEdited(action) => {
                if !action.is_edit() {
                    self.terminal_log.perform(action);
                }
                Task::none()
            }
            Message::ClearTerminal => {
                self.terminal.clear();
                self.sync_terminal_log();
                Task::none()
            }
            Message::CloseSettings => {
                self.settings_open = false;
                self.persist_settings(self.settings.clone())
            }
            Message::SettingsSaved => Task::none(),
            Message::UiFontChosen(font) => {
                self.store_appearance(&font, self.settings.ui_size)
            }
            Message::UiSizeChosen(size) => {
                let font = self.settings.ui_font.clone();
                self.store_appearance(&font, size)
            }
            Message::WorktreeRetentionChosen(mode) => {
                self.settings.worktree_tab_retention = mode;
                self.apply_settings_to_sessions();
                self.persist_settings(self.settings.clone())
            }
            Message::PatchLoaded {
                path,
                generation,
                result,
            } => {
                let Some(session) = self.session_mut_by_path(&path) else {
                    return Task::none();
                };
                if generation != session.patch_generation {
                    return Task::none();
                }
                session.patch_pending = false;
                match result {
                    Ok(patches) => {
                        session.patches = patches;
                        session.patch_error = None;
                    }
                    Err(error) => {
                        session.patches.clear();
                        session.patch_error = Some(error_message(&error));
                    }
                }
                Task::none()
            }
            Message::ShowMoreDiff => {
                if let Some(session) = self.active_session_mut() {
                    session.diff_lines_limit =
                        session.diff_lines_limit.saturating_add(DIFF_LINES_PER_PAGE);
                }
                Task::none()
            }
            Message::AskDiscard => {
                let Some(session) = self.active_session() else {
                    return Task::none();
                };
                if let Selection::File { path, place } = session.shown_selection() {
                    self.confirm_discard = Some((path, place));
                }
                Task::none()
            }
            Message::CancelDiscard => {
                self.confirm_discard = None;
                Task::none()
            }
            Message::ConfirmDiscard => {
                let Some((path, place)) = self.confirm_discard.clone() else {
                    return Task::none();
                };
                let Some(session) = self.active_session() else {
                    return Task::none();
                };
                if session.state.loading {
                    return Task::none();
                }
                let repo = session.adapter.clone();
                let session_path = session.path.clone();
                self.confirm_discard = None;
                self.session_mut().state.loading = true;
                self.error = None;
                Task::perform(
                    discard_in_background(repo, path, place),
                    move |result| Message::DiscardFinished(session_path.clone(), result),
                )
            }
            Message::DiscardFinished(path, result) => self.finish_reload(&path, result),
            Message::StageHunk { path, index } => self.edit_hunk(path, index, true),
            Message::DiscardHunk { path, index } => self.edit_hunk(path, index, false),
            Message::HunkFinished(path, result) => self.finish_reload(&path, result),
            Message::StashPush => self.run_stash(StashAction::Push),
            Message::StashApply(index) => self.run_stash(StashAction::Apply(index)),
            Message::StashPop(index) => self.run_stash(StashAction::Pop(index)),
            Message::AskDropStash => {
                let Some(session) = self.active_session() else {
                    return Task::none();
                };
                if session.state.loading {
                    return Task::none();
                }
                if let Selection::Stash { index, message } = session.shown_selection() {
                    self.confirm_discard = None;
                    self.confirm_drop = Some(Stash { index, message });
                }
                Task::none()
            }
            Message::CancelDropStash => {
                self.confirm_drop = None;
                Task::none()
            }
            Message::ConfirmDropStash => {
                let Some(stash) = self.confirm_drop.clone() else {
                    return Task::none();
                };
                let still_there = self.active_session().is_some_and(|session| {
                    session
                        .state
                        .stashes
                        .iter()
                        .any(|entry| entry.index == stash.index && entry.message == stash.message)
                });
                if !still_there {
                    self.confirm_drop = None;
                    self.error = Some("That stash is no longer there.".to_string());
                    return Task::none();
                }
                self.run_stash(StashAction::Drop(stash.index))
            }
            Message::StashFinished(path, result) => self.finish_reload(&path, result),
            Message::AskCreateBranch => self.ask_branch(BranchAsk::Create),
            Message::AskRenameBranch => self.ask_branch(BranchAsk::Rename),
            Message::AskDeleteBranch => self.ask_branch(BranchAsk::Delete),
            Message::BranchNameChanged(name) => {
                match &mut self.branch_prompt {
                    Some(BranchPrompt::Create { name: field, .. }) => *field = name,
                    Some(BranchPrompt::Rename { to, .. }) => *to = name,
                    _ => {}
                }
                Task::none()
            }
            Message::CancelBranch => {
                self.branch_prompt = None;
                if let Some(session) = self.active_session_mut() {
                    session.pending_checkout = None;
                }
                Task::none()
            }
            Message::ConfirmBranch => self.confirm_branch(),
            Message::BranchFinished(path, result) => {
                // An unknown path (closed or foreign tab) is dropped whole: no checkout,
                // no error, no refresh.
                let Some(session) = self.session_mut_by_path(&path) else {
                    return Task::none();
                };
                session.state.loading = false;
                let pending = session.pending_checkout.take();
                match result {
                    Ok(()) => match pending {
                        Some(name) => self.checkout_for(&path, name),
                        None => Task::done(Message::RefreshSession(path)),
                    },
                    Err(error) => {
                        self.error = Some(error_message(&error));
                        Task::none()
                    }
                }
            }
            Message::AskCherryPick => self.ask_history(HistoryAsk::CherryPick),
            Message::AskReset => self.ask_history(HistoryAsk::Reset),
            Message::AskMerge => self.ask_history(HistoryAsk::Merge),
            Message::AskRebase => self.ask_history(HistoryAsk::Rebase),
            Message::CancelHistory => {
                self.history_prompt = None;
                Task::none()
            }
            Message::ConfirmCherryPick => self.confirm_cherry_pick(),
            Message::ConfirmReset(mode) => self.confirm_reset(mode),
            Message::ConfirmMerge => self.confirm_merge(),
            Message::ConfirmRebase => self.confirm_rebase(),
            Message::HistoryFinished(path, result) => {
                let Some(session) = self.session_mut_by_path(&path) else {
                    return Task::none();
                };
                session.state.loading = false;
                match result {
                    Ok(()) => self.begin_refresh_for(&path, true),
                    Err(error) => {
                        self.error = Some(error_message(&error));
                        self.begin_refresh_for(&path, false)
                    }
                }
            }
            Message::SelectWorktreeTab(index) => {
                self.activate(index);
                self.tab_picker_open = false;
                Task::none()
            }
            Message::ShowSessionWorkingTree(index) => {
                if index >= self.sessions.len() {
                    return Task::none();
                }
                self.activate(index);
                self.update(Message::ShowWorkingTree)
            }
            Message::CloseWorktreeTab(index) => {
                // The last tab stays, and so does a tab with a job running: its finish
                // would have nowhere to land.
                if self.sessions.len() <= 1
                    || self
                        .sessions
                        .get(index)
                        .is_none_or(|session| session.state.loading)
                {
                    return Task::none();
                }
                self.drop_session(index);
                Task::none()
            }
            Message::ToggleTabPicker => {
                self.tab_picker_open = !self.tab_picker_open;
                Task::none()
            }
            Message::OpenWorktree(path) => self.open_worktree_tab(path),
            Message::WorktreeOpened(path, result) => self.worktree_opened(path, result),
            Message::WorktreesLoaded(path, result) => {
                // A result for a closed tab or a replaced repository is dropped.
                if self.session_mut_by_path(&path).is_none() {
                    return Task::none();
                }
                match result {
                    Ok(list) => {
                        self.worktree_catalogue = list;
                        self.catalogue_error = None;
                    }
                    // Keep the last good catalogue; the explorer shows why it is stale.
                    Err(error) => self.catalogue_error = Some(error_message(&error)),
                }
                Task::none()
            }
            Message::AskCreateWorktree => {
                if self.worktree_busy || self.active_session().is_none() {
                    return Task::none();
                }
                self.close_prompts();
                self.error = None;
                self.worktree_prompt = Some(WorktreePrompt::Create {
                    path: String::new(),
                    branch: String::new(),
                    create_branch: true,
                });
                Task::none()
            }
            Message::AskRemoveWorktree { path } => self.ask_remove_worktree(&path),
            Message::WorktreePathChanged(value) => {
                if let Some(WorktreePrompt::Create { path, .. }) = &mut self.worktree_prompt {
                    *path = value;
                }
                Task::none()
            }
            Message::WorktreeBranchChanged(value) => {
                if let Some(WorktreePrompt::Create { branch, .. }) = &mut self.worktree_prompt {
                    *branch = value;
                }
                Task::none()
            }
            Message::WorktreeCreateBranchToggled(value) => {
                if let Some(WorktreePrompt::Create { create_branch, .. }) =
                    &mut self.worktree_prompt
                {
                    *create_branch = value;
                }
                Task::none()
            }
            Message::WorktreeForceToggled(value) => {
                if let Some(WorktreePrompt::Remove { force, .. }) = &mut self.worktree_prompt {
                    *force = value;
                }
                Task::none()
            }
            Message::ChooseWorktreeFolder => {
                if !matches!(self.worktree_prompt, Some(WorktreePrompt::Create { .. })) {
                    return Task::none();
                }
                Task::perform(
                    choose_repository_folder("Worktree folder"),
                    Message::WorktreeFolderChosen,
                )
            }
            Message::WorktreeFolderChosen(None) => Task::none(),
            Message::WorktreeFolderChosen(Some(folder)) => {
                if let Some(WorktreePrompt::Create { path, .. }) = &mut self.worktree_prompt {
                    *path = folder;
                }
                Task::none()
            }
            Message::ConfirmCreateWorktree => self.confirm_create_worktree(),
            Message::ConfirmRemoveWorktree { force } => self.confirm_remove_worktree(force),
            Message::CancelWorktree => {
                self.worktree_prompt = None;
                Task::none()
            }
            Message::WorktreeCreated {
                session,
                path,
                result,
            } => {
                self.worktree_busy = false;
                match result {
                    Ok(()) => {
                        self.worktree_prompt = None;
                        let reload = self.load_catalogue_for(&session);
                        // Open the new tab only when the session that asked is still here;
                        // otherwise the repository was replaced and the path is foreign.
                        if self.sessions.iter().any(|open| open.path == session) {
                            Task::batch([reload, Task::done(Message::OpenWorktree(path))])
                        } else {
                            reload
                        }
                    }
                    // The prompt stays open with git's message so the user can fix the field.
                    Err(error) => {
                        self.error = Some(error_message(&error));
                        Task::none()
                    }
                }
            }
            Message::WorktreeRemoved {
                session,
                path,
                result,
            } => {
                self.worktree_busy = false;
                match result {
                    Ok(()) => {
                        self.worktree_prompt = None;
                        self.drop_sessions_at(&path);
                        self.load_catalogue_for(&session)
                    }
                    // Stay open: a dirty worktree is retried with Force ticked.
                    Err(error) => {
                        self.error = Some(error_message(&error));
                        Task::none()
                    }
                }
            }
        }
    }

    /// Close every modal that belongs to the window rather than a tab.
    fn close_prompts(&mut self) {
        self.confirm_discard = None;
        self.confirm_drop = None;
        self.branch_prompt = None;
        self.history_prompt = None;
        self.worktree_prompt = None;
    }

    /// Drop the session at `index`. A no-op when it is the last one, so a tab always remains.
    /// The active index follows: a closed active tab hands over to the main worktree's tab
    /// when it is open, otherwise to its neighbour.
    fn drop_session(&mut self, index: usize) {
        if index >= self.sessions.len() || self.sessions.len() <= 1 {
            return;
        }
        let was_active = index == self.active;
        self.sessions.remove(index);
        if was_active {
            self.active = self.successor_of_closed(index);
            self.reset_transient_ui();
        } else if index < self.active {
            self.active -= 1;
        }
    }

    /// The index to focus after the session at `closed` left the list.
    fn successor_of_closed(&self, closed: usize) -> usize {
        let main = self
            .worktree_catalogue
            .iter()
            .find(|entry| entry.is_main)
            .and_then(|entry| {
                self.sessions
                    .iter()
                    .position(|session| same_path(&session.path, &entry.path))
            });
        main.unwrap_or_else(|| closed.min(self.sessions.len() - 1))
    }

    /// Drop every session that opened `path`. Removing the last one leaves the welcome screen.
    fn drop_sessions_at(&mut self, path: &str) {
        while let Some(index) = self
            .sessions
            .iter()
            .position(|session| same_path(&session.path, path))
        {
            if self.sessions.len() == 1 {
                self.sessions.clear();
                self.active = 0;
                self.worktree_catalogue.clear();
                self.reset_transient_ui();
                return;
            }
            self.drop_session(index);
        }
    }

    /// Reload the catalogue through the active session.
    fn load_catalogue(&mut self) -> Task<Message> {
        let Some(path) = self.active_session().map(|session| session.path.clone()) else {
            return Task::none();
        };
        self.load_catalogue_for(&path)
    }

    /// Reload the catalogue through the session at `path`. An unknown path is dropped.
    fn load_catalogue_for(&mut self, path: &str) -> Task<Message> {
        let Some(session) = self.session_mut_by_path(path) else {
            return Task::none();
        };
        let repo = session.adapter.clone();
        let path = session.path.clone();
        Task::perform(worktrees_in_background(repo), move |result| {
            Message::WorktreesLoaded(path.clone(), result)
        })
    }

    /// Focus the tab for `path`, or build an adapter for it off-thread.
    fn open_worktree_tab(&mut self, path: String) -> Task<Message> {
        self.tab_picker_open = false;
        if let Some(index) = self
            .sessions
            .iter()
            .position(|session| same_path(&session.path, &path))
        {
            self.activate(index);
            return Task::none();
        }
        if self.sessions.is_empty() || self.worktree_opening.is_some() || path.trim().is_empty() {
            return Task::none();
        }
        self.worktree_opening = Some(path.clone());
        self.error = None;
        let open = Arc::clone(&self.open_worktree);
        Task::perform(
            open_repository_in_background(open, path),
            |(path, result)| Message::WorktreeOpened(path, result.map(OpenedRepository)),
        )
    }

    fn worktree_opened(
        &mut self,
        path: String,
        result: Result<OpenedRepository, DomainError>,
    ) -> Task<Message> {
        // A result that is not the open in flight belongs to a replaced repository.
        let expected = self
            .worktree_opening
            .as_deref()
            .is_some_and(|pending| same_path(pending, &path));
        if !expected {
            // An empty path is a failure with no echo; it can only be ours, so release the lock.
            if path.trim().is_empty() {
                self.worktree_opening = None;
                if let Err(error) = result {
                    self.error = Some(error_message(&error));
                }
            }
            return Task::none();
        }
        self.worktree_opening = None;
        if self.sessions.is_empty() {
            return Task::none();
        }
        match result {
            Ok(OpenedRepository(repo)) => {
                if let Some(index) = self
                    .sessions
                    .iter()
                    .position(|session| same_path(&session.path, &path))
                {
                    self.activate(index);
                    return Task::none();
                }
                self.sessions.push(WorktreeSession::new(
                    path.clone(),
                    repo,
                    self.settings.clone(),
                ));
                self.activate(self.sessions.len() - 1);
                Task::done(Message::RefreshSession(path))
            }
            Err(error) => {
                self.error = Some(error_message(&error));
                Task::none()
            }
        }
    }

    fn ask_remove_worktree(&mut self, path: &str) -> Task<Message> {
        if self.worktree_busy {
            return Task::none();
        }
        let Some(entry) = self
            .worktree_catalogue
            .iter()
            .find(|entry| same_path(&entry.path, path))
        else {
            return Task::none();
        };
        if entry.is_main {
            self.error = Some("The main worktree cannot be removed.".to_string());
            return Task::none();
        }
        let prompt = WorktreePrompt::Remove {
            path: entry.path.clone(),
            branch: entry.branch.clone(),
            force: false,
        };
        if self.only_session_is(path) {
            self.error = Some("Open another worktree tab before removing this one.".to_string());
            return Task::none();
        }
        self.close_prompts();
        self.error = None;
        self.worktree_prompt = Some(prompt);
        Task::none()
    }

    /// True when `path` is the only open session, so removing it would leave no tab.
    fn only_session_is(&self, path: &str) -> bool {
        self.sessions.len() == 1 && same_path(&self.sessions[0].path, path)
    }

    fn confirm_create_worktree(&mut self) -> Task<Message> {
        let Some(WorktreePrompt::Create {
            path,
            branch,
            create_branch,
        }) = self.worktree_prompt.clone()
        else {
            return Task::none();
        };
        let path = path.trim().to_string();
        let branch = branch.trim().to_string();
        // A relative path would resolve against the adapter's folder for git but against the
        // process folder for the new tab, so only an absolute path is accepted.
        if self.worktree_busy
            || path.is_empty()
            || branch.is_empty()
            || !std::path::Path::new(&path).is_absolute()
        {
            return Task::none();
        }
        let Some((repo, session)) = self
            .active_session()
            .map(|session| (session.adapter.clone(), session.path.clone()))
        else {
            return Task::none();
        };
        self.worktree_busy = true;
        self.error = None;
        let created = path.clone();
        Task::perform(
            add_worktree_in_background(repo, path, branch, create_branch),
            move |result| Message::WorktreeCreated {
                session: session.clone(),
                path: created.clone(),
                result,
            },
        )
    }

    fn confirm_remove_worktree(&mut self, force: bool) -> Task<Message> {
        let Some(WorktreePrompt::Remove { path, .. }) = self.worktree_prompt.clone() else {
            return Task::none();
        };
        if self.worktree_busy {
            return Task::none();
        }
        // The catalogue may have moved while the prompt was open.
        let still_removable = self
            .worktree_catalogue
            .iter()
            .any(|entry| same_path(&entry.path, &path) && !entry.is_main);
        if !still_removable || self.only_session_is(&path) {
            self.worktree_prompt = None;
            self.error = Some("That worktree can no longer be removed here.".to_string());
            return Task::none();
        }
        // Run git from a tab other than the one being deleted: git cannot remove the folder
        // it is standing in. Prefer the active tab when it is not the target.
        let active = self.active;
        let runner = self
            .sessions
            .iter()
            .enumerate()
            .filter(|(_, session)| !same_path(&session.path, &path))
            .min_by_key(|(index, _)| usize::from(*index != active))
            .map(|(_, session)| (session.adapter.clone(), session.path.clone()));
        let Some((repo, session)) = runner else {
            return Task::none();
        };
        self.worktree_busy = true;
        self.error = None;
        let removed = path.clone();
        Task::perform(
            remove_worktree_in_background(repo, path, force),
            move |result| Message::WorktreeRemoved {
                session: session.clone(),
                path: removed.clone(),
                result,
            },
        )
    }

    /// Stop the session's spinner, then reload on success or surface git's message.
    /// A finish for a closed or unknown path is dropped.
    fn finish_reload(&mut self, path: &str, result: Result<(), DomainError>) -> Task<Message> {
        let Some(session) = self.session_mut_by_path(path) else {
            return Task::none();
        };
        session.state.loading = false;
        match result {
            Ok(()) => Task::done(Message::RefreshSession(path.to_string())),
            Err(error) => {
                self.error = Some(error_message(&error));
                Task::none()
            }
        }
    }

    /// Reload the active session. UI-triggered refresh only.
    fn begin_refresh(&mut self, clear_error: bool) -> Task<Message> {
        let Some(path) = self.active_session().map(|session| session.path.clone()) else {
            return Task::none();
        };
        self.begin_refresh_for(&path, clear_error)
    }

    /// Reload the session at `path`, whichever tab is active. An unknown path is dropped.
    fn begin_refresh_for(&mut self, path: &str, clear_error: bool) -> Task<Message> {
        let Some(session) = self.session_mut_by_path(path) else {
            return Task::none();
        };
        if session.state.loading {
            return Task::none();
        }
        let repo = session.adapter.clone();
        let path = session.path.clone();
        session.state.loading = true;
        if clear_error {
            self.error = None;
        }
        Task::perform(load_repository_in_background(repo), move |events| {
            Message::RepositoryLoaded(path.clone(), events)
        })
    }

    /// Check out `branch` in the session at `path`, whichever tab is active.
    fn checkout_for(&mut self, path: &str, branch: String) -> Task<Message> {
        let Some(session) = self.session_mut_by_path(path) else {
            return Task::none();
        };
        if session.state.loading {
            return Task::none();
        }
        let repo = session.adapter.clone();
        let path = session.path.clone();
        session.state.loading = true;
        session.selection = Some(Selection::Branch {
            name: branch.clone(),
            current: false,
        });
        self.error = None;
        Task::perform(checkout_in_background(repo, branch), move |result| {
            Message::CheckoutFinished(path.clone(), result)
        })
    }

    fn ask_branch(&mut self, ask: BranchAsk) -> Task<Message> {
        let Some(session) = self.active_session() else {
            return Task::none();
        };
        if session.state.loading {
            return Task::none();
        }
        let visible = session.shown_selection();
        let prompt = match ask {
            BranchAsk::Create => {
                let start = match &visible {
                    Selection::Branch { name, .. } if session.local_branch(name).is_some() => {
                        name.clone()
                    }
                    Selection::Remote { name } if session.remote_branch(name).is_some() => {
                        name.clone()
                    }
                    Selection::Branch { .. } => "HEAD".to_string(),
                    _ => return Task::none(),
                };
                BranchPrompt::Create {
                    start,
                    name: String::new(),
                }
            }
            BranchAsk::Rename => {
                let Selection::Branch { name, .. } = visible else {
                    return Task::none();
                };
                if session.local_branch(&name).is_none() {
                    return Task::none();
                }
                BranchPrompt::Rename {
                    to: name.clone(),
                    from: name,
                }
            }
            BranchAsk::Delete => {
                let Selection::Branch { name, current } = visible else {
                    return Task::none();
                };
                if current || session.local_branch(&name).is_none() {
                    return Task::none();
                }
                BranchPrompt::Delete { name }
            }
        };
        self.confirm_discard = None;
        self.confirm_drop = None;
        self.history_prompt = None;
        self.branch_prompt = Some(prompt);
        Task::none()
    }

    fn sync_terminal_log(&mut self) {
        self.terminal_log = text_editor::Content::with_text(&self.terminal.shell_text());
    }

    fn confirm_branch(&mut self) -> Task<Message> {
        let Some(prompt) = self.branch_prompt.clone() else {
            return Task::none();
        };
        let action = match prompt {
            BranchPrompt::Create { start, name } => {
                let name = name.trim().to_string();
                if name.is_empty() {
                    return Task::none();
                }
                if start != "HEAD" && !self.has_local_branch(&start) {
                    return self.branch_is_gone();
                }
                BranchAction::Create { name, start }
            }
            BranchPrompt::Rename { from, to } => {
                let to = to.trim().to_string();
                if to.is_empty() || to == from {
                    return Task::none();
                }
                if !self.has_local_branch(&from) {
                    return self.branch_is_gone();
                }
                BranchAction::Rename { from, to }
            }
            BranchPrompt::Delete { name } => match self.local_branch_is_head(&name) {
                Some(true) => {
                    self.branch_prompt = None;
                    self.error = Some("The current branch cannot be deleted.".to_string());
                    return Task::none();
                }
                Some(false) => BranchAction::Delete { name },
                None => return self.branch_is_gone(),
            },
        };
        self.run_branch(action)
    }

    fn branch_is_gone(&mut self) -> Task<Message> {
        self.branch_prompt = None;
        self.history_prompt = None;
        self.error = Some("That branch is no longer there.".to_string());
        Task::none()
    }

    fn has_local_branch(&self, name: &str) -> bool {
        self.active_session()
            .is_some_and(|session| session.local_branch(name).is_some())
    }

    fn local_branch_is_head(&self, name: &str) -> Option<bool> {
        self.active_session()
            .and_then(|session| session.local_branch(name).map(|branch| branch.is_head))
    }

    fn commit_is_loaded(&self, id: &str) -> bool {
        self.active_session()
            .is_some_and(|session| session.commit_loaded(id))
    }

    fn edit_marked(&mut self, stage: bool) -> Task<Message> {
        let Some(session) = self.active_session() else {
            return Task::none();
        };
        let paths = marked_paths(&session.marked_files, stage);
        if paths.is_empty() {
            Task::none()
        } else if stage {
            self.update(Message::EditWorktree(WorktreeEdit::Stage(paths)))
        } else {
            self.update(Message::EditWorktree(WorktreeEdit::Unstage(paths)))
        }
    }

    fn run_branch(&mut self, action: BranchAction) -> Task<Message> {
        let Some(session) = self.active_session() else {
            return Task::none();
        };
        if session.state.loading {
            return Task::none();
        }
        let repo = session.adapter.clone();
        let path = session.path.clone();
        let session = self.session_mut();
        session.pending_checkout = match &action {
            BranchAction::Create { name, .. } => Some(name.clone()),
            BranchAction::Rename { .. } | BranchAction::Delete { .. } => None,
        };
        session.state.loading = true;
        self.branch_prompt = None;
        self.history_prompt = None;
        self.confirm_discard = None;
        self.confirm_drop = None;
        self.error = None;
        Task::perform(branch_in_background(repo, action), move |result| {
            Message::BranchFinished(path.clone(), result)
        })
    }

    fn ask_history(&mut self, ask: HistoryAsk) -> Task<Message> {
        let Some(session) = self.active_session() else {
            return Task::none();
        };
        if session.state.loading {
            return Task::none();
        }
        let visible = session.shown_selection();
        let prompt = match ask {
            HistoryAsk::CherryPick | HistoryAsk::Reset => {
                let Selection::Commit { id } = visible else {
                    return Task::none();
                };
                if !session.commit_loaded(&id) {
                    return Task::none();
                }
                if matches!(ask, HistoryAsk::CherryPick) {
                    HistoryPrompt::CherryPick { id }
                } else {
                    HistoryPrompt::Reset { id }
                }
            }
            HistoryAsk::Merge | HistoryAsk::Rebase => {
                let Selection::Branch { name, current } = visible else {
                    return Task::none();
                };
                if current || session.local_branch(&name).is_none() {
                    return Task::none();
                }
                if matches!(ask, HistoryAsk::Merge) {
                    HistoryPrompt::Merge { name }
                } else {
                    HistoryPrompt::Rebase { name }
                }
            }
        };
        self.confirm_discard = None;
        self.confirm_drop = None;
        self.branch_prompt = None;
        self.history_prompt = Some(prompt);
        Task::none()
    }

    fn confirm_cherry_pick(&mut self) -> Task<Message> {
        let Some(HistoryPrompt::CherryPick { id }) = self.history_prompt.clone() else {
            return Task::none();
        };
        if !self.commit_is_loaded(&id) {
            return self.commit_is_gone();
        }
        self.run_history(HistoryAction::CherryPick { id })
    }

    fn confirm_reset(&mut self, mode: ResetMode) -> Task<Message> {
        let Some(HistoryPrompt::Reset { id }) = self.history_prompt.clone() else {
            return Task::none();
        };
        if !self.commit_is_loaded(&id) {
            return self.commit_is_gone();
        }
        self.run_history(HistoryAction::Reset { id, mode })
    }

    fn confirm_merge(&mut self) -> Task<Message> {
        let Some(HistoryPrompt::Merge { name }) = self.history_prompt.clone() else {
            return Task::none();
        };
        self.confirm_branch_history(name, true)
    }

    fn confirm_rebase(&mut self) -> Task<Message> {
        let Some(HistoryPrompt::Rebase { name }) = self.history_prompt.clone() else {
            return Task::none();
        };
        self.confirm_branch_history(name, false)
    }

    fn confirm_branch_history(&mut self, name: String, merge: bool) -> Task<Message> {
        match self.local_branch_is_head(&name) {
            Some(true) => {
                self.history_prompt = None;
                self.error = Some(if merge {
                    "The current branch cannot be merged.".to_string()
                } else {
                    "The current branch cannot be rebased.".to_string()
                });
                Task::none()
            }
            Some(false) => {
                if merge {
                    self.run_history(HistoryAction::Merge { name })
                } else {
                    self.run_history(HistoryAction::Rebase { name })
                }
            }
            None => self.branch_is_gone(),
        }
    }

    fn commit_is_gone(&mut self) -> Task<Message> {
        self.history_prompt = None;
        self.error = Some("That commit is no longer there.".to_string());
        Task::none()
    }

    fn run_history(&mut self, action: HistoryAction) -> Task<Message> {
        let Some(session) = self.active_session() else {
            return Task::none();
        };
        if session.state.loading {
            return Task::none();
        }
        let repo = session.adapter.clone();
        let path = session.path.clone();
        self.history_prompt = None;
        self.branch_prompt = None;
        self.confirm_discard = None;
        self.confirm_drop = None;
        self.session_mut().state.loading = true;
        self.error = None;
        Task::perform(history_in_background(repo, action), move |result| {
            Message::HistoryFinished(path.clone(), result)
        })
    }

    fn run_stash(&mut self, action: StashAction) -> Task<Message> {
        let Some(session) = self.active_session() else {
            return Task::none();
        };
        if session.state.loading {
            return Task::none();
        }
        let repo = session.adapter.clone();
        let path = session.path.clone();
        self.confirm_drop = None;
        self.confirm_discard = None;
        self.session_mut().state.loading = true;
        self.error = None;
        Task::perform(stash_in_background(repo, action), move |result| {
            Message::StashFinished(path.clone(), result)
        })
    }

    fn edit_hunk(&mut self, path: String, index: usize, stage: bool) -> Task<Message> {
        let Some(session) = self.active_session() else {
            return Task::none();
        };
        if session.state.loading {
            return Task::none();
        }
        let Some(hunk) = session.hunk_at(&path, index) else {
            return Task::none();
        };
        let repo = session.adapter.clone();
        let session_path = session.path.clone();
        let place = session
            .selection
            .as_ref()
            .and_then(|selection| match selection {
                Selection::File { place, .. } => Some(*place),
                _ => None,
            })
            .unwrap_or(FileDisposition::Unstaged);
        self.session_mut().state.loading = true;
        self.error = None;
        if stage {
            let finish_path = session_path.clone();
            Task::perform(stage_hunk_in_background(repo, path, hunk), move |result| {
                Message::HunkFinished(finish_path.clone(), result)
            })
        } else {
            Task::perform(
                discard_hunk_in_background(repo, path, hunk, place),
                move |result| Message::HunkFinished(session_path.clone(), result),
            )
        }
    }

    pub(crate) fn subscription(&self) -> iced::Subscription<Message> {
        iced::keyboard::on_key_press(shortcut_message)
    }

    fn handle_escape(&mut self) -> Task<Message> {
        if self.confirm_discard.is_some() {
            return self.update(Message::CancelDiscard);
        }
        if self.confirm_drop.is_some() {
            return self.update(Message::CancelDropStash);
        }
        if self.branch_prompt.is_some() {
            return self.update(Message::CancelBranch);
        }
        if self.history_prompt.is_some() {
            return self.update(Message::CancelHistory);
        }
        if self.worktree_prompt.is_some() {
            return self.update(Message::CancelWorktree);
        }
        if self.settings_open {
            return self.update(Message::CloseSettings);
        }
        if self.tab_picker_open {
            self.tab_picker_open = false;
            return Task::none();
        }
        if self.branch_list_open {
            self.branch_list_open = false;
            self.branch_query.clear();
            return Task::none();
        }
        if self.error.is_some() {
            return self.update(Message::ClearError);
        }
        if self
            .active_session()
            .is_some_and(|session| session.focused_diff_path.is_some())
        {
            return self.update(Message::ClearDiffFocus);
        }
        Task::none()
    }
}

/// Global shortcuts. Text inputs still receive keys; iced delivers both.
fn shortcut_message(
    key: iced::keyboard::Key,
    modifiers: iced::keyboard::Modifiers,
) -> Option<Message> {
    use iced::keyboard::key::Named;
    use iced::keyboard::Key;

    let ctrl = modifiers.control() || modifiers.logo();
    match key {
        Key::Named(Named::Escape) => Some(Message::Escape),
        Key::Named(Named::F5) => Some(Message::Fetch),
        Key::Character(c)
            if ctrl && modifiers.shift() && c.as_str().eq_ignore_ascii_case("l") =>
        {
            Some(Message::CycleLayout)
        }
        Key::Character(c)
            if ctrl && !modifiers.shift() && c.as_str().eq_ignore_ascii_case("r") =>
        {
            Some(Message::RefreshRepository)
        }
        Key::Character(c)
            if ctrl && !modifiers.shift() && c.as_str().eq_ignore_ascii_case("s") =>
        {
            Some(Message::StageMarked)
        }
        _ => None,
    }
}

#[cfg(test)]
fn unavailable_opener() -> RepositoryOpener {
    Arc::new(|_| Err(DomainError::Unknown("no repository opener".into())))
}

fn unavailable_model_lister() -> crate::ModelLister {
    Arc::new(|_| crate::ModelList {
        ids: Ok(vec![AppSettings::DEFAULT_AGENT_MODEL.to_string()]),
        exchange: crate::ProviderExchange::kilo("model list is not connected"),
    })
}

fn unavailable_suggester() -> crate::CommitSuggester {
    Arc::new(|_, _, _| crate::CommitSuggestion {
        subject: Err(DomainError::Unknown("No model is configured.".to_string())),
        exchange: crate::ProviderExchange::kilo("No model is configured."),
    })
}

#[cfg(test)]
fn ignore_settings() -> SettingsSaver {
    Arc::new(|_| {})
}

#[cfg(test)]
fn unavailable_initialiser() -> RepositoryInitialiser {
    Arc::new(|_| Err(DomainError::Unknown("no initialiser".into())))
}

#[cfg(test)]
fn unavailable_cloner() -> RepositoryCloner {
    Arc::new(|_, _| Err(DomainError::Unknown("no cloner".into())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::load_repository;
    use core_domain::{Branch, Commit, DomainError};
    use test_support::FakeGitRepository;

    fn commit(id: &str) -> Commit {
        Commit {
            id: id.to_string(),
            parent_ids: vec![],
            author: "Test Author".to_string(),
            message: format!("commit {id}"),
            timestamp: 0,
        }
    }

    fn branch(name: &str, is_head: bool) -> Branch {
        Branch {
            name: name.to_string(),
            is_remote: false,
            is_head,
            tip: String::new(),
        }
    }

    fn populated_repo() -> FakeGitRepository {
        FakeGitRepository {
            log_result: Ok(vec![commit("aaaaaaa1"), commit("bbbbbbb2")]),
            branches_result: Ok(vec![branch("main", true), branch("feature/x", false)]),
            ..FakeGitRepository::ok()
        }
    }

    #[test]
    fn repository_loaded_populates_app_state() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(populated_repo()));
        let _ = app.update(Message::RefreshRepository);
        let _ = app.update(Message::RepositoryLoaded(
            app.active_path(),
            load_repository(&populated_repo()),
        ));
        assert!(!app.session_mut().state.loading);
        assert!(app.session_mut().state.current_status.is_some());
        assert_eq!(app.session_mut().state.commits.len(), 2);
        assert_eq!(app.session_mut().state.dag_nodes.len(), 2);
        assert_eq!(
            app.session_mut().state.branches,
            vec![branch("main", true), branch("feature/x", false)]
        );
    }

    #[test]
    fn with_repo_opens_the_repository_view() {
        let app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        assert_eq!(app.sessions.len(), 1);
    }

    #[test]
    fn welcome_has_no_repository() {
        assert!(GitApp::welcome().sessions.is_empty());
    }

    #[test]
    fn open_repository_starts_loading_without_blocking() {
        let mut app = GitApp::welcome_with(
            unavailable_opener(),
            unavailable_opener(),
            unavailable_initialiser(),
            unavailable_cloner(),
            AppSettings::default(),
            ignore_settings(),
        );
        app.error = Some("previous failure".to_string());

        let _ = app.update(Message::OpenRepository(r"D:\repo".to_string()));

        assert!(app.opening);
        assert!(app.sessions.is_empty());
        assert_eq!(app.error, None);
    }

    #[test]
    fn repository_opened_replaces_the_welcome_screen() {
        let mut app = GitApp::welcome();
        app.opening = true;

        let _ = app.update(Message::RepositoryOpened(
            r"D:\repo".to_string(),
            Ok(OpenedRepository(Arc::new(FakeGitRepository::ok()))),
        ));

        assert!(!app.sessions.is_empty());
        assert_eq!(app.error, None);
        assert_eq!(app.session().path, r"D:\repo");
    }

    #[test]
    fn opening_a_repository_keeps_the_kilo_connection() {
        let mut app = GitApp::welcome();
        app.list_models = Arc::new(|_| crate::ModelList {
            ids: Ok(vec!["kilo-auto/free".to_string()]),
            exchange: crate::ProviderExchange::kilo("connected"),
        });
        app.opening = true;

        let _ = app.update(Message::RepositoryOpened(
            r"D:\repo".to_string(),
            Ok(OpenedRepository(Arc::new(FakeGitRepository::ok()))),
        ));

        let listed = (app.list_models)(String::new());
        assert_eq!(listed.exchange.text, "connected");
        assert!(!app.sessions.is_empty());
    }

    struct ObservedSplit {
        axis: pane_grid::Axis,
        ratio: f32,
        left: Vec<WorkspacePane>,
        right: Vec<WorkspacePane>,
        id: pane_grid::Split,
    }

    fn splits_of(state: &pane_grid::State<WorkspacePane>) -> Vec<ObservedSplit> {
        fn panes(
            node: &pane_grid::Node,
            state: &pane_grid::State<WorkspacePane>,
        ) -> Vec<WorkspacePane> {
            match node {
                pane_grid::Node::Split { a, b, .. } => {
                    let mut all = panes(a, state);
                    all.extend(panes(b, state));
                    all
                }
                pane_grid::Node::Pane(pane) => vec![*state.panes.get(pane).expect("pane")],
            }
        }
        fn walk(
            node: &pane_grid::Node,
            state: &pane_grid::State<WorkspacePane>,
            out: &mut Vec<ObservedSplit>,
        ) {
            if let pane_grid::Node::Split {
                axis,
                ratio,
                a,
                b,
                id,
            } = node
            {
                out.push(ObservedSplit {
                    axis: *axis,
                    ratio: *ratio,
                    left: panes(a, state),
                    right: panes(b, state),
                    id: *id,
                });
                walk(a, state, out);
                walk(b, state, out);
            }
        }
        let mut out = Vec::new();
        walk(state.layout(), state, &mut out);
        out
    }

    fn split_between(
        state: &pane_grid::State<WorkspacePane>,
        left: &[WorkspacePane],
        right: &[WorkspacePane],
    ) -> (pane_grid::Axis, f32, pane_grid::Split) {
        let found: Vec<_> = splits_of(state)
            .into_iter()
            .filter(|split| split.left.as_slice() == left && split.right.as_slice() == right)
            .collect();
        assert_eq!(found.len(), 1, "expected one split {left:?} | {right:?}");
        (found[0].axis, found[0].ratio, found[0].id)
    }

    fn assert_open_preset(app: &GitApp, preset: LayoutPreset) {
        let ratios = app.ratios[preset.index()];
        let outer = match preset {
            LayoutPreset::Columns | LayoutPreset::InspectorOverDiff => pane_grid::Axis::Vertical,
            LayoutPreset::Stacked => pane_grid::Axis::Horizontal,
        };
        let middle = match preset {
            LayoutPreset::Columns => pane_grid::Axis::Vertical,
            LayoutPreset::InspectorOverDiff | LayoutPreset::Stacked => pane_grid::Axis::Horizontal,
        };
        let (files_axis, files_ratio, _) = split_between(
            &app.panes,
            &[WorkspacePane::Files],
            &[
                WorkspacePane::Graph,
                WorkspacePane::Diff,
                WorkspacePane::Inspector,
            ],
        );
        let (graph_axis, graph_ratio, middle_axis, middle_ratio) = match preset {
            LayoutPreset::Columns => {
                let (graph_axis, graph_ratio, _) = split_between(
                    &app.panes,
                    &[WorkspacePane::Graph],
                    &[WorkspacePane::Diff, WorkspacePane::Inspector],
                );
                let (middle_axis, middle_ratio, _) = split_between(
                    &app.panes,
                    &[WorkspacePane::Diff],
                    &[WorkspacePane::Inspector],
                );
                (graph_axis, graph_ratio, middle_axis, middle_ratio)
            }
            LayoutPreset::InspectorOverDiff | LayoutPreset::Stacked => {
                let (graph_axis, graph_ratio, _) = split_between(
                    &app.panes,
                    &[WorkspacePane::Graph, WorkspacePane::Diff],
                    &[WorkspacePane::Inspector],
                );
                let (middle_axis, middle_ratio, _) = split_between(
                    &app.panes,
                    &[WorkspacePane::Graph],
                    &[WorkspacePane::Diff],
                );
                (graph_axis, graph_ratio, middle_axis, middle_ratio)
            }
        };
        assert_eq!(files_axis, outer);
        assert_eq!(files_ratio, ratios.files);
        assert_eq!(graph_axis, outer);
        assert_eq!(graph_ratio, ratios.graph);
        assert_eq!(middle_axis, middle);
        assert_eq!(middle_ratio, ratios.middle);
        assert_eq!(splits_of(&app.panes).len(), 3);
    }

    #[test]
    fn inspector_over_diff_is_the_default_layout() {
        let app = GitApp::with_repo(Arc::new(FakeGitRepository::ok()));
        assert_eq!(app.layout, LayoutPreset::InspectorOverDiff);
        assert!(app.files_rail_open);
        assert_eq!(layout::GRAPH_RATIO, 0.68);
        assert_eq!(layout::COLUMNS_MIDDLE_RATIO, 0.50);
        assert_eq!(layout::STACKED_MIDDLE_RATIO, 0.48);
        assert_eq!(layout::FILES_RATIO, 0.22);
        assert_eq!(layout::MIN_GRAPH_RATIO, 0.50);
        assert_open_preset(&app, LayoutPreset::InspectorOverDiff);
    }

    #[test]
    fn columns_place_the_four_panes_side_by_side() {
        let mut app = GitApp::with_repo(Arc::new(FakeGitRepository::ok()));
        let _ = app.update(Message::SetLayout(LayoutPreset::Columns));
        assert_eq!(app.layout, LayoutPreset::Columns);
        assert_open_preset(&app, LayoutPreset::Columns);
    }

    #[test]
    fn stacked_places_every_pane_on_the_horizontal_axis() {
        let mut app = GitApp::with_repo(Arc::new(FakeGitRepository::ok()));
        let _ = app.update(Message::SetLayout(LayoutPreset::Stacked));
        assert_eq!(app.layout, LayoutPreset::Stacked);
        assert_open_preset(&app, LayoutPreset::Stacked);
    }

    #[test]
    fn cycle_layout_walks_the_three_presets() {
        let mut app = GitApp::with_repo(Arc::new(FakeGitRepository::ok()));
        assert_eq!(app.layout, LayoutPreset::InspectorOverDiff);
        let _ = app.update(Message::CycleLayout);
        assert_eq!(app.layout, LayoutPreset::Stacked);
        let _ = app.update(Message::CycleLayout);
        assert_eq!(app.layout, LayoutPreset::Columns);
        let _ = app.update(Message::CycleLayout);
        assert_eq!(app.layout, LayoutPreset::InspectorOverDiff);
    }

    #[test]
    fn escape_clears_an_error_then_does_nothing() {
        let mut app = GitApp::with_repo(Arc::new(FakeGitRepository::ok()));
        app.error = Some("boom".into());
        app.error_expanded = true;
        let _ = app.update(Message::Escape);
        assert!(app.error.is_none());
        assert!(!app.error_expanded);
        let _ = app.update(Message::Escape);
        assert!(app.error.is_none());
    }

    #[test]
    fn resizing_the_middle_split_changes_only_that_presets_ratio() {
        let mut app = GitApp::with_repo(Arc::new(FakeGitRepository::ok()));
        let middle = app
            .split_slots
            .iter()
            .find(|(_, slot)| *slot == RatioSlot::Middle)
            .map(|(split, _)| *split)
            .expect("middle split");

        let _ = app.update(Message::PaneResized(pane_grid::ResizeEvent {
            split: middle,
            ratio: 0.55,
        }));

        assert_eq!(
            app.ratios[LayoutPreset::InspectorOverDiff.index()].middle,
            0.55
        );
        assert_eq!(
            app.ratios[LayoutPreset::InspectorOverDiff.index()].graph,
            layout::GRAPH_RATIO
        );
        assert_eq!(
            app.ratios[LayoutPreset::InspectorOverDiff.index()].files,
            layout::FILES_RATIO
        );
        assert_eq!(
            app.ratios[LayoutPreset::Columns.index()].middle,
            layout::COLUMNS_MIDDLE_RATIO
        );
        assert_eq!(
            app.ratios[LayoutPreset::Stacked.index()].middle,
            layout::STACKED_MIDDLE_RATIO
        );
        let (_, middle_ratio, _) = split_between(
            &app.panes,
            &[WorkspacePane::Graph],
            &[WorkspacePane::Diff],
        );
        assert_eq!(middle_ratio, 0.55);

        let _ = app.update(Message::SetLayout(LayoutPreset::Columns));
        let (_, columns_middle, _) = split_between(
            &app.panes,
            &[WorkspacePane::Diff],
            &[WorkspacePane::Inspector],
        );
        assert_eq!(columns_middle, layout::COLUMNS_MIDDLE_RATIO);

        let _ = app.update(Message::SetLayout(LayoutPreset::InspectorOverDiff));
        let (_, restored, _) = split_between(
            &app.panes,
            &[WorkspacePane::Graph],
            &[WorkspacePane::Diff],
        );
        assert_eq!(restored, 0.55);
    }

    #[test]
    fn resizing_the_graph_split_respects_the_minimum_width() {
        let mut app = GitApp::with_repo(Arc::new(FakeGitRepository::ok()));
        let graph = mapped_slot(&app, RatioSlot::Graph);

        let _ = app.update(Message::PaneResized(pane_grid::ResizeEvent {
            split: graph,
            ratio: 0.20,
        }));

        assert_eq!(
            app.ratios[LayoutPreset::InspectorOverDiff.index()].graph,
            layout::MIN_GRAPH_RATIO
        );
        let (_, graph_ratio, _) = split_between(
            &app.panes,
            &[WorkspacePane::Graph, WorkspacePane::Diff],
            &[WorkspacePane::Inspector],
        );
        assert_eq!(graph_ratio, layout::MIN_GRAPH_RATIO);
    }

    #[test]
    fn toggling_the_files_rail_hides_and_restores_it() {
        let mut app = GitApp::with_repo(Arc::new(FakeGitRepository::ok()));
        app.ratios[LayoutPreset::InspectorOverDiff.index()].files = 0.30;
        assert!(app.files_rail_open);

        let _ = app.update(Message::ToggleFilesRail);
        assert!(!app.files_rail_open);
        let (graph_axis, _, graph_split) = split_between(
            &app.panes,
            &[WorkspacePane::Graph, WorkspacePane::Diff],
            &[WorkspacePane::Inspector],
        );
        assert_eq!(graph_axis, pane_grid::Axis::Vertical);
        let (_, _, middle_split) = split_between(
            &app.panes,
            &[WorkspacePane::Graph],
            &[WorkspacePane::Diff],
        );
        assert!(splits_of(&app.panes).iter().all(|split| {
            !split.left.contains(&WorkspacePane::Files)
                && !split.right.contains(&WorkspacePane::Files)
        }));
        assert_eq!(mapped_slot(&app, RatioSlot::Graph), graph_split);
        assert_eq!(mapped_slot(&app, RatioSlot::Middle), middle_split);
        assert!(app
            .split_slots
            .iter()
            .all(|(_, slot)| *slot != RatioSlot::Files));

        let _ = app.update(Message::SetLayout(LayoutPreset::Stacked));
        assert!(!app.files_rail_open);
        let (stacked_axis, _, _) = split_between(
            &app.panes,
            &[WorkspacePane::Graph, WorkspacePane::Diff],
            &[WorkspacePane::Inspector],
        );
        assert_eq!(stacked_axis, pane_grid::Axis::Horizontal);
        assert!(splits_of(&app.panes).iter().all(|split| {
            !split.left.contains(&WorkspacePane::Files)
                && !split.right.contains(&WorkspacePane::Files)
        }));

        let _ = app.update(Message::ToggleFilesRail);
        assert!(app.files_rail_open);
        assert_eq!(app.layout, LayoutPreset::Stacked);
        assert_eq!(
            app.ratios[LayoutPreset::Stacked.index()].files,
            layout::FILES_RATIO
        );
        assert_eq!(
            app.ratios[LayoutPreset::InspectorOverDiff.index()].files,
            0.30
        );
        assert_open_preset(&app, LayoutPreset::Stacked);
    }

    fn mapped_slot(app: &GitApp, slot: RatioSlot) -> pane_grid::Split {
        app.split_slots
            .iter()
            .find(|(_, found)| *found == slot)
            .map(|(split, _)| *split)
            .expect("slot")
    }

    #[test]
    fn choosing_another_branch_checks_it_out() {
        let mut app = GitApp::with_repo(Arc::new(FakeGitRepository::ok()));
        app.session_mut().state.branches = vec![branch("dev", true), branch("feature", false)];
        app.branch_list_open = true;
        app.branch_query = "feat".to_string();

        let _ = app.update(Message::ChooseBranch("feature".to_string()));

        assert!(!app.branch_list_open);
        assert!(app.branch_query.is_empty());
        assert!(app.session_mut().state.loading);
        assert_eq!(
            app.session_mut().selection,
            Some(Selection::Branch {
                name: "feature".to_string(),
                current: false,
            })
        );
    }

    #[test]
    fn choosing_the_current_branch_only_closes_the_list() {
        let mut app = GitApp::with_repo(Arc::new(FakeGitRepository::ok()));
        app.session_mut().state.branches = vec![branch("dev", true), branch("feature", false)];
        app.branch_list_open = true;

        let _ = app.update(Message::ChooseBranch("dev".to_string()));

        assert!(!app.branch_list_open);
        assert!(!app.session_mut().state.loading);
        assert_eq!(
            app.session_mut().selection,
            Some(Selection::Branch {
                name: "dev".to_string(),
                current: true,
            })
        );
    }

    #[test]
    fn creating_a_branch_then_switches_to_it() {
        let mut app = on_branch("main", true);
        app.branch_prompt = Some(BranchPrompt::Create {
            start: "main".to_string(),
            name: "topic".to_string(),
        });

        let _ = app.update(Message::ConfirmBranch);
        assert!(app.session_mut().state.loading);
        assert_eq!(app.session().pending_checkout.as_deref(), Some("topic"));

        let _ = app.update(Message::BranchFinished(app.active_path(), Ok(())));
        assert!(app.session().pending_checkout.is_none());
        assert!(app.session_mut().state.loading);
        assert_eq!(
            app.session_mut().selection,
            Some(Selection::Branch {
                name: "topic".to_string(),
                current: false,
            })
        );
    }

    #[test]
    fn creating_from_a_remote_uses_that_ref_as_the_start() {
        let mut app = GitApp::with_repo(Arc::new(FakeGitRepository::ok()));
        app.session_mut().state.branches = vec![
            branch("main", true),
            Branch {
                name: "origin/main".to_string(),
                is_remote: true,
                is_head: false,
                tip: String::new(),
            },
        ];
        app.session_mut().selection = Some(Selection::Remote {
            name: "origin/main".to_string(),
        });

        let _ = app.update(Message::AskCreateBranch);

        assert_eq!(
            app.branch_prompt,
            Some(BranchPrompt::Create {
                start: "origin/main".to_string(),
                name: String::new(),
            })
        );
    }

    #[test]
    fn suggest_without_a_file_explains_why() {
        let mut app = GitApp::with_repo(Arc::new(FakeGitRepository::ok()));
        let _ = app.update(Message::SuggestCommit);
        assert!(!app.agent_busy);
        assert_eq!(app.error.as_deref(), Some("Select a file first."));
        assert!(app.session_mut().commit_message.is_empty());
    }

    #[test]
    fn the_agent_token_is_kept_on_the_settings() {
        let mut app = GitApp::with_repo(Arc::new(FakeGitRepository::ok()));
        let _ = app.update(Message::AgentTokenChanged("kilo-token".to_string()));
        assert_eq!(app.session_mut().state.settings.agent_token, "kilo-token");
        assert_eq!(
            app.session_mut().state.settings.agent_model,
            AppSettings::DEFAULT_AGENT_MODEL
        );
    }

    #[test]
    fn select_all_marks_every_change_and_clear_drops_them() {
        let mut app = GitApp::with_repo(Arc::new(FakeGitRepository::ok()));
        app.session_mut().state.current_status = Some(core_domain::RepoStatus {
            staged_files: vec!["a.rs".to_string()],
            unstaged_files: vec!["b.rs".to_string()],
            untracked_files: vec![],
            tracking: None,
        });

        let _ = app.update(Message::SelectAllFiles);
        assert_eq!(app.session_mut().marked_files.len(), 2);

        let _ = app.update(Message::ToggleFileMark {
            path: "b.rs".to_string(),
            place: FileDisposition::Unstaged,
        });
        assert_eq!(app.session_mut().marked_files.len(), 1);

        let _ = app.update(Message::ClearFileSelection);
        assert!(app.session_mut().marked_files.is_empty());
        assert!(matches!(app.session_mut().selection, Some(Selection::Branch { .. })));
    }

    #[test]
    fn repository_open_failure_shows_an_error_on_the_welcome_screen() {
        let mut app = GitApp::welcome();
        app.opening = true;

        let _ = app.update(Message::RepositoryOpened(
            r"D:\repo".to_string(),
            Err(DomainError::Unknown("cannot open that folder".into())),
        ));

        assert!(app.sessions.is_empty());
        assert!(!app.opening);
        assert_eq!(app.error.as_deref(), Some("cannot open that folder"));
    }

    #[test]
    fn blank_repository_path_does_not_call_the_opener() {
        let open: RepositoryOpener =
            Arc::new(|_| Err(DomainError::Unknown("opener was called".into())));
        let mut app = GitApp::welcome_with(
            open,
            unavailable_opener(),
            unavailable_initialiser(),
            unavailable_cloner(),
            AppSettings::default(),
            ignore_settings(),
        );

        let _ = app.update(Message::OpenRepository("  ".to_string()));

        assert!(app.sessions.is_empty());
        assert_eq!(app.error.as_deref(), Some("Choose a repository folder."));
    }

    #[test]
    fn cancelled_folder_dialog_leaves_the_welcome_screen() {
        let open: RepositoryOpener =
            Arc::new(|_| Err(DomainError::Unknown("opener was called".into())));
        let mut app = GitApp::welcome_with(
            open,
            unavailable_opener(),
            unavailable_initialiser(),
            unavailable_cloner(),
            AppSettings::default(),
            ignore_settings(),
        );

        let _ = app.update(Message::RepositoryFolderChosen(None));

        assert!(app.sessions.is_empty());
        assert_eq!(app.error, None);
    }

    #[test]
    fn open_repository_is_ignored_while_another_operation_runs() {
        let open: RepositoryOpener =
            Arc::new(|_| Err(DomainError::Unknown("opener was called".into())));
        let mut app = GitApp::welcome_with(
            open,
            unavailable_opener(),
            unavailable_initialiser(),
            unavailable_cloner(),
            AppSettings::default(),
            ignore_settings(),
        );
        app.opening = true;
        app.error = Some("still showing".to_string());

        let _ = app.update(Message::OpenRepository(r"D:\repo".to_string()));

        assert!(app.sessions.is_empty());
        assert_eq!(app.error.as_deref(), Some("still showing"));
    }

    #[test]
    fn refresh_marks_state_as_loading() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        let _ = app.update(Message::RefreshRepository);
        assert!(app.session_mut().state.loading);
    }

    #[test]
    fn refresh_on_the_welcome_screen_does_nothing() {
        let mut app = GitApp::welcome();
        let _ = app.update(Message::RefreshRepository);
        assert!(app.sessions.is_empty());
    }

    #[test]
    fn refresh_errors_are_shown() {
        let missing = FakeGitRepository {
            status_result: Err(DomainError::RepositoryNotFound),
            ..FakeGitRepository::ok()
        };
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        let _ = app.update(Message::RepositoryLoaded(app.active_path(), load_repository(&missing)));
        assert_eq!(app.error.as_deref(), Some("Not a Git repository."));
    }

    #[test]
    fn checkout_on_the_welcome_screen_does_nothing() {
        let mut app = GitApp::welcome();
        let _ = app.update(Message::CheckoutBranch("main".to_string()));
        assert!(app.sessions.is_empty());
    }

    #[test]
    fn checkout_starts_loading_and_clears_the_last_error() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.error = Some("previous failure".to_string());
        let _ = app.update(Message::CheckoutBranch("feature".to_string()));
        assert!(app.session_mut().state.loading);
        assert_eq!(app.error, None);
    }

    #[test]
    fn checkout_is_ignored_while_another_operation_runs() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.session_mut().state.loading = true;
        app.error = Some("still showing".to_string());
        let _ = app.update(Message::CheckoutBranch("feature".to_string()));
        assert_eq!(app.error.as_deref(), Some("still showing"));
    }

    #[test]
    fn successful_checkout_stops_loading_without_an_error() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.session_mut().state.loading = true;
        let _ = app.update(Message::CheckoutFinished(app.active_path(), Ok(())));
        assert!(!app.session_mut().state.loading);
        assert_eq!(app.error, None);
    }

    #[test]
    fn failed_checkout_shows_why() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.session_mut().state.loading = true;
        let reason = "error: Your local changes would be overwritten by checkout".to_string();
        let _ = app.update(Message::CheckoutFinished(app.active_path(), Err(DomainError::Unknown(
            reason.clone(),
        ))));
        assert!(!app.session_mut().state.loading);
        assert_eq!(app.error, Some(reason));
    }

    #[test]
    fn fetch_on_the_welcome_screen_does_nothing() {
        let mut app = GitApp::welcome();
        let _ = app.update(Message::Fetch);
        assert!(app.sessions.is_empty());
    }

    #[test]
    fn fetch_starts_loading_and_clears_the_last_error() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.error = Some("previous failure".to_string());
        let _ = app.update(Message::Fetch);
        assert!(app.session_mut().state.loading);
        assert_eq!(app.error, None);
    }

    #[test]
    fn fetch_is_ignored_while_another_operation_runs() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.session_mut().state.loading = true;
        app.error = Some("still showing".to_string());
        let _ = app.update(Message::Fetch);
        assert_eq!(app.error.as_deref(), Some("still showing"));
    }

    #[test]
    fn successful_fetch_stops_loading_without_an_error() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.session_mut().state.loading = true;
        let _ = app.update(Message::FetchFinished(app.active_path(), Ok(())));
        assert!(!app.session_mut().state.loading);
        assert_eq!(app.error, None);
    }

    #[test]
    fn failed_fetch_shows_git_message() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.session_mut().state.loading = true;
        let reason = "fatal: No remote repository specified.".to_string();
        let _ = app.update(Message::FetchFinished(app.active_path(), Err(DomainError::Unknown(
            reason.clone(),
        ))));
        assert!(!app.session_mut().state.loading);
        assert_eq!(app.error, Some(reason));
    }

    #[test]
    fn push_on_the_welcome_screen_does_nothing() {
        let mut app = GitApp::welcome();
        let _ = app.update(Message::Push);
        assert!(app.sessions.is_empty());
    }

    #[test]
    fn push_starts_loading_and_clears_the_last_error() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.error = Some("previous failure".to_string());
        let _ = app.update(Message::Push);
        assert!(app.session_mut().state.loading);
        assert_eq!(app.error, None);
    }

    #[test]
    fn push_is_ignored_while_another_operation_runs() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.session_mut().state.loading = true;
        app.error = Some("still showing".to_string());
        let _ = app.update(Message::Push);
        assert_eq!(app.error.as_deref(), Some("still showing"));
    }

    #[test]
    fn successful_push_stops_loading_without_an_error() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.session_mut().state.loading = true;
        let _ = app.update(Message::PushFinished(app.active_path(), Ok(())));
        assert!(!app.session_mut().state.loading);
        assert_eq!(app.error, None);
    }

    #[test]
    fn failed_push_shows_git_message() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.session_mut().state.loading = true;
        let reason = "fatal: The current branch main has no upstream branch.".to_string();
        let _ = app.update(Message::PushFinished(app.active_path(), Err(DomainError::Unknown(
            reason.clone(),
        ))));
        assert!(!app.session_mut().state.loading);
        assert_eq!(app.error, Some(reason));
    }

    #[test]
    fn pull_on_the_welcome_screen_does_nothing() {
        let mut app = GitApp::welcome();
        let _ = app.update(Message::Pull);
        assert!(app.sessions.is_empty());
    }

    #[test]
    fn pull_starts_loading_and_clears_the_last_error() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.error = Some("previous failure".to_string());
        let _ = app.update(Message::Pull);
        assert!(app.session_mut().state.loading);
        assert_eq!(app.error, None);
    }

    #[test]
    fn pull_is_ignored_while_another_operation_runs() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.session_mut().state.loading = true;
        app.error = Some("still showing".to_string());
        let _ = app.update(Message::Pull);
        assert_eq!(app.error.as_deref(), Some("still showing"));
    }

    #[test]
    fn successful_pull_stops_loading_without_an_error() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.session_mut().state.loading = true;
        let _ = app.update(Message::PullFinished(app.active_path(), Ok(())));
        assert!(!app.session_mut().state.loading);
        assert_eq!(app.error, None);
    }

    #[test]
    fn failed_pull_shows_git_message() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.session_mut().state.loading = true;
        let reason = "fatal: Not possible to fast-forward, aborting.".to_string();
        let _ = app.update(Message::PullFinished(app.active_path(), Err(DomainError::Unknown(
            reason.clone(),
        ))));
        assert!(!app.session_mut().state.loading);
        assert_eq!(app.error, Some(reason));
    }

    #[test]
    fn edit_on_the_welcome_screen_does_nothing() {
        let mut app = GitApp::welcome();
        let _ = app.update(Message::EditWorktree(WorktreeEdit::Stage(vec![
            "a.txt".to_string()
        ])));
        assert!(app.sessions.is_empty());
    }

    #[test]
    fn edit_starts_loading_and_clears_the_last_error() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.error = Some("previous failure".to_string());
        let _ = app.update(Message::EditWorktree(WorktreeEdit::Stage(vec![
            "a.txt".to_string()
        ])));
        assert!(app.session_mut().state.loading);
        assert_eq!(app.error, None);
    }

    #[test]
    fn edit_is_ignored_while_another_operation_runs() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.session_mut().state.loading = true;
        app.error = Some("still showing".to_string());
        let _ = app.update(Message::EditWorktree(WorktreeEdit::Unstage(vec![
            "a.txt".to_string()
        ])));
        assert_eq!(app.error.as_deref(), Some("still showing"));
    }

    #[test]
    fn whitespace_commit_message_does_not_start_an_edit() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.error = Some("previous failure".to_string());
        let _ = app.update(Message::EditWorktree(WorktreeEdit::Commit(
            "  \t".to_string(),
        )));
        assert!(!app.session_mut().state.loading);
        assert_eq!(app.error.as_deref(), Some("previous failure"));
    }

    #[test]
    fn successful_commit_clears_the_message_and_stops_loading() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.session_mut().state.loading = true;
        app.session_mut().commit_message = "record the change".to_string();
        let _ = app.update(Message::WorktreeEditFinished(
            app.active_path(),
            WorktreeEdit::Commit("record the change".to_string()),
            Ok(()),
        ));
        assert!(!app.session_mut().state.loading);
        assert_eq!(app.error, None);
        assert_eq!(app.session_mut().commit_message, "");
    }

    #[test]
    fn successful_stage_keeps_the_commit_message() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.session_mut().state.loading = true;
        app.session_mut().commit_message = "still drafting".to_string();
        let _ = app.update(Message::WorktreeEditFinished(
            app.active_path(),
            WorktreeEdit::Stage(vec!["a.txt".to_string()]),
            Ok(()),
        ));
        assert!(!app.session_mut().state.loading);
        assert_eq!(app.session_mut().commit_message, "still drafting");
    }

    #[test]
    fn failed_edit_shows_why() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.session_mut().state.loading = true;
        let reason = "error: hook declined to update".to_string();
        let _ = app.update(Message::WorktreeEditFinished(
            app.active_path(),
            WorktreeEdit::Commit("record the change".to_string()),
            Err(DomainError::Unknown(reason.clone())),
        ));
        assert!(!app.session_mut().state.loading);
        assert_eq!(app.error, Some(reason));
    }

    #[test]
    fn commit_message_changes_are_kept() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        let _ = app.update(Message::CommitMessageChanged(
            "record the change".to_string(),
        ));
        assert_eq!(app.session_mut().commit_message, "record the change");
        assert!(!app.session_mut().state.loading);
    }

    #[test]
    fn a_blank_init_path_does_not_start() {
        let mut app = GitApp::welcome();
        let _ = app.update(Message::InitRepository("  ".to_string()));
        assert!(!app.opening);
        assert!(app.sessions.is_empty());
        assert!(app.error.is_some());
    }

    #[test]
    fn init_failure_stays_on_the_welcome_screen() {
        let mut app = GitApp::welcome();
        let _ = app.update(Message::InitRepository(r"D:\fresh".to_string()));
        assert!(app.opening);
        let _ = app.update(Message::InitFinished(
            r"D:\fresh".to_string(),
            Err(DomainError::Unknown(
                "Already a Git repository.".to_string(),
            )),
        ));
        assert!(app.sessions.is_empty());
        assert!(!app.opening);
        assert_eq!(app.error.as_deref(), Some("Already a Git repository."));
    }

    #[test]
    fn init_then_opens_the_repository() {
        let mut app = GitApp::welcome();
        let _ = app.update(Message::InitFinished(r"D:\fresh".to_string(), Ok(())));
        assert!(app.opening);
        assert!(app.sessions.is_empty());
        let _ = app.update(Message::RepositoryOpened(
            r"D:\fresh".to_string(),
            Ok(OpenedRepository(Arc::new(FakeGitRepository::ok()))),
        ));
        assert!(!app.sessions.is_empty());
        assert_eq!(app.error, None);
    }

    #[test]
    fn an_empty_clone_url_does_not_start() {
        let mut app = GitApp::welcome();
        let _ = app.update(Message::ChooseCloneFolder);
        assert!(!app.opening);
        assert!(app.sessions.is_empty());
    }

    #[test]
    fn clone_failure_stays_on_the_welcome_screen() {
        let mut app = GitApp::welcome();
        let _ = app.update(Message::CloneFolderChosen(
            "https://example.com/repo.git".to_string(),
            Some(r"D:\parent".to_string()),
        ));
        assert!(app.opening);
        let _ = app.update(Message::CloneFinished(Err(DomainError::Unknown(
            "repo already exists in D:\\parent".to_string(),
        ))));
        assert!(app.sessions.is_empty());
        assert!(!app.opening);
        assert_eq!(
            app.error.as_deref(),
            Some("repo already exists in D:\\parent")
        );
    }

    #[test]
    fn clone_then_opens_the_created_directory() {
        let mut app = GitApp::welcome();
        let _ = app.update(Message::CloneFinished(Ok(r"D:\parent\repo".to_string())));
        assert!(app.opening);
        let _ = app.update(Message::RepositoryOpened(
            r"D:\parent\repo".to_string(),
            Ok(OpenedRepository(Arc::new(FakeGitRepository::ok()))),
        ));
        assert!(!app.sessions.is_empty());
        assert_eq!(app.session().path, r"D:\parent\repo");
    }

    #[test]
    fn stash_push_starts_loading_and_is_ignored_while_busy() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        let _ = app.update(Message::StashPush);
        assert!(app.session_mut().state.loading);
        app.error = Some("still showing".to_string());
        let _ = app.update(Message::StashApply(0));
        assert_eq!(app.error.as_deref(), Some("still showing"));
    }

    #[test]
    fn dropping_a_stash_waits_for_confirm() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.session_mut().state.stashes = vec![Stash {
            index: 0,
            message: "WIP on dev".to_string(),
        }];
        app.session_mut().selection = Some(Selection::Stash {
            index: 0,
            message: "WIP on dev".to_string(),
        });

        let _ = app.update(Message::AskDropStash);
        assert_eq!(app.confirm_drop.as_ref().map(|stash| stash.index), Some(0));
        assert!(!app.session_mut().state.loading);

        let _ = app.update(Message::ConfirmDropStash);
        assert!(app.confirm_drop.is_none());
        assert!(app.session_mut().state.loading);
    }

    #[test]
    fn drop_refuses_a_stash_whose_slot_moved() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.confirm_drop = Some(Stash {
            index: 0,
            message: "WIP on dev".to_string(),
        });
        app.session_mut().state.stashes = vec![Stash {
            index: 0,
            message: "On main: other".to_string(),
        }];

        let _ = app.update(Message::ConfirmDropStash);
        assert!(!app.session_mut().state.loading);
        assert!(app.confirm_drop.is_none());
        assert_eq!(app.error.as_deref(), Some("That stash is no longer there."));
    }

    fn on_branch(name: &str, current: bool) -> GitApp {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.session_mut().state.branches = vec![branch(name, current)];
        app.session_mut().selection = Some(Selection::Branch {
            name: name.to_string(),
            current,
        });
        app
    }

    #[test]
    fn creating_a_branch_waits_for_a_name() {
        let mut app = on_branch("main", true);
        let _ = app.update(Message::AskCreateBranch);
        assert_eq!(
            app.branch_prompt,
            Some(BranchPrompt::Create {
                start: "main".to_string(),
                name: String::new(),
            })
        );
        assert!(!app.session_mut().state.loading);

        let _ = app.update(Message::ConfirmBranch);
        assert!(!app.session_mut().state.loading);

        let _ = app.update(Message::BranchNameChanged(" topic ".to_string()));
        let _ = app.update(Message::ConfirmBranch);
        assert!(app.branch_prompt.is_none());
        assert!(app.session_mut().state.loading);
    }

    #[test]
    fn renaming_keeps_the_old_name_until_it_changes() {
        let mut app = on_branch("feature", false);
        let _ = app.update(Message::AskRenameBranch);
        assert_eq!(
            app.branch_prompt,
            Some(BranchPrompt::Rename {
                from: "feature".to_string(),
                to: "feature".to_string(),
            })
        );
        let _ = app.update(Message::ConfirmBranch);
        assert!(!app.session_mut().state.loading);

        let _ = app.update(Message::BranchNameChanged("feature-2".to_string()));
        let _ = app.update(Message::ConfirmBranch);
        assert!(app.session_mut().state.loading);
        assert!(app.branch_prompt.is_none());
    }

    #[test]
    fn deleting_the_current_branch_does_not_open() {
        let mut app = on_branch("main", true);
        let _ = app.update(Message::AskDeleteBranch);
        assert!(app.branch_prompt.is_none());
        assert!(!app.session_mut().state.loading);
    }

    #[test]
    fn deleting_another_branch_waits_for_confirm() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.session_mut().state.branches = vec![branch("main", true), branch("feature", false)];
        app.session_mut().selection = Some(Selection::Branch {
            name: "feature".to_string(),
            current: false,
        });
        let _ = app.update(Message::AskDeleteBranch);
        assert_eq!(
            app.branch_prompt,
            Some(BranchPrompt::Delete {
                name: "feature".to_string(),
            })
        );
        let _ = app.update(Message::ConfirmBranch);
        assert!(app.session_mut().state.loading);
        assert!(app.branch_prompt.is_none());
    }

    #[test]
    fn delete_refuses_a_branch_that_became_current() {
        let mut app = on_branch("main", true);
        app.branch_prompt = Some(BranchPrompt::Delete {
            name: "main".to_string(),
        });
        let _ = app.update(Message::ConfirmBranch);
        assert!(!app.session_mut().state.loading);
        assert!(app.branch_prompt.is_none());
        assert_eq!(
            app.error.as_deref(),
            Some("The current branch cannot be deleted.")
        );
    }

    #[test]
    fn create_refuses_a_start_point_that_disappeared() {
        let mut app = on_branch("main", true);
        app.branch_prompt = Some(BranchPrompt::Create {
            start: "feature".to_string(),
            name: "topic".to_string(),
        });
        let _ = app.update(Message::ConfirmBranch);
        assert!(!app.session_mut().state.loading);
        assert_eq!(
            app.error.as_deref(),
            Some("That branch is no longer there.")
        );
    }

    #[test]
    fn selecting_a_tag_loads_the_commit_it_points_at() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        let _ = app.update(Message::Select(Selection::Tag {
            name: "v1".to_string(),
            commit_id: "abc1234".to_string(),
        }));
        assert!(app.session_mut().patch_pending);
        assert!(app.session_mut().patches.is_empty());
    }

    #[test]
    fn a_missing_tag_is_cleared_when_the_repository_reloads() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.session_mut().selection = Some(Selection::Tag {
            name: "gone".to_string(),
            commit_id: "abc1234".to_string(),
        });
        let _ = app.update(Message::RepositoryLoaded(app.active_path(), load_repository(
            &FakeGitRepository::ok(),
        )));
        assert!(app.session_mut().selection.is_none());
    }

    #[test]
    fn cherry_pick_waits_for_confirm_and_a_missing_commit_is_refused() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.session_mut().state.commits = vec![commit("abc1234")];
        app.session_mut().selection = Some(Selection::Commit {
            id: "abc1234".to_string(),
        });
        let _ = app.update(Message::AskCherryPick);
        assert_eq!(
            app.history_prompt,
            Some(HistoryPrompt::CherryPick {
                id: "abc1234".to_string(),
            })
        );
        app.session_mut().state.commits.clear();
        let _ = app.update(Message::ConfirmCherryPick);
        assert!(!app.session_mut().state.loading);
        assert!(app.history_prompt.is_none());
        assert_eq!(
            app.error.as_deref(),
            Some("That commit is no longer there.")
        );
    }

    #[test]
    fn reset_hard_starts_after_confirm() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.session_mut().state.commits = vec![commit("abc1234")];
        app.session_mut().selection = Some(Selection::Commit {
            id: "abc1234".to_string(),
        });
        let _ = app.update(Message::AskReset);
        assert!(matches!(
            app.history_prompt,
            Some(HistoryPrompt::Reset { .. })
        ));
        let _ = app.update(Message::ConfirmReset(ResetMode::Hard));
        assert!(app.session_mut().state.loading);
        assert!(app.history_prompt.is_none());
    }

    #[test]
    fn merge_and_rebase_wait_on_another_branch_and_skip_the_current_one() {
        let mut current = on_branch("main", true);
        let _ = current.update(Message::AskMerge);
        let _ = current.update(Message::AskRebase);
        assert!(current.history_prompt.is_none());

        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.session_mut().state.branches = vec![branch("main", true), branch("feature", false)];
        app.session_mut().selection = Some(Selection::Branch {
            name: "feature".to_string(),
            current: false,
        });
        let _ = app.update(Message::AskMerge);
        assert_eq!(
            app.history_prompt,
            Some(HistoryPrompt::Merge {
                name: "feature".to_string(),
            })
        );
        let _ = app.update(Message::ConfirmMerge);
        assert!(app.session_mut().state.loading);

        let mut rebase = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        rebase.session_mut().state.branches = vec![branch("main", true), branch("feature", false)];
        rebase.session_mut().selection = Some(Selection::Branch {
            name: "feature".to_string(),
            current: false,
        });
        let _ = rebase.update(Message::AskRebase);
        let _ = rebase.update(Message::ConfirmRebase);
        assert!(rebase.session_mut().state.loading);
        assert!(rebase.history_prompt.is_none());
    }

    #[test]
    fn a_failed_history_command_keeps_git_message_and_reloads() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.session_mut().state.loading = true;
        let _ = app.update(Message::HistoryFinished(app.active_path(), Err(DomainError::Unknown(
            "conflict in a.txt".to_string(),
        ))));
        assert_eq!(app.error.as_deref(), Some("conflict in a.txt"));
        assert!(app.session_mut().state.loading);
    }

    #[test]
    fn a_finish_for_an_unknown_path_is_dropped() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.session_mut().state.loading = true;

        // A finish routed to a closed or foreign tab must not touch the active session.
        let _ = app.update(Message::CheckoutFinished(r"D:\other".to_string(), Ok(())));

        assert!(app.session().state.loading);
        assert_eq!(app.sessions.len(), 1);
    }

    const FIRST: &str = r"D:\first";
    const SECOND: &str = r"D:\second";

    /// Two sessions with the first active, so finishes for `SECOND` are background finishes.
    fn two_session_app() -> GitApp {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.sessions[0].path = FIRST.to_string();
        app.sessions.push(WorktreeSession::new(
            SECOND.to_string(),
            std::sync::Arc::new(populated_repo()),
            AppSettings::default(),
        ));
        assert_eq!(app.active, 0);
        app
    }

    #[test]
    fn a_background_checkout_finish_clears_only_its_own_session() {
        let mut app = two_session_app();
        app.sessions[0].state.loading = true;
        app.sessions[1].state.loading = true;

        let _ = app.update(Message::CheckoutFinished(SECOND.to_string(), Ok(())));

        assert!(app.sessions[0].state.loading);
        assert!(!app.sessions[1].state.loading);
        assert_eq!(app.error, None);
    }

    #[test]
    fn refresh_session_reloads_the_named_session_not_the_active_one() {
        let mut app = two_session_app();

        let _ = app.update(Message::RefreshSession(SECOND.to_string()));

        assert!(!app.sessions[0].state.loading);
        assert!(app.sessions[1].state.loading);
    }

    #[test]
    fn refresh_session_for_an_unknown_path_is_dropped() {
        let mut app = two_session_app();

        let _ = app.update(Message::RefreshSession(r"D:\gone".to_string()));

        assert!(app.sessions.iter().all(|session| !session.state.loading));
    }

    #[test]
    fn a_background_commit_clears_the_message_on_its_own_session_only() {
        let mut app = two_session_app();
        app.sessions[0].commit_message = "keep me".to_string();
        app.sessions[1].commit_message = "record the change".to_string();
        app.sessions[1].state.loading = true;

        let _ = app.update(Message::WorktreeEditFinished(
            SECOND.to_string(),
            WorktreeEdit::Commit("record the change".to_string()),
            Ok(()),
        ));

        assert_eq!(app.sessions[0].commit_message, "keep me");
        assert_eq!(app.sessions[1].commit_message, "");
        assert!(!app.sessions[1].state.loading);
    }

    #[test]
    fn repository_loaded_lands_on_the_finishing_session() {
        let mut app = two_session_app();

        let _ = app.update(Message::RepositoryLoaded(
            SECOND.to_string(),
            load_repository(&populated_repo()),
        ));

        assert!(app.sessions[0].state.commits.is_empty());
        assert_eq!(app.sessions[1].state.commits.len(), 2);
    }

    #[test]
    fn patch_loaded_lands_on_the_finishing_session() {
        let mut app = two_session_app();
        app.sessions[1].patch_pending = true;
        let patch = Patch {
            path: "a.txt".to_string(),
            hunks: vec![],
            binary: false,
        };

        let _ = app.update(Message::PatchLoaded {
            path: SECOND.to_string(),
            generation: app.sessions[1].patch_generation,
            result: Ok(vec![patch]),
        });

        assert!(app.sessions[0].patches.is_empty());
        assert_eq!(app.sessions[1].patches.len(), 1);
        assert!(!app.sessions[1].patch_pending);
    }

    #[test]
    fn a_background_branch_create_checks_out_in_its_own_session() {
        let mut app = two_session_app();
        app.sessions[1].pending_checkout = Some("topic".to_string());
        app.sessions[1].state.loading = true;

        let _ = app.update(Message::BranchFinished(SECOND.to_string(), Ok(())));

        assert!(app.sessions[0].selection.is_none());
        assert!(!app.sessions[0].state.loading);
        assert!(app.sessions[1].pending_checkout.is_none());
        assert!(app.sessions[1].state.loading);
        assert_eq!(
            app.sessions[1].selection,
            Some(Selection::Branch {
                name: "topic".to_string(),
                current: false,
            })
        );
    }

    #[test]
    fn a_pending_checkout_belongs_to_the_session_that_started_it() {
        let mut app = two_session_app();
        app.sessions[1].pending_checkout = Some("topic".to_string());
        app.sessions[1].state.loading = true;

        // A different session finishing a branch job must not consume session 1's pending.
        let _ = app.update(Message::BranchFinished(FIRST.to_string(), Ok(())));

        assert_eq!(app.sessions[1].pending_checkout.as_deref(), Some("topic"));
        assert!(app.sessions[1].selection.is_none());
    }

    #[test]
    fn an_unknown_path_branch_finish_does_nothing() {
        for result in [Ok(()), Err(DomainError::Unknown("boom".to_string()))] {
            let mut app = two_session_app();
            app.sessions[0].pending_checkout = Some("topic".to_string());
            app.sessions[0].state.loading = true;

            let _ = app.update(Message::BranchFinished(r"D:\gone".to_string(), result));

            assert_eq!(app.sessions[0].pending_checkout.as_deref(), Some("topic"));
            assert!(app.sessions[0].state.loading);
            assert!(app.sessions[0].selection.is_none());
            assert!(!app.sessions[1].state.loading);
            assert_eq!(app.error, None);
        }
    }

    #[test]
    fn a_background_history_finish_reloads_its_own_session() {
        let mut app = two_session_app();
        app.sessions[1].state.loading = true;

        let _ = app.update(Message::HistoryFinished(SECOND.to_string(), Ok(())));

        assert!(!app.sessions[0].state.loading);
        assert!(app.sessions[1].state.loading);
    }

    #[test]
    fn an_unknown_path_history_finish_does_nothing() {
        for result in [Ok(()), Err(DomainError::Unknown("boom".to_string()))] {
            let mut app = two_session_app();
            app.sessions[0].state.loading = true;

            let _ = app.update(Message::HistoryFinished(r"D:\gone".to_string(), result));

            assert!(app.sessions[0].state.loading);
            assert!(!app.sessions[1].state.loading);
            assert_eq!(app.error, None);
        }
    }

    #[test]
    fn a_suggested_subject_lands_on_the_session_that_asked() {
        let mut app = two_session_app();
        app.agent_busy = true;
        app.sessions[0].commit_message = "mine".to_string();

        let _ = app.update(Message::CommitSuggested(
            SECOND.to_string(),
            crate::CommitSuggestion {
                subject: Ok("add the thing".to_string()),
                exchange: crate::ProviderExchange::kilo("add the thing"),
            },
        ));

        assert!(!app.agent_busy);
        assert_eq!(app.sessions[0].commit_message, "mine");
        assert_eq!(app.sessions[1].commit_message, "add the thing");
    }

    #[test]
    fn a_suggested_subject_for_an_unknown_path_is_dropped() {
        let mut app = two_session_app();
        app.agent_busy = true;
        app.sessions[0].commit_message = "mine".to_string();

        let _ = app.update(Message::CommitSuggested(
            r"D:\gone".to_string(),
            crate::CommitSuggestion {
                subject: Ok("add the thing".to_string()),
                exchange: crate::ProviderExchange::kilo("add the thing"),
            },
        ));

        assert!(!app.agent_busy);
        assert_eq!(app.sessions[0].commit_message, "mine");
        assert_eq!(app.sessions[1].commit_message, "");
    }

    #[test]
    fn activating_another_tab_leaves_the_first_session_untouched() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.session_mut().state.commits = vec![commit("aaaaaaa1")];
        app.session_mut().commit_message = "draft".to_string();
        app.session_mut().selection = Some(Selection::Commit {
            id: "aaaaaaa1".to_string(),
        });
        app.sessions.push(WorktreeSession::new(
            r"D:\second".to_string(),
            std::sync::Arc::new(FakeGitRepository::ok()),
            AppSettings::default(),
        ));

        app.activate(1);

        assert_eq!(app.active, 1);
        // The first session keeps its history; activating never reloaded it.
        assert_eq!(app.sessions[0].state.commits.len(), 1);
        assert!(!app.sessions[0].state.loading);
        // Default Partial retention keeps message and marks, clears the selection.
        assert_eq!(app.sessions[0].commit_message, "draft");
        assert!(app.sessions[0].selection.is_none());
        // The newly active session was not sent a load job.
        assert!(!app.sessions[1].state.loading);
    }

    #[test]
    fn path_only_retention_clears_the_leaving_session_message() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.settings.worktree_tab_retention = WorktreeTabRetention::PathOnly;
        app.session_mut().commit_message = "draft".to_string();
        app.sessions.push(WorktreeSession::new(
            r"D:\second".to_string(),
            std::sync::Arc::new(FakeGitRepository::ok()),
            AppSettings::default(),
        ));

        app.activate(1);

        assert!(app.sessions[0].commit_message.is_empty());
    }

    const THIRD: &str = r"D:\third";

    fn linked(path: &str, branch: &str, is_main: bool) -> Worktree {
        Worktree {
            path: path.to_string(),
            branch: branch.to_string(),
            is_main,
            locked: false,
        }
    }

    /// `two_session_app` plus a catalogue where `FIRST` is main and `THIRD` has no tab.
    fn catalogued_app() -> GitApp {
        let mut app = two_session_app();
        app.worktree_catalogue = vec![
            linked(FIRST, "main", true),
            linked(SECOND, "feature", false),
            linked(THIRD, "other", false),
        ];
        app
    }

    fn push_session(app: &mut GitApp, path: &str) {
        app.sessions.push(WorktreeSession::new(
            path.to_string(),
            std::sync::Arc::new(FakeGitRepository::ok()),
            AppSettings::default(),
        ));
    }

    #[test]
    fn closing_the_last_tab_is_refused() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));

        let _ = app.update(Message::CloseWorktreeTab(0));

        assert_eq!(app.sessions.len(), 1);
    }

    #[test]
    fn closing_a_tab_with_a_job_running_is_refused() {
        let mut app = two_session_app();
        app.sessions[1].state.loading = true;

        let _ = app.update(Message::CloseWorktreeTab(1));

        assert_eq!(app.sessions.len(), 2);
    }

    #[test]
    fn closing_a_background_tab_keeps_the_active_one_focused() {
        let mut app = two_session_app();
        push_session(&mut app, THIRD);
        app.activate(2);

        let _ = app.update(Message::CloseWorktreeTab(0));

        assert_eq!(app.sessions.len(), 2);
        assert_eq!(app.active, 1);
        assert_eq!(app.active_path(), THIRD);
    }

    #[test]
    fn closing_the_active_tab_prefers_the_main_worktrees_tab() {
        let mut app = catalogued_app();
        push_session(&mut app, THIRD);
        app.activate(2);

        let _ = app.update(Message::CloseWorktreeTab(2));

        assert_eq!(app.active_path(), FIRST);
    }

    #[test]
    fn closing_the_active_tab_without_main_open_takes_its_neighbour() {
        let mut app = two_session_app();
        push_session(&mut app, THIRD);
        app.worktree_catalogue = vec![linked(r"D:\elsewhere", "main", true)];
        app.activate(1);

        let _ = app.update(Message::CloseWorktreeTab(1));

        assert_eq!(app.active_path(), THIRD);
    }

    #[test]
    fn closing_a_tab_leaves_the_worktree_in_the_catalogue() {
        let mut app = catalogued_app();

        let _ = app.update(Message::CloseWorktreeTab(1));

        assert_eq!(app.sessions.len(), 1);
        assert_eq!(app.worktree_catalogue.len(), 3);
    }

    #[test]
    fn opening_an_open_worktree_focuses_its_tab_without_a_new_session() {
        let mut app = catalogued_app();

        // Git prints forward slashes; the tab still matches.
        let _ = app.update(Message::OpenWorktree("D:/second/".to_string()));

        assert_eq!(app.sessions.len(), 2);
        assert_eq!(app.active, 1);
        assert!(app.worktree_opening.is_none());
    }

    #[test]
    fn opening_a_second_worktree_adds_a_session_and_leaves_the_first_alone() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.sessions[0].path = FIRST.to_string();
        app.session_mut().state.commits = vec![commit("aaaaaaa1")];

        let _ = app.update(Message::OpenWorktree(SECOND.to_string()));
        assert_eq!(app.worktree_opening.as_deref(), Some(SECOND));
        let _ = app.update(Message::WorktreeOpened(
            SECOND.to_string(),
            Ok(OpenedRepository(std::sync::Arc::new(populated_repo()))),
        ));

        assert_eq!(app.sessions.len(), 2);
        assert_eq!(app.active, 1);
        assert!(app.worktree_opening.is_none());
        // The first tab keeps its history and was sent no load.
        assert_eq!(app.sessions[0].state.commits.len(), 1);
        assert!(!app.sessions[0].state.loading);
    }

    #[test]
    fn a_failed_worktree_open_shows_why_and_adds_no_session() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        let _ = app.update(Message::OpenWorktree(SECOND.to_string()));

        let _ = app.update(Message::WorktreeOpened(
            SECOND.to_string(),
            Err(DomainError::Unknown("no such folder".to_string())),
        ));

        assert_eq!(app.sessions.len(), 1);
        assert!(app.error.is_some());
        assert!(app.worktree_opening.is_none());
    }

    #[test]
    fn an_open_that_was_never_asked_for_is_dropped() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));

        let _ = app.update(Message::WorktreeOpened(
            SECOND.to_string(),
            Ok(OpenedRepository(std::sync::Arc::new(populated_repo()))),
        ));

        assert_eq!(app.sessions.len(), 1);
    }

    #[test]
    fn an_empty_path_open_failure_releases_the_open_lock() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        let _ = app.update(Message::OpenWorktree(SECOND.to_string()));
        assert_eq!(app.worktree_opening.as_deref(), Some(SECOND));

        // The interrupted-thread shape: no path echoed back.
        let _ = app.update(Message::WorktreeOpened(
            String::new(),
            Err(DomainError::Unknown(
                "repository open was interrupted".to_string(),
            )),
        ));

        assert!(app.worktree_opening.is_none());
        assert_eq!(app.sessions.len(), 1);
        assert!(app.error.is_some());

        // A later open is accepted rather than silently dropped.
        let _ = app.update(Message::OpenWorktree(THIRD.to_string()));
        assert_eq!(app.worktree_opening.as_deref(), Some(THIRD));
    }

    #[test]
    fn asking_to_create_or_remove_clears_a_stale_error() {
        let mut app = catalogued_app();
        app.error = Some("stale".to_string());
        let _ = app.update(Message::AskCreateWorktree);
        assert!(app.worktree_prompt.is_some());
        assert_eq!(app.error, None);

        app.error = Some("stale".to_string());
        let _ = app.update(Message::AskRemoveWorktree {
            path: THIRD.to_string(),
        });
        assert!(matches!(
            app.worktree_prompt,
            Some(WorktreePrompt::Remove { .. })
        ));
        assert_eq!(app.error, None);
    }

    #[test]
    fn a_worktree_opens_through_the_worktree_opener_and_never_remembers() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        fn counting(calls: &Arc<AtomicUsize>) -> RepositoryOpener {
            let calls = Arc::clone(calls);
            Arc::new(move |_| {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(Arc::new(FakeGitRepository::ok()) as SharedRepository)
            })
        }

        // The repository opener is the one that remembers `last_repo_path`.
        let remembered = Arc::new(AtomicUsize::new(0));
        let worktree_opened = Arc::new(AtomicUsize::new(0));
        let mut app = GitApp::with_repo_opener(
            Arc::new(FakeGitRepository::ok()),
            Some(FIRST.to_string()),
            counting(&remembered),
            counting(&worktree_opened),
            unavailable_initialiser(),
            unavailable_cloner(),
            AppSettings::default(),
            ignore_settings(),
        );

        // The job starts its thread as soon as the handler builds it, so the opener runs
        // even though the test never polls the returned task.
        let _ = app.update(Message::OpenWorktree(SECOND.to_string()));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while worktree_opened.load(Ordering::SeqCst) == 0 && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let _ = app.update(Message::WorktreeOpened(
            SECOND.to_string(),
            Ok(OpenedRepository(Arc::new(FakeGitRepository::ok()))),
        ));

        assert_eq!(app.sessions.len(), 2);
        assert_eq!(worktree_opened.load(Ordering::SeqCst), 1);
        assert_eq!(remembered.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn opening_another_repository_keeps_the_worktree_opener() {
        let mut app = GitApp::with_repo(Arc::new(FakeGitRepository::ok()));
        let before = Arc::clone(&app.open_worktree);

        let _ = app.update(Message::RepositoryOpened(
            SECOND.to_string(),
            Ok(OpenedRepository(Arc::new(FakeGitRepository::ok()))),
        ));

        assert!(Arc::ptr_eq(&before, &app.open_worktree));
    }

    #[test]
    fn a_loaded_catalogue_lands_for_a_known_session_only() {
        let mut app = two_session_app();
        let list = vec![linked(FIRST, "main", true)];

        let _ = app.update(Message::WorktreesLoaded(
            r"D:\gone".to_string(),
            Ok(list.clone()),
        ));
        assert!(app.worktree_catalogue.is_empty());

        let _ = app.update(Message::WorktreesLoaded(
            SECOND.to_string(),
            Ok(list.clone()),
        ));
        assert_eq!(app.worktree_catalogue, list);
        assert_eq!(app.catalogue_error, None);
    }

    #[test]
    fn a_failed_catalogue_load_keeps_the_list_and_every_session() {
        let mut app = catalogued_app();

        let _ = app.update(Message::WorktreesLoaded(
            FIRST.to_string(),
            Err(DomainError::Unknown("git is missing".to_string())),
        ));

        assert_eq!(app.worktree_catalogue.len(), 3);
        assert_eq!(app.sessions.len(), 2);
        assert!(app.catalogue_error.is_some());
        assert_eq!(app.error, None);
    }

    #[test]
    fn the_main_worktree_cannot_be_removed() {
        let mut app = catalogued_app();

        let _ = app.update(Message::AskRemoveWorktree {
            path: FIRST.to_string(),
        });

        assert!(app.worktree_prompt.is_none());
        assert!(app.error.is_some());
    }

    #[test]
    fn removing_asks_first_with_force_off() {
        let mut app = catalogued_app();

        let _ = app.update(Message::AskRemoveWorktree {
            path: THIRD.to_string(),
        });

        assert_eq!(
            app.worktree_prompt,
            Some(WorktreePrompt::Remove {
                path: THIRD.to_string(),
                branch: "other".to_string(),
                force: false,
            })
        );
        let _ = app.update(Message::WorktreeForceToggled(true));
        assert!(matches!(
            app.worktree_prompt,
            Some(WorktreePrompt::Remove { force: true, .. })
        ));
    }

    #[test]
    fn the_only_open_tab_cannot_be_removed() {
        let mut app = GitApp::with_repo(Arc::new(FakeGitRepository::ok()));
        app.sessions[0].path = SECOND.to_string();
        app.worktree_catalogue = vec![
            linked(FIRST, "main", true),
            linked(SECOND, "feature", false),
        ];

        let _ = app.update(Message::AskRemoveWorktree {
            path: SECOND.to_string(),
        });

        assert!(app.worktree_prompt.is_none());
        assert!(app.error.is_some());
    }

    #[test]
    fn confirming_a_remove_starts_a_job_once() {
        let mut app = catalogued_app();
        let _ = app.update(Message::AskRemoveWorktree {
            path: SECOND.to_string(),
        });

        let _ = app.update(Message::ConfirmRemoveWorktree { force: true });
        assert!(app.worktree_busy);

        // A second confirm while the job runs is ignored; the prompt is still the same one.
        let _ = app.update(Message::ConfirmRemoveWorktree { force: true });
        assert!(app.worktree_busy);
        assert_eq!(app.sessions.len(), 2);
    }

    #[test]
    fn a_removed_worktree_drops_its_session_and_reselects() {
        let mut app = catalogued_app();
        app.activate(1);
        app.worktree_prompt = Some(WorktreePrompt::Remove {
            path: SECOND.to_string(),
            branch: "feature".to_string(),
            force: false,
        });
        app.worktree_busy = true;

        let _ = app.update(Message::WorktreeRemoved {
            session: FIRST.to_string(),
            path: "D:/second".to_string(),
            result: Ok(()),
        });

        assert_eq!(app.sessions.len(), 1);
        assert_eq!(app.active_path(), FIRST);
        assert!(app.worktree_prompt.is_none());
        assert!(!app.worktree_busy);
    }

    #[test]
    fn a_failed_remove_keeps_the_prompt_open_with_git_s_message() {
        let mut app = catalogued_app();
        app.worktree_prompt = Some(WorktreePrompt::Remove {
            path: SECOND.to_string(),
            branch: "feature".to_string(),
            force: false,
        });
        app.worktree_busy = true;

        let _ = app.update(Message::WorktreeRemoved {
            session: FIRST.to_string(),
            path: SECOND.to_string(),
            result: Err(DomainError::Unknown("contains modified files".to_string())),
        });

        assert_eq!(app.sessions.len(), 2);
        assert!(app.worktree_prompt.is_some());
        assert!(app.error.is_some());
        assert!(!app.worktree_busy);
    }

    fn absolute_temp(name: &str) -> String {
        std::env::temp_dir()
            .join(name)
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn a_create_needs_an_absolute_path_and_a_branch() {
        let mut app = catalogued_app();
        let _ = app.update(Message::AskCreateWorktree);

        let _ = app.update(Message::WorktreePathChanged("..\\relative".to_string()));
        let _ = app.update(Message::WorktreeBranchChanged("topic".to_string()));
        let _ = app.update(Message::ConfirmCreateWorktree);
        assert!(!app.worktree_busy);

        let _ = app.update(Message::WorktreePathChanged(absolute_temp("wt-topic")));
        let _ = app.update(Message::WorktreeBranchChanged("  ".to_string()));
        let _ = app.update(Message::ConfirmCreateWorktree);
        assert!(!app.worktree_busy);

        let _ = app.update(Message::WorktreeBranchChanged("topic".to_string()));
        let _ = app.update(Message::ConfirmCreateWorktree);
        assert!(app.worktree_busy);
    }

    #[test]
    fn a_created_worktree_closes_the_prompt_and_is_not_opened_for_a_foreign_session() {
        let mut app = catalogued_app();
        let _ = app.update(Message::AskCreateWorktree);
        app.worktree_busy = true;

        let _ = app.update(Message::WorktreeCreated {
            session: r"D:\gone".to_string(),
            path: THIRD.to_string(),
            result: Ok(()),
        });

        assert!(app.worktree_prompt.is_none());
        assert!(!app.worktree_busy);
        assert_eq!(app.sessions.len(), 2);
    }

    #[test]
    fn a_failed_create_keeps_the_prompt_open_with_git_s_message() {
        let mut app = catalogued_app();
        let _ = app.update(Message::AskCreateWorktree);
        app.worktree_busy = true;

        let _ = app.update(Message::WorktreeCreated {
            session: FIRST.to_string(),
            path: THIRD.to_string(),
            result: Err(DomainError::Unknown("already exists".to_string())),
        });

        assert!(matches!(
            app.worktree_prompt,
            Some(WorktreePrompt::Create { .. })
        ));
        assert!(app.error.is_some());
        assert!(!app.worktree_busy);
        assert_eq!(app.worktree_catalogue.len(), 3);
    }

    #[test]
    fn switching_tabs_clears_a_worktree_prompt_and_the_picker() {
        let mut app = catalogued_app();
        let _ = app.update(Message::AskCreateWorktree);
        app.tab_picker_open = true;

        let _ = app.update(Message::SelectWorktreeTab(1));

        assert_eq!(app.active, 1);
        assert!(app.worktree_prompt.is_none());
        assert!(!app.tab_picker_open);
    }

    #[test]
    fn a_wip_row_activates_its_session_and_shows_its_working_tree() {
        let mut app = two_session_app();

        let _ = app.update(Message::ShowSessionWorkingTree(1));

        assert_eq!(app.active, 1);
        assert!(matches!(
            app.session().selection,
            Some(Selection::Branch { current: true, .. })
        ));
    }

    #[test]
    fn a_wip_row_for_a_missing_session_changes_nothing() {
        let mut app = two_session_app();

        let _ = app.update(Message::ShowSessionWorkingTree(7));

        assert_eq!(app.active, 0);
        assert!(app.session().selection.is_none());
    }

    #[test]
    fn escape_dismisses_the_worktree_prompt_then_the_picker() {
        let mut app = catalogued_app();
        let _ = app.update(Message::AskCreateWorktree);
        app.tab_picker_open = true;

        let _ = app.update(Message::Escape);
        assert!(app.worktree_prompt.is_none());
        assert!(app.tab_picker_open);

        let _ = app.update(Message::Escape);
        assert!(!app.tab_picker_open);
    }
}
