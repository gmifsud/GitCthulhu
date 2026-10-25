use std::sync::Arc;

use core_domain::{
    AppSettings, AppState, Branch, DomainError, FileDisposition, Hunk, Patch, ResetMode, Stash,
    ViewEvent,
};
use iced::widget::pane_grid;
use iced::Task;

use crate::appearance::{font_family, font_size};
use crate::changes::{marked_paths, MarkedFile};
use crate::jobs::{
    branch_in_background, checkout_in_background, choose_repository_folder, clone_in_background,
    discard_hunk_in_background, discard_in_background, edit_in_background, error_message,
    fetch_in_background, history_in_background, init_in_background, list_models_in_background,
    load_patch_in_background, load_repository_in_background, pull_in_background,
    push_in_background, stage_hunk_in_background, stash_in_background,
    suggest_commit_in_background, BranchAction, HistoryAction, PatchQuery, SharedRepository,
    StashAction, WorktreeEdit,
};
use crate::layout::{self, LayoutPreset, RatioSlot, WorkspacePane};
use crate::selection::{visible_selection, Selection};
use crate::theme::DesignTokens;
use crate::{RepositoryCloner, RepositoryInitialiser, RepositoryOpener, SettingsSaver};

pub(crate) struct GitApp {
    pub(crate) state: AppState,
    pub(crate) adapter: Option<SharedRepository>,
    /// Folder opened in this window. `None` on the welcome screen.
    pub(crate) repository_path: Option<String>,
    pub(crate) panes: pane_grid::State<WorkspacePane>,
    pub(crate) layout: LayoutPreset,
    ratios: [layout::LayoutRatios; 3],
    /// Rebuilt whenever [`Self::install_layout`] constructs a new pane grid.
    split_slots: Vec<(pane_grid::Split, RatioSlot)>,
    pub(crate) files_rail_open: bool,
    pub(crate) branch_list_open: bool,
    pub(crate) branch_query: String,
    pub(crate) marked_files: Vec<crate::changes::MarkedFile>,
    pub(crate) files_as_tree: bool,
    pub(crate) agent_models: Vec<String>,
    pub(crate) agent_models_error: Option<String>,
    pub(crate) agent_busy: bool,
    pub(crate) terminal: crate::terminal::Transcript,
    pub(crate) terminal_open: bool,
    pub(crate) list_models: crate::ModelLister,
    pub(crate) suggest_commit: crate::CommitSuggester,
    pub(crate) error: Option<String>,
    pub(crate) commit_message: String,
    pub(crate) tokens: DesignTokens,
    pub(crate) selection: Option<Selection>,
    pub(crate) settings_open: bool,
    pub(crate) patches: Vec<Patch>,
    pub(crate) patch_error: Option<String>,
    pub(crate) patch_pending: bool,
    pub(crate) confirm_discard: Option<(String, FileDisposition)>,
    pub(crate) confirm_drop: Option<Stash>,
    pub(crate) branch_prompt: Option<BranchPrompt>,
    pub(crate) history_prompt: Option<HistoryPrompt>,
    pub(crate) clone_url: String,
    patch_generation: u64,
    open_repository: RepositoryOpener,
    initialise: RepositoryInitialiser,
    clone_repository: RepositoryCloner,
    save_settings: SettingsSaver,
}

#[derive(Debug, Clone)]
pub(crate) enum Message {
    RefreshRepository,
    CheckoutBranch(String),
    CommitMessageChanged(String),
    EditWorktree(WorktreeEdit),
    RepositoryLoaded(Vec<ViewEvent>),
    CheckoutFinished(Result<(), DomainError>),
    Fetch,
    FetchFinished(Result<(), DomainError>),
    Push,
    PushFinished(Result<(), DomainError>),
    Pull,
    PullFinished(Result<(), DomainError>),
    WorktreeEditFinished(WorktreeEdit, Result<(), DomainError>),
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
    StageMarked,
    UnstageMarked,
    /// `None` means the dialog was cancelled.
    RepositoryFolderChosen(Option<String>),
    /// Replace the current adapter with the repository at this path.
    OpenRepository(String),
    /// Select the row the ribbon describes.
    Select(Selection),
    OpenSettings,
    RefreshAgentModels,
    AgentTokenChanged(String),
    AgentModelChosen(String),
    AgentModelsLoaded(crate::ModelList),
    SuggestCommit,
    CommitSuggested(crate::CommitSuggestion),
    ToggleTerminal,
    ClearTerminal,
    CloseSettings,
    UiFontChosen(String),
    UiSizeChosen(u8),
    PatchLoaded(u64, Result<Vec<Patch>, DomainError>),
    AskDiscard,
    CancelDiscard,
    ConfirmDiscard,
    DiscardFinished(Result<(), DomainError>),
    StageHunk {
        path: String,
        index: usize,
    },
    DiscardHunk {
        path: String,
        index: usize,
    },
    HunkFinished(Result<(), DomainError>),
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
    StashFinished(Result<(), DomainError>),
    AskCreateBranch,
    AskRenameBranch,
    AskDeleteBranch,
    BranchNameChanged(String),
    ConfirmBranch,
    CancelBranch,
    BranchFinished(Result<(), DomainError>),
    AskCherryPick,
    AskReset,
    AskMerge,
    AskRebase,
    CancelHistory,
    ConfirmCherryPick,
    ConfirmReset(ResetMode),
    ConfirmMerge,
    ConfirmRebase,
    HistoryFinished(Result<(), DomainError>),
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
    fn new(
        adapter: Option<SharedRepository>,
        repository_path: Option<String>,
        open_repository: RepositoryOpener,
        initialise: RepositoryInitialiser,
        clone_repository: RepositoryCloner,
        settings: AppSettings,
        save_settings: SettingsSaver,
    ) -> Self {
        let mut state = AppState::new();
        state.apply_event(ViewEvent::SettingsUpdated(settings));
        let layout = LayoutPreset::DEFAULT;
        let ratios = layout::default_ratios();
        let panes = layout::new_panes(layout, &ratios[layout.index()]);
        let split_slots = layout::split_slots(&panes);
        Self {
            state,
            adapter,
            repository_path,
            panes,
            layout,
            ratios,
            split_slots,
            files_rail_open: true,
            branch_list_open: false,
            branch_query: String::new(),
            marked_files: Vec::new(),
            files_as_tree: false,
            agent_models: Vec::new(),
            agent_models_error: None,
            agent_busy: false,
            terminal: crate::terminal::Transcript::new(),
            terminal_open: true,
            list_models: unavailable_model_lister(),
            suggest_commit: unavailable_suggester(),
            error: None,
            commit_message: String::new(),
            tokens: DesignTokens::deep_focus(),
            selection: None,
            settings_open: false,
            patches: Vec::new(),
            patch_error: None,
            patch_pending: false,
            confirm_discard: None,
            confirm_drop: None,
            branch_prompt: None,
            history_prompt: None,
            clone_url: String::new(),
            patch_generation: 0,
            open_repository,
            initialise,
            clone_repository,
            save_settings,
        }
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
        let query = match self.selection.clone() {
            Some(Selection::Commit { id } | Selection::Tag { commit_id: id, .. }) => {
                PatchQuery::Commit(id)
            }
            Some(Selection::File { path, place }) => PatchQuery::File { path, place },
            _ => {
                self.patches.clear();
                self.patch_error = None;
                self.patch_pending = false;
                return Task::none();
            }
        };
        let Some(repo) = self.adapter.clone() else {
            return Task::none();
        };
        self.patch_generation = self.patch_generation.wrapping_add(1);
        let generation = self.patch_generation;
        self.patches.clear();
        self.patch_error = None;
        self.patch_pending = true;
        Task::perform(load_patch_in_background(repo, query), move |result| {
            Message::PatchLoaded(generation, result)
        })
    }

    fn reconcile_selection(&mut self) {
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

    fn hunk_at(&self, path: &str, index: usize) -> Option<Hunk> {
        self.patches
            .iter()
            .find(|patch| patch.path == path)
            .and_then(|patch| patch.hunks.get(index))
            .cloned()
    }

    fn store_agent(&mut self, token: String, model: String) {
        let mut settings = self.state.settings.clone();
        settings.agent_token = token;
        settings.agent_model = model;
        self.state
            .apply_event(ViewEvent::SettingsUpdated(settings.clone()));
        (self.save_settings)(settings);
    }

    fn refresh_models(&mut self) -> Task<Message> {
        if self.agent_busy {
            return Task::none();
        }
        self.agent_busy = true;
        self.agent_models_error = None;
        let token = self.state.settings.agent_token.clone();
        let list = std::sync::Arc::clone(&self.list_models);
        Task::perform(
            list_models_in_background(list, token),
            Message::AgentModelsLoaded,
        )
    }

    fn suggest_commit_message(&mut self) -> Task<Message> {
        if self.agent_busy || self.state.loading {
            return Task::none();
        }
        let files = self.files_for_suggestion();
        if files.is_empty() {
            self.error = Some("Select a file first.".to_string());
            return Task::none();
        }
        let model = self.state.settings.agent_model.trim().to_string();
        if model.is_empty() {
            self.error = Some("Choose a model under Settings.".to_string());
            return Task::none();
        }
        let Some(repo) = self.adapter.clone() else {
            return Task::none();
        };
        self.agent_busy = true;
        self.error = None;
        let token = self.state.settings.agent_token.clone();
        let suggest = std::sync::Arc::clone(&self.suggest_commit);
        Task::perform(
            suggest_commit_in_background(repo, files, token, model, suggest),
            Message::CommitSuggested,
        )
    }

    fn files_for_suggestion(&self) -> Vec<(String, FileDisposition)> {
        if !self.marked_files.is_empty() {
            return self
                .marked_files
                .iter()
                .map(|mark| (mark.path.clone(), mark.place))
                .collect();
        }
        match &self.selection {
            Some(Selection::File { path, place }) => vec![(path.clone(), *place)],
            _ => Vec::new(),
        }
    }

    fn store_appearance(&mut self, font: &str, size: u8) {
        let mut settings = self.state.settings.clone();
        settings.ui_font = font_family(font).to_string();
        settings.ui_size = font_size(size);
        self.state
            .apply_event(ViewEvent::SettingsUpdated(settings.clone()));
        (self.save_settings)(settings);
    }

    #[cfg(test)]
    pub(crate) fn with_repo(repo: SharedRepository) -> Self {
        Self::with_repo_opener(
            repo,
            None,
            unavailable_opener(),
            unavailable_initialiser(),
            unavailable_cloner(),
            AppSettings::default(),
            ignore_settings(),
        )
    }

    pub(crate) fn with_repo_opener(
        repo: SharedRepository,
        repository_path: Option<String>,
        open_repository: RepositoryOpener,
        initialise: RepositoryInitialiser,
        clone_repository: RepositoryCloner,
        settings: AppSettings,
        save_settings: SettingsSaver,
    ) -> Self {
        Self::new(
            Some(repo),
            repository_path,
            open_repository,
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
            unavailable_initialiser(),
            unavailable_cloner(),
            AppSettings::default(),
            ignore_settings(),
        )
    }

    pub(crate) fn welcome_with(
        open_repository: RepositoryOpener,
        initialise: RepositoryInitialiser,
        clone_repository: RepositoryCloner,
        settings: AppSettings,
        save_settings: SettingsSaver,
    ) -> Self {
        Self::new(
            None,
            None,
            open_repository,
            initialise,
            clone_repository,
            settings,
            save_settings,
        )
    }

    pub(crate) fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::RefreshRepository => self.begin_refresh(true),
            Message::RepositoryLoaded(events) => {
                for event in events {
                    if let ViewEvent::ErrorOccurred(error) = &event {
                        self.error.get_or_insert_with(|| error_message(error));
                    }
                    self.state.apply_event(event);
                }
                self.reconcile_selection();
                self.retain_marks();
                self.start_patch()
            }
            Message::FocusFile { path, place } => {
                self.marked_files = vec![MarkedFile {
                    path: path.clone(),
                    place,
                }];
                self.update(Message::Select(Selection::File { path, place }))
            }
            Message::ToggleFileMark { path, place } => {
                if let Some(index) = self
                    .marked_files
                    .iter()
                    .position(|mark| mark.path == path && mark.place == place)
                {
                    self.marked_files.remove(index);
                } else {
                    self.marked_files.push(MarkedFile {
                        path: path.clone(),
                        place,
                    });
                }
                self.update(Message::Select(Selection::File { path, place }))
            }
            Message::SelectAllFiles => {
                self.marked_files = self.all_changed_files();
                if self.marked_files.is_empty() {
                    return Task::none();
                }
                if !matches!(self.selection, Some(Selection::File { .. })) {
                    let first = self.marked_files[0].clone();
                    return self.update(Message::Select(Selection::File {
                        path: first.path,
                        place: first.place,
                    }));
                }
                Task::none()
            }
            Message::ClearFileSelection => {
                self.marked_files.clear();
                self.update(Message::ShowWorkingTree)
            }
            Message::SetFileTree(as_tree) => {
                self.files_as_tree = as_tree;
                Task::none()
            }
            Message::StageMarked => self.edit_marked(true),
            Message::UnstageMarked => self.edit_marked(false),
            Message::CheckoutBranch(branch) => {
                let Some(repo) = self.adapter.clone() else {
                    return Task::none();
                };
                if self.state.loading {
                    return Task::none();
                }
                self.state.loading = true;
                self.error = None;
                Task::perform(
                    checkout_in_background(repo, branch),
                    Message::CheckoutFinished,
                )
            }
            Message::CheckoutFinished(result) => {
                self.state.loading = false;
                match result {
                    Ok(()) => Task::done(Message::RefreshRepository),
                    Err(error) => {
                        self.error = Some(error_message(&error));
                        Task::none()
                    }
                }
            }
            Message::Fetch => {
                let Some(repo) = self.adapter.clone() else {
                    return Task::none();
                };
                if self.state.loading {
                    return Task::none();
                }
                self.state.loading = true;
                self.error = None;
                Task::perform(fetch_in_background(repo), Message::FetchFinished)
            }
            Message::FetchFinished(result) => {
                self.state.loading = false;
                match result {
                    Ok(()) => Task::done(Message::RefreshRepository),
                    Err(error) => {
                        self.error = Some(error_message(&error));
                        Task::none()
                    }
                }
            }
            Message::Push => {
                let Some(repo) = self.adapter.clone() else {
                    return Task::none();
                };
                if self.state.loading {
                    return Task::none();
                }
                self.state.loading = true;
                self.error = None;
                Task::perform(push_in_background(repo), Message::PushFinished)
            }
            Message::PushFinished(result) => {
                self.state.loading = false;
                match result {
                    Ok(()) => Task::done(Message::RefreshRepository),
                    Err(error) => {
                        self.error = Some(error_message(&error));
                        Task::none()
                    }
                }
            }
            Message::Pull => {
                let Some(repo) = self.adapter.clone() else {
                    return Task::none();
                };
                if self.state.loading {
                    return Task::none();
                }
                self.state.loading = true;
                self.error = None;
                Task::perform(pull_in_background(repo), Message::PullFinished)
            }
            Message::PullFinished(result) => {
                self.state.loading = false;
                match result {
                    Ok(()) => Task::done(Message::RefreshRepository),
                    Err(error) => {
                        self.error = Some(error_message(&error));
                        Task::none()
                    }
                }
            }
            Message::CommitMessageChanged(message) => {
                self.commit_message = message;
                Task::none()
            }
            Message::EditWorktree(edit) => {
                let Some(repo) = self.adapter.clone() else {
                    return Task::none();
                };
                if self.state.loading {
                    return Task::none();
                }
                if let WorktreeEdit::Commit(message) = &edit {
                    if message.trim().is_empty() {
                        return Task::none();
                    }
                }
                self.state.loading = true;
                self.error = None;
                let report = edit.clone();
                Task::perform(edit_in_background(repo, edit), move |result| {
                    Message::WorktreeEditFinished(report.clone(), result)
                })
            }
            Message::WorktreeEditFinished(edit, result) => {
                self.state.loading = false;
                match result {
                    Ok(()) => {
                        if matches!(edit, WorktreeEdit::Commit(_)) {
                            self.commit_message.clear();
                        }
                        Task::done(Message::RefreshRepository)
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
                self.panes.resize(event.split, event.ratio);
                if let Some(slot) = slot {
                    let ratios = &mut self.ratios[self.layout.index()];
                    match slot {
                        RatioSlot::Graph => ratios.graph = event.ratio,
                        RatioSlot::Middle => ratios.middle = event.ratio,
                        RatioSlot::Files => ratios.files = event.ratio,
                    }
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
                let current = self
                    .state
                    .branches
                    .iter()
                    .any(|branch| !branch.is_remote && branch.is_head && branch.name == name);
                self.selection = Some(Selection::Branch {
                    name: name.clone(),
                    current,
                });
                if current {
                    Task::none()
                } else {
                    self.update(Message::CheckoutBranch(name))
                }
            }
            Message::ShowWorkingTree => {
                let name = self
                    .state
                    .branches
                    .iter()
                    .find(|branch| branch.is_head && !branch.is_remote)
                    .map(|branch| branch.name.clone())
                    .unwrap_or_else(|| "Detached HEAD".to_string());
                self.selection = Some(Selection::Branch {
                    name,
                    current: true,
                });
                self.start_patch()
            }
            Message::ChooseRepositoryFolder => {
                if self.state.loading {
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
                if self.state.loading {
                    return Task::none();
                }
                if path.trim().is_empty() {
                    self.error = Some("Choose a repository folder.".to_string());
                    return Task::none();
                }
                match (self.open_repository)(&path) {
                    Ok(repo) => {
                        let open_repository = Arc::clone(&self.open_repository);
                        let initialise = Arc::clone(&self.initialise);
                        let clone_repository = Arc::clone(&self.clone_repository);
                        let save_settings = Arc::clone(&self.save_settings);
                        let settings = self.state.settings.clone();
                        *self = Self::new(
                            Some(repo),
                            Some(path),
                            open_repository,
                            initialise,
                            clone_repository,
                            settings,
                            save_settings,
                        );
                        Task::done(Message::RefreshRepository)
                    }
                    Err(error) => {
                        self.error = Some(error_message(&error));
                        Task::none()
                    }
                }
            }
            Message::ChooseInitFolder => {
                if self.state.loading {
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
                if self.state.loading {
                    return Task::none();
                }
                if path.trim().is_empty() {
                    self.error = Some("Choose a folder to init.".to_string());
                    return Task::none();
                }
                self.state.loading = true;
                self.error = None;
                let initialise = Arc::clone(&self.initialise);
                let finished = path.clone();
                Task::perform(init_in_background(initialise, path), move |result| {
                    Message::InitFinished(finished.clone(), result)
                })
            }
            Message::InitFinished(path, Ok(())) => {
                self.state.loading = false;
                self.update(Message::OpenRepository(path))
            }
            Message::InitFinished(_, Err(error)) => {
                self.state.loading = false;
                self.error = Some(error_message(&error));
                Task::none()
            }
            Message::CloneUrlChanged(url) => {
                self.clone_url = url;
                Task::none()
            }
            Message::ChooseCloneFolder => {
                if self.state.loading || self.clone_url.trim().is_empty() {
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
                if self.state.loading {
                    return Task::none();
                }
                self.state.loading = true;
                self.error = None;
                let clone_repository = Arc::clone(&self.clone_repository);
                Task::perform(
                    clone_in_background(clone_repository, url, parent),
                    Message::CloneFinished,
                )
            }
            Message::CloneFinished(Ok(path)) => {
                self.state.loading = false;
                self.update(Message::OpenRepository(path))
            }
            Message::CloneFinished(Err(error)) => {
                self.state.loading = false;
                self.error = Some(error_message(&error));
                Task::none()
            }
            Message::Select(selection) => {
                self.branch_list_open = false;
                self.branch_query.clear();
                self.selection = Some(selection);
                self.confirm_discard = None;
                self.confirm_drop = None;
                self.branch_prompt = None;
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
                self.store_agent(token, self.state.settings.agent_model.clone());
                Task::none()
            }
            Message::AgentModelChosen(model) => {
                self.store_agent(self.state.settings.agent_token.clone(), model);
                Task::none()
            }
            Message::AgentModelsLoaded(result) => {
                self.agent_busy = false;
                self.terminal.record(result.exchange);
                match result.ids {
                    Ok(mut ids) => {
                        self.agent_models_error = None;
                        let current = self.state.settings.agent_model.clone();
                        if !current.is_empty() && !ids.iter().any(|id| id == &current) {
                            ids.insert(0, current);
                        } else if self.state.settings.agent_model.is_empty() {
                            let chosen = ids
                                .iter()
                                .find(|id| id.as_str() == AppSettings::DEFAULT_AGENT_MODEL)
                                .cloned()
                                .unwrap_or_else(|| {
                                    ids.first()
                                        .cloned()
                                        .unwrap_or_else(|| AppSettings::DEFAULT_AGENT_MODEL.into())
                                });
                            self.store_agent(self.state.settings.agent_token.clone(), chosen);
                        }
                        self.agent_models = ids;
                    }
                    Err(error) => {
                        self.agent_models_error = Some(error_message(&error));
                    }
                }
                Task::none()
            }
            Message::SuggestCommit => self.suggest_commit_message(),
            Message::CommitSuggested(result) => {
                self.agent_busy = false;
                self.terminal.record(result.exchange);
                match result.subject {
                    Ok(subject) => self.commit_message = subject,
                    Err(error) => self.error = Some(error_message(&error)),
                }
                Task::none()
            }
            Message::ToggleTerminal => {
                self.terminal_open = !self.terminal_open;
                Task::none()
            }
            Message::ClearTerminal => {
                self.terminal.clear();
                Task::none()
            }
            Message::CloseSettings => {
                self.settings_open = false;
                Task::none()
            }
            Message::UiFontChosen(font) => {
                self.store_appearance(&font, self.state.settings.ui_size);
                Task::none()
            }
            Message::UiSizeChosen(size) => {
                let font = self.state.settings.ui_font.clone();
                self.store_appearance(&font, size);
                Task::none()
            }
            Message::PatchLoaded(generation, result) => {
                if generation != self.patch_generation {
                    return Task::none();
                }
                self.patch_pending = false;
                match result {
                    Ok(patches) => {
                        self.patches = patches;
                        self.patch_error = None;
                    }
                    Err(error) => {
                        self.patches.clear();
                        self.patch_error = Some(error_message(&error));
                    }
                }
                Task::none()
            }
            Message::AskDiscard => {
                let visible = visible_selection(
                    self.selection.as_ref(),
                    &self.state.branches,
                    &self.state.commits,
                    &self.state.stashes,
                    &self.state.tags,
                    self.state.current_status.as_ref(),
                );
                if let Selection::File { path, place } = visible {
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
                let Some(repo) = self.adapter.clone() else {
                    return Task::none();
                };
                if self.state.loading {
                    return Task::none();
                }
                self.confirm_discard = None;
                self.state.loading = true;
                self.error = None;
                Task::perform(
                    discard_in_background(repo, path, place),
                    Message::DiscardFinished,
                )
            }
            Message::DiscardFinished(result) => {
                self.state.loading = false;
                match result {
                    Ok(()) => Task::done(Message::RefreshRepository),
                    Err(error) => {
                        self.error = Some(error_message(&error));
                        Task::none()
                    }
                }
            }
            Message::StageHunk { path, index } => self.edit_hunk(path, index, true),
            Message::DiscardHunk { path, index } => self.edit_hunk(path, index, false),
            Message::HunkFinished(result) => {
                self.state.loading = false;
                match result {
                    Ok(()) => Task::done(Message::RefreshRepository),
                    Err(error) => {
                        self.error = Some(error_message(&error));
                        Task::none()
                    }
                }
            }
            Message::StashPush => self.run_stash(StashAction::Push),
            Message::StashApply(index) => self.run_stash(StashAction::Apply(index)),
            Message::StashPop(index) => self.run_stash(StashAction::Pop(index)),
            Message::AskDropStash => {
                if self.state.loading {
                    return Task::none();
                }
                let visible = visible_selection(
                    self.selection.as_ref(),
                    &self.state.branches,
                    &self.state.commits,
                    &self.state.stashes,
                    &self.state.tags,
                    self.state.current_status.as_ref(),
                );
                if let Selection::Stash { index, message } = visible {
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
                let still_there = self
                    .state
                    .stashes
                    .iter()
                    .any(|entry| entry.index == stash.index && entry.message == stash.message);
                if !still_there {
                    self.confirm_drop = None;
                    self.error = Some("That stash is no longer there.".to_string());
                    return Task::none();
                }
                self.run_stash(StashAction::Drop(stash.index))
            }
            Message::StashFinished(result) => {
                self.state.loading = false;
                match result {
                    Ok(()) => Task::done(Message::RefreshRepository),
                    Err(error) => {
                        self.error = Some(error_message(&error));
                        Task::none()
                    }
                }
            }
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
                Task::none()
            }
            Message::ConfirmBranch => self.confirm_branch(),
            Message::BranchFinished(result) => {
                self.state.loading = false;
                match result {
                    Ok(()) => Task::done(Message::RefreshRepository),
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
            Message::HistoryFinished(result) => {
                self.state.loading = false;
                match result {
                    Ok(()) => self.begin_refresh(true),
                    Err(error) => {
                        self.error = Some(error_message(&error));
                        self.begin_refresh(false)
                    }
                }
            }
        }
    }

    fn begin_refresh(&mut self, clear_error: bool) -> Task<Message> {
        let Some(repo) = self.adapter.clone() else {
            return Task::none();
        };
        if self.state.loading {
            return Task::none();
        }
        self.state.loading = true;
        if clear_error {
            self.error = None;
        }
        Task::perform(
            load_repository_in_background(repo),
            Message::RepositoryLoaded,
        )
    }

    fn ask_branch(&mut self, ask: BranchAsk) -> Task<Message> {
        if self.adapter.is_none() || self.state.loading {
            return Task::none();
        }
        let visible = self.shown_selection();
        let Selection::Branch { name, current } = visible else {
            return Task::none();
        };
        let prompt = match ask {
            BranchAsk::Create => {
                let start = if self.local_branch(&name).is_some() {
                    name
                } else {
                    "HEAD".to_string()
                };
                BranchPrompt::Create {
                    start,
                    name: String::new(),
                }
            }
            BranchAsk::Rename => {
                if self.local_branch(&name).is_none() {
                    return Task::none();
                }
                BranchPrompt::Rename {
                    to: name.clone(),
                    from: name,
                }
            }
            BranchAsk::Delete => {
                if current || self.local_branch(&name).is_none() {
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
                if start != "HEAD" && self.local_branch(&start).is_none() {
                    return self.branch_is_gone();
                }
                BranchAction::Create { name, start }
            }
            BranchPrompt::Rename { from, to } => {
                let to = to.trim().to_string();
                if to.is_empty() || to == from {
                    return Task::none();
                }
                if self.local_branch(&from).is_none() {
                    return self.branch_is_gone();
                }
                BranchAction::Rename { from, to }
            }
            BranchPrompt::Delete { name } => match self.local_branch(&name) {
                Some(branch) if branch.is_head => {
                    self.branch_prompt = None;
                    self.error = Some("The current branch cannot be deleted.".to_string());
                    return Task::none();
                }
                Some(_) => BranchAction::Delete { name },
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

    fn local_branch(&self, name: &str) -> Option<&Branch> {
        self.state
            .branches
            .iter()
            .find(|branch| !branch.is_remote && branch.name == name)
    }

    fn edit_marked(&mut self, stage: bool) -> Task<Message> {
        let paths = marked_paths(&self.marked_files, stage);
        if paths.is_empty() {
            Task::none()
        } else if stage {
            self.update(Message::EditWorktree(WorktreeEdit::Stage(paths)))
        } else {
            self.update(Message::EditWorktree(WorktreeEdit::Unstage(paths)))
        }
    }

    fn all_changed_files(&self) -> Vec<MarkedFile> {
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

    fn retain_marks(&mut self) {
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

    fn shown_selection(&self) -> Selection {
        visible_selection(
            self.selection.as_ref(),
            &self.state.branches,
            &self.state.commits,
            &self.state.stashes,
            &self.state.tags,
            self.state.current_status.as_ref(),
        )
    }

    fn run_branch(&mut self, action: BranchAction) -> Task<Message> {
        let Some(repo) = self.adapter.clone() else {
            return Task::none();
        };
        if self.state.loading {
            return Task::none();
        }
        self.branch_prompt = None;
        self.history_prompt = None;
        self.confirm_discard = None;
        self.confirm_drop = None;
        self.state.loading = true;
        self.error = None;
        Task::perform(branch_in_background(repo, action), Message::BranchFinished)
    }

    fn ask_history(&mut self, ask: HistoryAsk) -> Task<Message> {
        if self.adapter.is_none() || self.state.loading {
            return Task::none();
        }
        let visible = self.shown_selection();
        let prompt = match ask {
            HistoryAsk::CherryPick | HistoryAsk::Reset => {
                let Selection::Commit { id } = visible else {
                    return Task::none();
                };
                if !self.commit_loaded(&id) {
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
                if current || self.local_branch(&name).is_none() {
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
        if !self.commit_loaded(&id) {
            return self.commit_is_gone();
        }
        self.run_history(HistoryAction::CherryPick { id })
    }

    fn confirm_reset(&mut self, mode: ResetMode) -> Task<Message> {
        let Some(HistoryPrompt::Reset { id }) = self.history_prompt.clone() else {
            return Task::none();
        };
        if !self.commit_loaded(&id) {
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
        match self.local_branch(&name) {
            Some(branch) if branch.is_head => {
                self.history_prompt = None;
                self.error = Some(if merge {
                    "The current branch cannot be merged.".to_string()
                } else {
                    "The current branch cannot be rebased.".to_string()
                });
                Task::none()
            }
            Some(_) => {
                if merge {
                    self.run_history(HistoryAction::Merge { name })
                } else {
                    self.run_history(HistoryAction::Rebase { name })
                }
            }
            None => self.branch_is_gone(),
        }
    }

    fn commit_loaded(&self, id: &str) -> bool {
        self.state.commits.iter().any(|commit| commit.id == id)
    }

    fn commit_is_gone(&mut self) -> Task<Message> {
        self.history_prompt = None;
        self.error = Some("That commit is no longer there.".to_string());
        Task::none()
    }

    fn run_history(&mut self, action: HistoryAction) -> Task<Message> {
        let Some(repo) = self.adapter.clone() else {
            return Task::none();
        };
        if self.state.loading {
            return Task::none();
        }
        self.history_prompt = None;
        self.branch_prompt = None;
        self.confirm_discard = None;
        self.confirm_drop = None;
        self.state.loading = true;
        self.error = None;
        Task::perform(
            history_in_background(repo, action),
            Message::HistoryFinished,
        )
    }

    fn run_stash(&mut self, action: StashAction) -> Task<Message> {
        let Some(repo) = self.adapter.clone() else {
            return Task::none();
        };
        if self.state.loading {
            return Task::none();
        }
        self.confirm_drop = None;
        self.confirm_discard = None;
        self.state.loading = true;
        self.error = None;
        Task::perform(stash_in_background(repo, action), Message::StashFinished)
    }

    fn edit_hunk(&mut self, path: String, index: usize, stage: bool) -> Task<Message> {
        let Some(repo) = self.adapter.clone() else {
            return Task::none();
        };
        if self.state.loading {
            return Task::none();
        }
        let Some(hunk) = self.hunk_at(&path, index) else {
            return Task::none();
        };
        let place = self
            .selection
            .as_ref()
            .and_then(|selection| match selection {
                Selection::File { place, .. } => Some(*place),
                _ => None,
            })
            .unwrap_or(FileDisposition::Unstaged);
        self.state.loading = true;
        self.error = None;
        if stage {
            Task::perform(
                stage_hunk_in_background(repo, path, hunk),
                Message::HunkFinished,
            )
        } else {
            Task::perform(
                discard_hunk_in_background(repo, path, hunk, place),
                Message::HunkFinished,
            )
        }
    }

    pub(crate) fn subscription(&self) -> iced::Subscription<Message> {
        iced::Subscription::none()
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
            load_repository(&populated_repo()),
        ));
        assert!(!app.state.loading);
        assert!(app.state.current_status.is_some());
        assert_eq!(app.state.commits.len(), 2);
        assert_eq!(app.state.dag_nodes.len(), 2);
        assert_eq!(
            app.state.branches,
            vec![branch("main", true), branch("feature/x", false)]
        );
    }

    #[test]
    fn with_repo_opens_the_repository_view() {
        let app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        assert!(app.adapter.is_some());
    }

    #[test]
    fn welcome_has_no_repository() {
        assert!(GitApp::welcome().adapter.is_none());
    }

    #[test]
    fn open_repository_starts_loading_without_blocking() {
        let mut app = GitApp::welcome_with(
            unavailable_opener(),
            unavailable_initialiser(),
            unavailable_cloner(),
            AppSettings::default(),
            ignore_settings(),
        );
        app.error = Some("previous failure".to_string());

        let _ = app.update(Message::OpenRepository(r"D:\repo".to_string()));

        assert!(app.state.loading);
        assert!(app.adapter.is_none());
        assert_eq!(app.error, None);
    }

    #[test]
    fn repository_opened_replaces_the_welcome_screen() {
        let mut app = GitApp::welcome();
        app.state.loading = true;

        let _ = app.update(Message::RepositoryOpened(
            r"D:\repo".to_string(),
            Ok(OpenedRepository(Arc::new(FakeGitRepository::ok()))),
        ));

        assert!(app.adapter.is_some());
        assert_eq!(app.error, None);
        assert_eq!(app.repository_path.as_deref(), Some(r"D:\repo"));
    }

    #[test]
    fn opening_a_repository_keeps_the_kilo_connection() {
        let mut app = GitApp::welcome();
        app.list_models = Arc::new(|_| crate::ModelList {
            ids: Ok(vec!["kilo-auto/free".to_string()]),
            exchange: crate::ProviderExchange::kilo("connected"),
        });
        app.state.loading = true;

        let _ = app.update(Message::RepositoryOpened(
            r"D:\repo".to_string(),
            Ok(OpenedRepository(Arc::new(FakeGitRepository::ok()))),
        ));

        let listed = (app.list_models)(String::new());
        assert_eq!(listed.exchange.text, "connected");
        assert!(app.adapter.is_some());
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
        app.state.branches = vec![branch("dev", true), branch("feature", false)];
        app.branch_list_open = true;
        app.branch_query = "feat".to_string();

        let _ = app.update(Message::ChooseBranch("feature".to_string()));

        assert!(!app.branch_list_open);
        assert!(app.branch_query.is_empty());
        assert!(app.state.loading);
        assert_eq!(
            app.selection,
            Some(Selection::Branch {
                name: "feature".to_string(),
                current: false,
            })
        );
    }

    #[test]
    fn choosing_the_current_branch_only_closes_the_list() {
        let mut app = GitApp::with_repo(Arc::new(FakeGitRepository::ok()));
        app.state.branches = vec![branch("dev", true), branch("feature", false)];
        app.branch_list_open = true;

        let _ = app.update(Message::ChooseBranch("dev".to_string()));

        assert!(!app.branch_list_open);
        assert!(!app.state.loading);
        assert_eq!(
            app.selection,
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
        assert!(app.state.loading);
        assert_eq!(app.pending_checkout.as_deref(), Some("topic"));

        let _ = app.update(Message::BranchFinished(Ok(())));
        assert!(app.pending_checkout.is_none());
        assert!(app.state.loading);
        assert_eq!(
            app.selection,
            Some(Selection::Branch {
                name: "topic".to_string(),
                current: false,
            })
        );
    }

    #[test]
    fn creating_from_a_remote_uses_that_ref_as_the_start() {
        let mut app = GitApp::with_repo(Arc::new(FakeGitRepository::ok()));
        app.state.branches = vec![
            branch("main", true),
            Branch {
                name: "origin/main".to_string(),
                is_remote: true,
                is_head: false,
                tip: String::new(),
            },
        ];
        app.selection = Some(Selection::Remote {
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
        assert!(app.commit_message.is_empty());
    }

    #[test]
    fn the_agent_token_is_kept_on_the_settings() {
        let mut app = GitApp::with_repo(Arc::new(FakeGitRepository::ok()));
        let _ = app.update(Message::AgentTokenChanged("kilo-token".to_string()));
        assert_eq!(app.state.settings.agent_token, "kilo-token");
        assert_eq!(
            app.state.settings.agent_model,
            AppSettings::DEFAULT_AGENT_MODEL
        );
    }

    #[test]
    fn select_all_marks_every_change_and_clear_drops_them() {
        let mut app = GitApp::with_repo(Arc::new(FakeGitRepository::ok()));
        app.state.current_status = Some(core_domain::RepoStatus {
            staged_files: vec!["a.rs".to_string()],
            unstaged_files: vec!["b.rs".to_string()],
            untracked_files: vec![],
            tracking: None,
        });

        let _ = app.update(Message::SelectAllFiles);
        assert_eq!(app.marked_files.len(), 2);

        let _ = app.update(Message::ToggleFileMark {
            path: "b.rs".to_string(),
            place: FileDisposition::Unstaged,
        });
        assert_eq!(app.marked_files.len(), 1);

        let _ = app.update(Message::ClearFileSelection);
        assert!(app.marked_files.is_empty());
        assert!(matches!(app.selection, Some(Selection::Branch { .. })));
    }

    #[test]
    fn repository_open_failure_shows_an_error_on_the_welcome_screen() {
        let mut app = GitApp::welcome();
        app.state.loading = true;

        let _ = app.update(Message::RepositoryOpened(
            r"D:\repo".to_string(),
            Err(DomainError::Unknown("cannot open that folder".into())),
        ));

        assert!(app.adapter.is_none());
        assert!(!app.state.loading);
        assert_eq!(app.error.as_deref(), Some("cannot open that folder"));
    }

    #[test]
    fn blank_repository_path_does_not_call_the_opener() {
        let open: RepositoryOpener =
            Arc::new(|_| Err(DomainError::Unknown("opener was called".into())));
        let mut app = GitApp::welcome_with(
            open,
            unavailable_initialiser(),
            unavailable_cloner(),
            AppSettings::default(),
            ignore_settings(),
        );

        let _ = app.update(Message::OpenRepository("  ".to_string()));

        assert!(app.adapter.is_none());
        assert_eq!(app.error.as_deref(), Some("Choose a repository folder."));
    }

    #[test]
    fn cancelled_folder_dialog_leaves_the_welcome_screen() {
        let open: RepositoryOpener =
            Arc::new(|_| Err(DomainError::Unknown("opener was called".into())));
        let mut app = GitApp::welcome_with(
            open,
            unavailable_initialiser(),
            unavailable_cloner(),
            AppSettings::default(),
            ignore_settings(),
        );

        let _ = app.update(Message::RepositoryFolderChosen(None));

        assert!(app.adapter.is_none());
        assert_eq!(app.error, None);
    }

    #[test]
    fn open_repository_is_ignored_while_another_operation_runs() {
        let open: RepositoryOpener =
            Arc::new(|_| Err(DomainError::Unknown("opener was called".into())));
        let mut app = GitApp::welcome_with(
            open,
            unavailable_initialiser(),
            unavailable_cloner(),
            AppSettings::default(),
            ignore_settings(),
        );
        app.state.loading = true;
        app.error = Some("still showing".to_string());

        let _ = app.update(Message::OpenRepository(r"D:\repo".to_string()));

        assert!(app.adapter.is_none());
        assert_eq!(app.error.as_deref(), Some("still showing"));
    }

    #[test]
    fn refresh_marks_state_as_loading() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        let _ = app.update(Message::RefreshRepository);
        assert!(app.state.loading);
    }

    #[test]
    fn refresh_on_the_welcome_screen_does_nothing() {
        let mut app = GitApp::welcome();
        let _ = app.update(Message::RefreshRepository);
        assert!(!app.state.loading);
    }

    #[test]
    fn refresh_errors_are_shown() {
        let missing = FakeGitRepository {
            status_result: Err(DomainError::RepositoryNotFound),
            ..FakeGitRepository::ok()
        };
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        let _ = app.update(Message::RepositoryLoaded(load_repository(&missing)));
        assert_eq!(app.error.as_deref(), Some("Not a Git repository."));
    }

    #[test]
    fn checkout_on_the_welcome_screen_does_nothing() {
        let mut app = GitApp::welcome();
        let _ = app.update(Message::CheckoutBranch("main".to_string()));
        assert!(!app.state.loading);
    }

    #[test]
    fn checkout_starts_loading_and_clears_the_last_error() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.error = Some("previous failure".to_string());
        let _ = app.update(Message::CheckoutBranch("feature".to_string()));
        assert!(app.state.loading);
        assert_eq!(app.error, None);
    }

    #[test]
    fn checkout_is_ignored_while_another_operation_runs() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.state.loading = true;
        app.error = Some("still showing".to_string());
        let _ = app.update(Message::CheckoutBranch("feature".to_string()));
        assert_eq!(app.error.as_deref(), Some("still showing"));
    }

    #[test]
    fn successful_checkout_stops_loading_without_an_error() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.state.loading = true;
        let _ = app.update(Message::CheckoutFinished(Ok(())));
        assert!(!app.state.loading);
        assert_eq!(app.error, None);
    }

    #[test]
    fn failed_checkout_shows_why() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.state.loading = true;
        let reason = "error: Your local changes would be overwritten by checkout".to_string();
        let _ = app.update(Message::CheckoutFinished(Err(DomainError::Unknown(
            reason.clone(),
        ))));
        assert!(!app.state.loading);
        assert_eq!(app.error, Some(reason));
    }

    #[test]
    fn fetch_on_the_welcome_screen_does_nothing() {
        let mut app = GitApp::welcome();
        let _ = app.update(Message::Fetch);
        assert!(!app.state.loading);
    }

    #[test]
    fn fetch_starts_loading_and_clears_the_last_error() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.error = Some("previous failure".to_string());
        let _ = app.update(Message::Fetch);
        assert!(app.state.loading);
        assert_eq!(app.error, None);
    }

    #[test]
    fn fetch_is_ignored_while_another_operation_runs() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.state.loading = true;
        app.error = Some("still showing".to_string());
        let _ = app.update(Message::Fetch);
        assert_eq!(app.error.as_deref(), Some("still showing"));
    }

    #[test]
    fn successful_fetch_stops_loading_without_an_error() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.state.loading = true;
        let _ = app.update(Message::FetchFinished(Ok(())));
        assert!(!app.state.loading);
        assert_eq!(app.error, None);
    }

    #[test]
    fn failed_fetch_shows_git_message() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.state.loading = true;
        let reason = "fatal: No remote repository specified.".to_string();
        let _ = app.update(Message::FetchFinished(Err(DomainError::Unknown(
            reason.clone(),
        ))));
        assert!(!app.state.loading);
        assert_eq!(app.error, Some(reason));
    }

    #[test]
    fn push_on_the_welcome_screen_does_nothing() {
        let mut app = GitApp::welcome();
        let _ = app.update(Message::Push);
        assert!(!app.state.loading);
    }

    #[test]
    fn push_starts_loading_and_clears_the_last_error() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.error = Some("previous failure".to_string());
        let _ = app.update(Message::Push);
        assert!(app.state.loading);
        assert_eq!(app.error, None);
    }

    #[test]
    fn push_is_ignored_while_another_operation_runs() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.state.loading = true;
        app.error = Some("still showing".to_string());
        let _ = app.update(Message::Push);
        assert_eq!(app.error.as_deref(), Some("still showing"));
    }

    #[test]
    fn successful_push_stops_loading_without_an_error() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.state.loading = true;
        let _ = app.update(Message::PushFinished(Ok(())));
        assert!(!app.state.loading);
        assert_eq!(app.error, None);
    }

    #[test]
    fn failed_push_shows_git_message() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.state.loading = true;
        let reason = "fatal: The current branch main has no upstream branch.".to_string();
        let _ = app.update(Message::PushFinished(Err(DomainError::Unknown(
            reason.clone(),
        ))));
        assert!(!app.state.loading);
        assert_eq!(app.error, Some(reason));
    }

    #[test]
    fn pull_on_the_welcome_screen_does_nothing() {
        let mut app = GitApp::welcome();
        let _ = app.update(Message::Pull);
        assert!(!app.state.loading);
    }

    #[test]
    fn pull_starts_loading_and_clears_the_last_error() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.error = Some("previous failure".to_string());
        let _ = app.update(Message::Pull);
        assert!(app.state.loading);
        assert_eq!(app.error, None);
    }

    #[test]
    fn pull_is_ignored_while_another_operation_runs() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.state.loading = true;
        app.error = Some("still showing".to_string());
        let _ = app.update(Message::Pull);
        assert_eq!(app.error.as_deref(), Some("still showing"));
    }

    #[test]
    fn successful_pull_stops_loading_without_an_error() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.state.loading = true;
        let _ = app.update(Message::PullFinished(Ok(())));
        assert!(!app.state.loading);
        assert_eq!(app.error, None);
    }

    #[test]
    fn failed_pull_shows_git_message() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.state.loading = true;
        let reason = "fatal: Not possible to fast-forward, aborting.".to_string();
        let _ = app.update(Message::PullFinished(Err(DomainError::Unknown(
            reason.clone(),
        ))));
        assert!(!app.state.loading);
        assert_eq!(app.error, Some(reason));
    }

    #[test]
    fn edit_on_the_welcome_screen_does_nothing() {
        let mut app = GitApp::welcome();
        let _ = app.update(Message::EditWorktree(WorktreeEdit::Stage(vec![
            "a.txt".to_string()
        ])));
        assert!(!app.state.loading);
    }

    #[test]
    fn edit_starts_loading_and_clears_the_last_error() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.error = Some("previous failure".to_string());
        let _ = app.update(Message::EditWorktree(WorktreeEdit::Stage(vec![
            "a.txt".to_string()
        ])));
        assert!(app.state.loading);
        assert_eq!(app.error, None);
    }

    #[test]
    fn edit_is_ignored_while_another_operation_runs() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.state.loading = true;
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
        assert!(!app.state.loading);
        assert_eq!(app.error.as_deref(), Some("previous failure"));
    }

    #[test]
    fn successful_commit_clears_the_message_and_stops_loading() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.state.loading = true;
        app.commit_message = "record the change".to_string();
        let _ = app.update(Message::WorktreeEditFinished(
            WorktreeEdit::Commit("record the change".to_string()),
            Ok(()),
        ));
        assert!(!app.state.loading);
        assert_eq!(app.error, None);
        assert_eq!(app.commit_message, "");
    }

    #[test]
    fn successful_stage_keeps_the_commit_message() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.state.loading = true;
        app.commit_message = "still drafting".to_string();
        let _ = app.update(Message::WorktreeEditFinished(
            WorktreeEdit::Stage(vec!["a.txt".to_string()]),
            Ok(()),
        ));
        assert!(!app.state.loading);
        assert_eq!(app.commit_message, "still drafting");
    }

    #[test]
    fn failed_edit_shows_why() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.state.loading = true;
        let reason = "error: hook declined to update".to_string();
        let _ = app.update(Message::WorktreeEditFinished(
            WorktreeEdit::Commit("record the change".to_string()),
            Err(DomainError::Unknown(reason.clone())),
        ));
        assert!(!app.state.loading);
        assert_eq!(app.error, Some(reason));
    }

    #[test]
    fn commit_message_changes_are_kept() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        let _ = app.update(Message::CommitMessageChanged(
            "record the change".to_string(),
        ));
        assert_eq!(app.commit_message, "record the change");
        assert!(!app.state.loading);
    }

    #[test]
    fn a_blank_init_path_does_not_start() {
        let mut app = GitApp::welcome();
        let _ = app.update(Message::InitRepository("  ".to_string()));
        assert!(!app.state.loading);
        assert!(app.adapter.is_none());
        assert!(app.error.is_some());
    }

    #[test]
    fn init_failure_stays_on_the_welcome_screen() {
        let mut app = GitApp::welcome();
        let _ = app.update(Message::InitRepository(r"D:\fresh".to_string()));
        assert!(app.state.loading);
        let _ = app.update(Message::InitFinished(
            r"D:\fresh".to_string(),
            Err(DomainError::Unknown(
                "Already a Git repository.".to_string(),
            )),
        ));
        assert!(app.adapter.is_none());
        assert!(!app.state.loading);
        assert_eq!(app.error.as_deref(), Some("Already a Git repository."));
    }

    #[test]
    fn init_then_opens_the_repository() {
        let mut app = GitApp::welcome();
        let _ = app.update(Message::InitFinished(r"D:\fresh".to_string(), Ok(())));
        assert!(app.state.loading);
        assert!(app.adapter.is_none());
        let _ = app.update(Message::RepositoryOpened(
            r"D:\fresh".to_string(),
            Ok(OpenedRepository(Arc::new(FakeGitRepository::ok()))),
        ));
        assert!(app.adapter.is_some());
        assert_eq!(app.error, None);
    }

    #[test]
    fn an_empty_clone_url_does_not_start() {
        let mut app = GitApp::welcome();
        let _ = app.update(Message::ChooseCloneFolder);
        assert!(!app.state.loading);
        assert!(app.adapter.is_none());
    }

    #[test]
    fn clone_failure_stays_on_the_welcome_screen() {
        let mut app = GitApp::welcome();
        let _ = app.update(Message::CloneFolderChosen(
            "https://example.com/repo.git".to_string(),
            Some(r"D:\parent".to_string()),
        ));
        assert!(app.state.loading);
        let _ = app.update(Message::CloneFinished(Err(DomainError::Unknown(
            "repo already exists in D:\\parent".to_string(),
        ))));
        assert!(app.adapter.is_none());
        assert!(!app.state.loading);
        assert_eq!(
            app.error.as_deref(),
            Some("repo already exists in D:\\parent")
        );
    }

    #[test]
    fn clone_then_opens_the_created_directory() {
        let mut app = GitApp::welcome();
        let _ = app.update(Message::CloneFinished(Ok(r"D:\parent\repo".to_string())));
        assert!(app.state.loading);
        let _ = app.update(Message::RepositoryOpened(
            r"D:\parent\repo".to_string(),
            Ok(OpenedRepository(Arc::new(FakeGitRepository::ok()))),
        ));
        assert!(app.adapter.is_some());
        assert_eq!(app.repository_path.as_deref(), Some(r"D:\parent\repo"));
    }

    #[test]
    fn stash_push_starts_loading_and_is_ignored_while_busy() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        let _ = app.update(Message::StashPush);
        assert!(app.state.loading);
        app.error = Some("still showing".to_string());
        let _ = app.update(Message::StashApply(0));
        assert_eq!(app.error.as_deref(), Some("still showing"));
    }

    #[test]
    fn dropping_a_stash_waits_for_confirm() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.state.stashes = vec![Stash {
            index: 0,
            message: "WIP on dev".to_string(),
        }];
        app.selection = Some(Selection::Stash {
            index: 0,
            message: "WIP on dev".to_string(),
        });

        let _ = app.update(Message::AskDropStash);
        assert_eq!(app.confirm_drop.as_ref().map(|stash| stash.index), Some(0));
        assert!(!app.state.loading);

        let _ = app.update(Message::ConfirmDropStash);
        assert!(app.confirm_drop.is_none());
        assert!(app.state.loading);
    }

    #[test]
    fn drop_refuses_a_stash_whose_slot_moved() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.confirm_drop = Some(Stash {
            index: 0,
            message: "WIP on dev".to_string(),
        });
        app.state.stashes = vec![Stash {
            index: 0,
            message: "On main: other".to_string(),
        }];

        let _ = app.update(Message::ConfirmDropStash);
        assert!(!app.state.loading);
        assert!(app.confirm_drop.is_none());
        assert_eq!(app.error.as_deref(), Some("That stash is no longer there."));
    }

    fn on_branch(name: &str, current: bool) -> GitApp {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.state.branches = vec![branch(name, current)];
        app.selection = Some(Selection::Branch {
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
        assert!(!app.state.loading);

        let _ = app.update(Message::ConfirmBranch);
        assert!(!app.state.loading);

        let _ = app.update(Message::BranchNameChanged(" topic ".to_string()));
        let _ = app.update(Message::ConfirmBranch);
        assert!(app.branch_prompt.is_none());
        assert!(app.state.loading);
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
        assert!(!app.state.loading);

        let _ = app.update(Message::BranchNameChanged("feature-2".to_string()));
        let _ = app.update(Message::ConfirmBranch);
        assert!(app.state.loading);
        assert!(app.branch_prompt.is_none());
    }

    #[test]
    fn deleting_the_current_branch_does_not_open() {
        let mut app = on_branch("main", true);
        let _ = app.update(Message::AskDeleteBranch);
        assert!(app.branch_prompt.is_none());
        assert!(!app.state.loading);
    }

    #[test]
    fn deleting_another_branch_waits_for_confirm() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.state.branches = vec![branch("main", true), branch("feature", false)];
        app.selection = Some(Selection::Branch {
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
        assert!(app.state.loading);
        assert!(app.branch_prompt.is_none());
    }

    #[test]
    fn delete_refuses_a_branch_that_became_current() {
        let mut app = on_branch("main", true);
        app.branch_prompt = Some(BranchPrompt::Delete {
            name: "main".to_string(),
        });
        let _ = app.update(Message::ConfirmBranch);
        assert!(!app.state.loading);
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
        assert!(!app.state.loading);
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
        assert!(app.patch_pending);
        assert!(app.patches.is_empty());
    }

    #[test]
    fn a_missing_tag_is_cleared_when_the_repository_reloads() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.selection = Some(Selection::Tag {
            name: "gone".to_string(),
            commit_id: "abc1234".to_string(),
        });
        let _ = app.update(Message::RepositoryLoaded(load_repository(
            &FakeGitRepository::ok(),
        )));
        assert!(app.selection.is_none());
    }

    #[test]
    fn cherry_pick_waits_for_confirm_and_a_missing_commit_is_refused() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.state.commits = vec![commit("abc1234")];
        app.selection = Some(Selection::Commit {
            id: "abc1234".to_string(),
        });
        let _ = app.update(Message::AskCherryPick);
        assert_eq!(
            app.history_prompt,
            Some(HistoryPrompt::CherryPick {
                id: "abc1234".to_string(),
            })
        );
        app.state.commits.clear();
        let _ = app.update(Message::ConfirmCherryPick);
        assert!(!app.state.loading);
        assert!(app.history_prompt.is_none());
        assert_eq!(
            app.error.as_deref(),
            Some("That commit is no longer there.")
        );
    }

    #[test]
    fn reset_hard_starts_after_confirm() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.state.commits = vec![commit("abc1234")];
        app.selection = Some(Selection::Commit {
            id: "abc1234".to_string(),
        });
        let _ = app.update(Message::AskReset);
        assert!(matches!(
            app.history_prompt,
            Some(HistoryPrompt::Reset { .. })
        ));
        let _ = app.update(Message::ConfirmReset(ResetMode::Hard));
        assert!(app.state.loading);
        assert!(app.history_prompt.is_none());
    }

    #[test]
    fn merge_and_rebase_wait_on_another_branch_and_skip_the_current_one() {
        let mut current = on_branch("main", true);
        let _ = current.update(Message::AskMerge);
        let _ = current.update(Message::AskRebase);
        assert!(current.history_prompt.is_none());

        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.state.branches = vec![branch("main", true), branch("feature", false)];
        app.selection = Some(Selection::Branch {
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
        assert!(app.state.loading);

        let mut rebase = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        rebase.state.branches = vec![branch("main", true), branch("feature", false)];
        rebase.selection = Some(Selection::Branch {
            name: "feature".to_string(),
            current: false,
        });
        let _ = rebase.update(Message::AskRebase);
        let _ = rebase.update(Message::ConfirmRebase);
        assert!(rebase.state.loading);
        assert!(rebase.history_prompt.is_none());
    }

    #[test]
    fn a_failed_history_command_keeps_git_message_and_reloads() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.state.loading = true;
        let _ = app.update(Message::HistoryFinished(Err(DomainError::Unknown(
            "conflict in a.txt".to_string(),
        ))));
        assert_eq!(app.error.as_deref(), Some("conflict in a.txt"));
        assert!(app.state.loading);
    }
}
