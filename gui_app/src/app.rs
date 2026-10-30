use std::sync::Arc;

use core_domain::{
    AppSettings, DomainError, FileDisposition, Patch, ResetMode, Stash, ViewEvent, Worktree,
    WorktreeTabRetention,
};
use iced::widget::{pane_grid, text_editor};
use iced::Task;

use crate::appearance::{font_family, font_size};
use crate::changes::{marked_paths, matching_change_paths, MarkedFile};
use crate::context_menu::{gitignore_pattern, ContextTarget};
use crate::jobs::{
    add_worktree_in_background, append_gitignore_in_background, branch_in_background,
    checkout_in_background, choose_repository_folder, clone_in_background,
    discard_hunk_in_background, discard_paths_in_background,
    edit_in_background, error_message, fetch_in_background, history_in_background,
    init_in_background, list_models_in_background, load_patch_in_background,
    load_repository_in_background, open_repository_in_background, pull_in_background,
    push_in_background, remove_worktree_in_background, save_settings_in_background,
    compare_in_background, shell_stream, stage_hunk_in_background, stash_in_background,
    stop_shell, suggest_commit_in_background, worktrees_in_background, BranchAction,
    HistoryAction, SharedRepository, ShellEvent, StashAction, WorktreeEdit,
};
use crate::compare::{CompareSide, CompareState};
use crate::terminal::{ConsoleChange, LineKind};
use crate::layout::{self, HunkMode, LayoutPreset, RatioSlot, WorkspacePane};
use crate::selection::Selection;
use crate::session::{apply_retention, same_path, WorktreeSession, DIFF_LINES_PER_PAGE};
use crate::theme::DesignTokens;
use crate::{
    RepositoryCloner, RepositoryInitialiser, RepositoryOpener, SettingsSaver, WorktreeOpener,
};

/// Rail visibility and hunk mode to restore when leaving [`HunkMode::Focus`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FocusRestore {
    pub files_rail_open: bool,
    pub inspector_open: bool,
    pub hunk_mode: HunkMode,
}

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

/// Text-input id of the top-bar history search (focused by Ctrl+K).
pub(crate) const HISTORY_SEARCH_ID: &str = "history-search";
/// Scrollable id of the Diff pane, for jumping between differences.
pub(crate) const DIFF_SCROLL_ID: &str = "diff-scroll";
/// Text-input id of the stash dialog's message.
pub(crate) const STASH_MESSAGE_ID: &str = "stash-message";
/// Text-input id of the branches popup filter.
pub(crate) const BRANCH_POPUP_ID: &str = "branch-popup-filter";
/// Text-input id of the quick-open palette field.
pub(crate) const PALETTE_INPUT_ID: &str = "quick-open";
/// Text-input id of the terminal's command line.
pub(crate) const SHELL_INPUT_ID: &str = "shell-input";
/// Text-input id of the Repositories filter.
pub(crate) const REPOSITORY_FILTER_ID: &str = "repository-filter";
/// Terminal height to start with, and its limits while dragging.
pub(crate) const TERMINAL_HEIGHT: f32 = 240.0;
const TERMINAL_MIN_HEIGHT: f32 = 110.0;
const TERMINAL_MAX_HEIGHT: f32 = 900.0;

/// The open Ctrl+P palette: its query and the highlighted row among the ranked results.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PaletteState {
    pub query: String,
    pub selected: usize,
}

pub(crate) struct GitApp {
    /// Top-bar search: narrows the history table. Window-wide, not per tab.
    pub(crate) history_query: String,
    /// Quick-open palette, when showing.
    pub(crate) palette: Option<PaletteState>,
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
    /// Right-click menu over a commit or local branch.
    pub(crate) context_menu: Option<crate::context_menu::ContextMenuState>,
    /// Commit id waiting for detach confirmation when the worktree is dirty.
    pub(crate) confirm_detach: Option<String>,
    /// Icon-button hover: key, title, when the pointer entered. Tip shows after two seconds.
    pub(crate) tip_hover: Option<(String, String, std::time::Instant)>,
    /// Tip key whose hover has lasted long enough to show the title.
    pub(crate) tip_ready_key: Option<String>,
    /// Last pointer position in window coordinates (for tips and context menus).
    pub(crate) cursor: (f32, f32),
    /// Dragging an Explorer section height grip: section, start y, start weight.
    /// The last branch-tree click, to recognise a double-click (checkout).
    last_ref_click: Option<(String, std::time::Instant)>,
    /// Branch-tree filter text ("Branch or tag"). Window-wide.
    pub(crate) ref_filter: String,
    /// Authoritative, global settings. Fanned into each session's `AppState`.
    pub(crate) settings: AppSettings,
    /// True while a welcome-screen open/init/clone is in flight (no session yet).
    pub(crate) opening: bool,
    pub(crate) panes: pane_grid::State<WorkspacePane>,
    pub(crate) layout: LayoutPreset,
    ratios: [layout::LayoutRatios; LayoutPreset::COUNT],
    /// Rebuilt whenever [`Self::install_layout`] constructs a new pane grid.
    split_slots: Vec<(pane_grid::Split, RatioSlot)>,
    pub(crate) files_rail_open: bool,
    pub(crate) inspector_open: bool,
    pub(crate) hunk_mode: HunkMode,
    focus_restore: Option<FocusRestore>,
    pub(crate) branch_list_open: bool,
    pub(crate) branch_popup_filter: String,
    pub(crate) files_as_tree: bool,
    /// Diff drawn side by side (true) or as one unified column.
    pub(crate) diff_split: bool,
    /// Keyboard modifiers held now.
    modifiers: iced::keyboard::Modifiers,
    /// Window width from the last resize event; 0 until the first one.
    window_width: f32,
    /// Explorer width the user chose, kept when the window is resized.
    files_px: f32,
    pub(crate) agent_models: Vec<String>,
    pub(crate) agent_models_error: Option<String>,
    pub(crate) agent_busy: bool,
    /// Scrollback and history. `terminal_log` mirrors its text for selection and copy.
    pub(crate) console: crate::terminal::Console,
    pub(crate) terminal_log: text_editor::Content,
    pub(crate) terminal_open: bool,
    /// Height the user dragged the terminal to.
    pub(crate) terminal_height: f32,
    /// Dragging the terminal's top edge: pointer y and height when the drag began.
    terminal_drag: Option<(f32, f32)>,
    /// Draft line for the interactive terminal.
    pub(crate) shell_input: String,
    /// A shell command is running in the active worktree.
    pub(crate) shell_busy: bool,
    /// Folder the next command starts in, after a `cd`. `None` is the active tab's folder.
    pub(crate) shell_cwd: Option<String>,
    /// Process of the running command, for Stop and Ctrl+C.
    shell_pid: Option<u32>,
    /// Numbers each command so output from one that was stopped cannot land in the next.
    shell_run: u64,
    /// The Repositories column folded to a rail is showing its list over the window.
    pub(crate) repositories_flyout: bool,
    pub(crate) repository_filter: String,
    /// The Compare window, when open.
    pub(crate) compare: Option<CompareState>,
    compare_generation: u64,
    pub(crate) list_models: crate::ModelLister,
    pub(crate) suggest_commit: crate::CommitSuggester,
    pub(crate) error: Option<String>,
    /// Status-bar error shows full text when true; first line only when false.
    pub(crate) error_expanded: bool,
    pub(crate) tokens: DesignTokens,
    pub(crate) settings_open: bool,
    /// Paths to discard after the confirm dialog (one file, or every match under a folder).
    pub(crate) confirm_discard: Option<Vec<MarkedFile>>,
    pub(crate) confirm_drop: Option<Stash>,
    pub(crate) branch_prompt: Option<BranchPrompt>,
    pub(crate) history_prompt: Option<HistoryPrompt>,
    pub(crate) stash_prompt: Option<StashPrompt>,
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
    /// Ctrl, Shift… as now held, for Ctrl- and Shift-click in the working tree.
    ModifiersChanged(iced::keyboard::Modifiers),
    BranchPopupFilterChanged(String),
    /// A branch row in the branches popup: its menu opens at the pointer.
    BranchPopupPick { name: String, remote: bool },
    /// Right-click on a working-tree file: the menu acts on the selection.
    ChangeContextMenu { path: String, place: FileDisposition },
    /// The window is this wide now.
    WindowResized(f32),
    SetLayout(LayoutPreset),
    ToggleFilesRail,
    /// Collapse or expand one Explorer accordion section for the active session.
    ToggleExplorerSection(crate::session::ExplorerSection),
    /// Branch-tree filter text changed.
    RefFilterChanged(String),
    /// Open or close a branch-tree folder by key.
    ToggleRefFolder(String),
    /// A branch in the tree was clicked: filter the log to it; a second click soon after
    /// checks out a local branch.
    RefClicked { name: String, remote: bool },
    /// Drop the log's branch filter.
    ClearBranchFilter,
    /// The filter bar's Branch ▾: All, HEAD, or one branch.
    BranchChoicePicked(crate::log_filter::LogChoice),
    /// The filter bar's User ▾: All, or one author.
    AuthorChoicePicked(crate::log_filter::LogChoice),
    ToggleBranchList,
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
    SetHunkMode(HunkMode),
    /// Beside ↔ Wide (Focus is explicit via [`Message::SetHunkMode`]).
    CycleHunkMode,
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
    /// User typed in the terminal command field.
    ShellInputChanged(String),
    /// User pressed Enter to run the terminal command.
    ShellSubmit,
    /// Output, start or end of the command numbered `run`.
    Shell(u64, ShellEvent),
    /// Stop button, or Ctrl+C while the command line has focus.
    ShellInterrupt,
    /// Ctrl+C anywhere; stops the command only when the command line has focus.
    CtrlCPressed,
    /// Ctrl+C was pressed; `focused` says whether the command line had focus.
    ShellCtrlC { focused: bool },
    /// Up / Down. `focused` says whether the command line had focus.
    ShellHistory { step: i32, focused: bool },
    /// Click in the terminal: put the caret on the command line.
    FocusShell,
    /// The terminal's top edge was grabbed.
    TerminalDragStart,
    /// The mouse button went up anywhere.
    PointerReleased,
    /// Pin or unpin the Repositories column.
    ToggleRepositoriesPinned,
    /// Show or hide the Repositories list over the window while the column is unpinned.
    ToggleRepositoriesFlyout,
    RepositoryFilterChanged(String),
    /// Take a repository off the Repositories list. The folder is left alone.
    ForgetRepository(String),
    /// Settings: how the header draws the app name.
    HeaderMarkChosen(core_domain::HeaderMark),
    /// Open the Compare window: a file (else the selected or first changed file), and
    /// optionally the revision to compare the working tree against.
    OpenCompare { path: Option<String>, against: Option<String> },
    ComparePathChanged(String),
    /// Run the compare for the path typed.
    ComparePathSubmitted,
    /// A changed file picked from the list.
    ComparePathPicked(String),
    /// A side picked from a list: `left` true for the left side.
    CompareSidePicked { left: bool, side: CompareSide },
    CompareDraftChanged { left: bool, text: String },
    /// Enter in a side's revision field.
    CompareDraftSubmitted { left: bool },
    CompareSwap,
    CompareLoaded { generation: u64, result: Result<Patch, DomainError> },
    CloseCompare,
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
    /// Side by side ↔ unified.
    ToggleDiffSplit,
    /// Jump to the next (true) or previous difference.
    StepDiffChange(bool),
    AskDiscard,
    CancelDiscard,
    ConfirmDiscard,
    DiscardFinished(String, Result<(), DomainError>),
    /// Finished appending patterns to `.gitignore`.
    IgnoreFinished(String, Result<(), DomainError>),
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
    /// Open the stash dialog for these paths (empty: every tracked change).
    AskStash(Vec<String>),
    StashMessageChanged(String),
    StashUntrackedToggled(bool),
    ConfirmStash,
    CancelStash,
    MergeTargetPicked(String),
    MergeNoFfToggled(bool),
    /// Stop the merge, rebase… in progress.
    AbortOperation,
    /// Commit the merge, or continue the rebase…, once conflicts are resolved.
    ContinueOperation,
    /// Keep one side of a conflicted file and stage it.
    ResolveConflict { path: String, side: core_domain::ConflictSide },
    /// `git add` a conflicted file the user fixed by hand.
    MarkResolved(String),
    StashApply(usize),
    StashPop(usize),
    AskDropStash,
    CancelDropStash,
    ConfirmDropStash,
    StashFinished(String, Result<(), DomainError>),
    AskCreateBranch,
    /// Open the New Branch dialog at any ref or commit (context menus).
    AskCreateBranchFrom(String),
    /// The New Branch dialog's "Checkout branch" box.
    BranchCheckoutToggled(bool),
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
    /// Prefill the create dialog from a context-menu target.
    AskCreateWorktreeFrom {
        branch: String,
        create_branch: bool,
        start_point: Option<String>,
    },
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
    /// Toggle Focus-current on the active session's graph.
    ToggleFocusCurrentBranch,
    /// Open the right-click menu at the current pointer (`cursor`).
    OpenContextMenu {
        target: crate::context_menu::ContextTarget,
    },
    CloseContextMenu,
    /// Pointer moved; keeps context menus and delayed tips near the cursor.
    CursorMoved {
        x: f32,
        y: f32,
    },
    /// Run a context-menu action (closes the menu first).
    ContextMenuAction(crate::context_menu::ContextActionKind),
    /// Detach HEAD to this commit; confirms when the worktree is dirty.
    AskDetachHead {
        commit_id: String,
    },
    ConfirmDetachHead,
    CancelDetach,
    /// Pointer entered an icon button; start the two-second tip timer.
    IconTipEnter {
        key: String,
        title: String,
    },
    IconTipLeave {
        key: String,
    },
    /// Poll while a tip hover is pending.
    IconTipTick,
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
    /// Top-bar search text changed.
    HistoryQueryChanged(String),
    /// Ctrl+K: put the caret in the top-bar search.
    FocusHistorySearch,
    /// Ctrl+P: open the palette (closes it when already open). The text prefills the query.
    TogglePalette(String),
    PaletteQueryChanged(String),
    /// Arrow keys: move the highlighted palette row by this many rows.
    PaletteMove(i32),
    /// Enter in the palette field: activate the highlighted row.
    PaletteSubmit,
    /// Click on a palette row: its position among the ranked results.
    PaletteActivate(usize),
}

/// The terminal, carried across the rebuild that opening another repository does.
struct KeptTerminal {
    console: crate::terminal::Console,
    log: text_editor::Content,
    open: bool,
    height: f32,
    input: String,
    busy: bool,
    pid: Option<u32>,
    run: u64,
}

/// A worktree create or remove waiting for the user to confirm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WorktreePrompt {
    Create {
        path: String,
        branch: String,
        /// `-b`: make `branch`. Off checks out an existing local branch.
        create_branch: bool,
        /// When creating a branch, tip to start from (`None` = HEAD).
        start_point: Option<String>,
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
    /// `name` is empty until the user picks a branch; `no_ff` records a merge commit always.
    Merge { name: String, no_ff: bool },
    Rebase { name: String },
}

/// A stash waiting for its message. `paths` empty stashes every tracked change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StashPrompt {
    pub message: String,
    pub include_untracked: bool,
    pub paths: Vec<String>,
}

/// A create, rename, or delete waiting for the user to confirm the name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BranchPrompt {
    /// `checkout` switches to the new branch once it exists (JetBrains' default).
    Create {
        start: String,
        name: String,
        checkout: bool,
    },
    Rename { from: String, to: String },
    Delete { name: String },
}

enum BranchAsk {
    Create,
    Rename,
    Delete,
}

#[derive(Clone, Copy)]
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
        let panes = layout::new_panes(layout, &ratios[layout.index()], true, true);
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
            context_menu: None,
            confirm_detach: None,
            tip_hover: None,
            tip_ready_key: None,
            cursor: (0.0, 0.0),
            last_ref_click: None,
            ref_filter: String::new(),
            settings,
            opening: false,
            panes,
            layout,
            ratios,
            split_slots,
            files_rail_open: true,
            inspector_open: true,
            hunk_mode: HunkMode::Beside,
            focus_restore: None,
            branch_list_open: false,
            branch_popup_filter: String::new(),
            files_as_tree: true,
            diff_split: true,
            modifiers: iced::keyboard::Modifiers::default(),
            window_width: 0.0,
            files_px: layout::FILES_PX,
            agent_models: Vec::new(),
            agent_models_error: None,
            agent_busy: false,
            console: crate::terminal::Console::new(),
            terminal_log: text_editor::Content::new(),
            // Collapsed by default: it costs History rows. Ctrl+` / the status bar opens it.
            terminal_open: false,
            terminal_height: TERMINAL_HEIGHT,
            terminal_drag: None,
            shell_input: String::new(),
            shell_busy: false,
            shell_cwd: None,
            shell_pid: None,
            shell_run: 0,
            repositories_flyout: false,
            repository_filter: String::new(),
            compare: None,
            compare_generation: 0,
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
            stash_prompt: None,
            clone_url: String::new(),
            history_query: String::new(),
            palette: None,
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
        self.context_menu = None;
        self.confirm_detach = None;
    }

    #[cfg(test)]
    fn active_path(&self) -> String {
        self.active_session()
            .map(|session| session.path.clone())
            .unwrap_or_default()
    }

    /// The merge, rebase, cherry-pick or revert the active tab is in the middle of.
    pub(crate) fn current_operation(&self) -> Option<core_domain::RepoOperation> {
        self.active_session()?.state.current_status.as_ref()?.operation
    }

    /// In every preset but Stacked the Explorer is a column, so its split is a width.
    fn files_split_is_a_width(&self) -> bool {
        self.layout != LayoutPreset::Stacked
    }

    /// Width of the pane grid: the window less the Repositories column beside it.
    /// 0 until the first resize event.
    fn grid_width(&self) -> f32 {
        if self.window_width <= 0.0 {
            return 0.0;
        }
        (self.window_width - crate::view::repositories_width(self.settings.repositories_pinned)).max(0.0)
    }

    /// Hold the Explorer at the width the user chose while the window changes size.
    fn keep_files_width(&mut self) {
        let grid = self.grid_width();
        if !self.files_split_is_a_width() || grid <= 0.0 {
            return;
        }
        let ratio = layout::files_ratio_for(self.files_px, grid);
        self.ratios[self.layout.index()].files = ratio;
        let split = self
            .split_slots
            .iter()
            .find(|(_, slot)| *slot == RatioSlot::Files)
            .map(|(split, _)| *split);
        if let Some(split) = split {
            self.panes.resize(split, ratio);
        }
    }

    fn install_layout(&mut self) {
        let grid = self.grid_width();
        if self.files_split_is_a_width() && grid > 0.0 {
            self.ratios[self.layout.index()].files = layout::files_ratio_for(self.files_px, grid);
        }
        let base = self.ratios[self.layout.index()];
        let ratios = base.for_hunk_mode(self.hunk_mode);
        self.panes = layout::new_panes(
            self.layout,
            &ratios,
            self.files_rail_open,
            self.inspector_open,
        );
        self.split_slots = layout::split_slots(&self.panes);
    }

    fn apply_hunk_mode(&mut self, mode: HunkMode) {
        if mode == HunkMode::Focus {
            if self.hunk_mode != HunkMode::Focus {
                self.focus_restore = Some(FocusRestore {
                    files_rail_open: self.files_rail_open,
                    inspector_open: self.inspector_open,
                    hunk_mode: self.hunk_mode,
                });
                self.files_rail_open = false;
                self.inspector_open = false;
            }
            self.hunk_mode = HunkMode::Focus;
        } else {
            if self.hunk_mode == HunkMode::Focus {
                if let Some(restore) = self.focus_restore.take() {
                    self.files_rail_open = restore.files_rail_open;
                    self.inspector_open = restore.inspector_open;
                }
            }
            self.hunk_mode = mode;
        }
        self.install_layout();
    }

    fn restore_focus_mode(&mut self) -> bool {
        if self.hunk_mode != HunkMode::Focus {
            return false;
        }
        if let Some(restore) = self.focus_restore.take() {
            self.files_rail_open = restore.files_rail_open;
            self.inspector_open = restore.inspector_open;
            self.hunk_mode = restore.hunk_mode;
        } else {
            self.files_rail_open = true;
            self.inspector_open = true;
            self.hunk_mode = HunkMode::Beside;
        }
        self.install_layout();
        true
    }

    /// Scroll the Diff pane to the next or previous difference, using the same layout the
    /// pane is drawn with.
    fn step_diff_change(&mut self, forward: bool) -> Task<Message> {
        let split = self.diff_split;
        let Some(session) = self.active_session_mut() else {
            return Task::none();
        };
        let layout = crate::diff_view::layout(
            &session.patches,
            session.focused_diff_path.as_deref(),
            session.diff_lines_limit,
            split,
        );
        let changes = layout.changes();
        let Some(next) = crate::diff_view::step(session.diff_change, changes.len(), forward) else {
            return Task::none();
        };
        session.diff_change = Some(next);
        iced::widget::scrollable::scroll_to(
            iced::widget::scrollable::Id::new(DIFF_SCROLL_ID),
            iced::widget::scrollable::AbsoluteOffset { x: 0.0, y: layout.offset_of(changes[next]) },
        )
    }

    fn start_patch(&mut self) -> Task<Message> {
        let Some(session) = self.active_session_mut() else {
            return Task::none();
        };
        session.diff_lines_limit = DIFF_LINES_PER_PAGE;
        session.diff_change = None;
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
        if let Some(Selection::File { path, place }) = &session.selection {
            return vec![(path.clone(), *place)];
        }
        // Fall back to the whole working tree so ✦ works without a focused file.
        let Some(status) = session.state.current_status.as_ref() else {
            return Vec::new();
        };
        let mut files = Vec::new();
        for path in &status.staged_files {
            files.push((path.clone(), FileDisposition::Staged));
        }
        for path in &status.unstaged_files {
            files.push((path.clone(), FileDisposition::Unstaged));
        }
        for path in &status.untracked_files {
            files.push((path.clone(), FileDisposition::Untracked));
        }
        files
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
                let modifiers = self.modifiers;
                let as_tree = self.files_as_tree;
                if let Some(session) = self.active_session_mut() {
                    let file = MarkedFile { path: path.clone(), place };
                    session.marked_files = select_files(session, file, modifiers, as_tree);
                }
                self.update(Message::Select(Selection::File { path, place }))
            }
            Message::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers;
                Task::none()
            }
            Message::ChangeContextMenu { path, place } => {
                let Some(session) = self.active_session_mut() else {
                    return Task::none();
                };
                let file = MarkedFile { path: path.clone(), place };
                // Right-clicking outside the selection selects that row first (JetBrains).
                if !session.marked_files.contains(&file) {
                    session.marked_files = vec![file.clone()];
                    session.selection_anchor = Some(file);
                }
                let target = if session.marked_files.len() > 1 {
                    crate::context_menu::ContextTarget::Changes { files: session.marked_files.clone() }
                } else {
                    crate::context_menu::ContextTarget::ChangePath { path, place, is_dir: false }
                };
                self.update(Message::OpenContextMenu { target })
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
                    session.diff_change = None;
                }
                Task::none()
            }
            Message::ClearDiffFocus => {
                if let Some(session) = self.active_session_mut() {
                    session.focused_diff_path = None;
                    session.diff_change = None;
                }
                Task::none()
            }
            Message::ToggleDiffSplit => {
                self.diff_split = !self.diff_split;
                if let Some(session) = self.active_session_mut() {
                    session.diff_change = None;
                }
                Task::none()
            }
            Message::StepDiffChange(forward) => self.step_diff_change(forward),
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
            Message::SetHunkMode(mode) => {
                self.apply_hunk_mode(mode);
                Task::none()
            }
            Message::CycleHunkMode => {
                let next = if self.hunk_mode == HunkMode::Focus {
                    HunkMode::Beside
                } else {
                    self.hunk_mode.cycle()
                };
                self.apply_hunk_mode(next);
                Task::none()
            }
            Message::Escape => self.handle_escape(),
            Message::HistoryQueryChanged(query) => {
                self.history_query = query;
                Task::none()
            }
            Message::FocusHistorySearch => {
                if self.sessions.is_empty() {
                    return Task::none();
                }
                self.palette = None;
                Task::batch([
                    iced::widget::text_input::focus(HISTORY_SEARCH_ID),
                    iced::widget::text_input::select_all(HISTORY_SEARCH_ID),
                ])
            }
            Message::TogglePalette(prefill) => {
                if self.sessions.is_empty() || self.palette.take().is_some() {
                    return Task::none();
                }
                self.context_menu = None;
                self.palette = Some(PaletteState {
                    query: prefill,
                    selected: 0,
                });
                Task::batch([
                    iced::widget::text_input::focus(PALETTE_INPUT_ID),
                    iced::widget::text_input::move_cursor_to_end(PALETTE_INPUT_ID),
                ])
            }
            Message::PaletteQueryChanged(query) => {
                if let Some(palette) = &mut self.palette {
                    palette.query = query;
                    palette.selected = 0;
                }
                Task::none()
            }
            Message::PaletteMove(step) => {
                let count = self.palette_results().1.len();
                if let Some(palette) = &mut self.palette {
                    palette.selected = step_selection(palette.selected, step, count);
                    return Task::none();
                }
                // No palette: Up / Down recall commands when the terminal's line has focus.
                if self.terminal_open {
                    return when_shell_focused(move |focused| Message::ShellHistory { step, focused });
                }
                Task::none()
            }
            Message::CtrlCPressed => {
                if self.shell_busy {
                    when_shell_focused(|focused| Message::ShellCtrlC { focused })
                } else {
                    Task::none()
                }
            }
            Message::PaletteSubmit => {
                let selected = self.palette.as_ref().map_or(0, |palette| palette.selected);
                self.update(Message::PaletteActivate(selected))
            }
            Message::PaletteActivate(position) => {
                let (items, ranked) = self.palette_results();
                let Some(item) = ranked.get(position).and_then(|&index| items.get(index)) else {
                    return Task::none();
                };
                let next = palette_message(item.target.clone());
                self.palette = None;
                self.update(next)
            }
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
                    let grid = self.grid_width();
                    if slot == RatioSlot::Files && self.files_split_is_a_width() && grid > 0.0 {
                        self.files_px = clamped * grid;
                    }
                } else {
                    self.panes.resize(event.split, event.ratio);
                }
                Task::none()
            }
            Message::WindowResized(width) => {
                self.window_width = width;
                self.keep_files_width();
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
            Message::ToggleExplorerSection(section) => {
                self.session_mut().toggle_explorer(section);
                Task::none()
            }
            Message::RefFilterChanged(filter) => {
                self.ref_filter = filter;
                Task::none()
            }
            Message::ToggleRefFolder(key) => {
                self.session_mut().toggle_ref_folder(&key);
                Task::none()
            }
            Message::RefClicked { name, remote } => self.ref_clicked(name, remote),
            Message::ClearBranchFilter => {
                if let Some(session) = self.active_session_mut() {
                    session.branch_filter = None;
                    session.focus_current_branch = false;
                }
                Task::none()
            }
            Message::BranchChoicePicked(choice) => {
                use crate::log_filter::LogChoice;
                if let Some(session) = self.active_session_mut() {
                    session.focus_current_branch = choice == LogChoice::Head;
                    session.branch_filter = match choice {
                        LogChoice::Named(name) => Some(name),
                        LogChoice::All | LogChoice::Head => None,
                    };
                }
                Task::none()
            }
            Message::AuthorChoicePicked(choice) => {
                use crate::log_filter::LogChoice;
                if let Some(session) = self.active_session_mut() {
                    session.author_filter = match choice {
                        LogChoice::Named(name) => Some(name),
                        LogChoice::All | LogChoice::Head => None,
                    };
                }
                Task::none()
            }
            Message::ToggleBranchList => {
                // The top-bar branch chip opens the JetBrains-style branches popup.
                self.branch_list_open = !self.branch_list_open && !self.sessions.is_empty();
                self.branch_popup_filter.clear();
                self.context_menu = None;
                if self.branch_list_open {
                    iced::widget::text_input::focus(BRANCH_POPUP_ID)
                } else {
                    Task::none()
                }
            }
            Message::BranchPopupFilterChanged(filter) => {
                self.branch_popup_filter = filter;
                Task::none()
            }
            Message::BranchPopupPick { name, remote } => {
                self.branch_list_open = false;
                let Some(session) = self.active_session() else {
                    return Task::none();
                };
                let tip = session
                    .state
                    .branches
                    .iter()
                    .find(|branch| branch.is_remote == remote && branch.name == name)
                    .map(|branch| branch.tip.clone())
                    .unwrap_or_default();
                let target = if remote {
                    ContextTarget::RemoteBranch { name, tip }
                } else {
                    ContextTarget::LocalBranch { name, tip }
                };
                self.update(Message::OpenContextMenu { target })
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
                // The opener already stored this on disk; keep the window's copy in step.
                let mut settings = self.settings.clone();
                settings.remember_recent(&path);
                let list_models = Arc::clone(&self.list_models);
                let suggest_commit = Arc::clone(&self.suggest_commit);
                let terminal = self.take_terminal();
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
                self.restore_terminal(terminal);
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
                let change = self.console.record(result.exchange);
                self.mirror_console(change);
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
                let change = self.console.record(result.exchange);
                self.mirror_console(change);
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
                if self.terminal_open {
                    focus_shell()
                } else {
                    Task::none()
                }
            }
            Message::TerminalEdited(action) => {
                // The log is read-only: select, scroll and copy, never type.
                if !action.is_edit() {
                    self.terminal_log.perform(action);
                }
                Task::none()
            }
            Message::ClearTerminal => {
                self.console.clear();
                self.terminal_log = text_editor::Content::new();
                focus_shell()
            }
            Message::ShellInputChanged(value) => {
                self.shell_input = value;
                Task::none()
            }
            Message::ShellSubmit => self.submit_shell(),
            Message::Shell(run, event) => self.shell_event(run, event),
            Message::ShellInterrupt => {
                if let Some(pid) = self.shell_pid.take() {
                    stop_shell(pid);
                    self.console_line(LineKind::Error, "^C");
                }
                Task::none()
            }
            Message::ShellCtrlC { focused } => {
                if focused && self.shell_busy {
                    self.update(Message::ShellInterrupt)
                } else {
                    Task::none()
                }
            }
            Message::ShellHistory { step, focused } => {
                if focused && !self.shell_busy {
                    if let Some(line) = self.console.step_history(step) {
                        self.shell_input = line;
                        // Put the caret at the end, like a shell recalling a line.
                        return iced::widget::text_input::move_cursor_to_end(
                            iced::widget::text_input::Id::new(SHELL_INPUT_ID),
                        );
                    }
                }
                Task::none()
            }
            Message::FocusShell => focus_shell(),
            Message::TerminalDragStart => {
                self.terminal_drag = Some((self.cursor.1, self.terminal_height));
                Task::none()
            }
            Message::PointerReleased => {
                self.terminal_drag = None;
                Task::none()
            }
            Message::ToggleRepositoriesPinned => {
                self.settings.repositories_pinned = !self.settings.repositories_pinned;
                self.repositories_flyout = false;
                // The grid just got wider or narrower; keep the Explorer's pixel width.
                self.keep_files_width();
                self.persist_settings(self.settings.clone())
            }
            Message::ToggleRepositoriesFlyout => {
                self.repositories_flyout = !self.repositories_flyout;
                if self.repositories_flyout {
                    iced::widget::text_input::focus(iced::widget::text_input::Id::new(
                        REPOSITORY_FILTER_ID,
                    ))
                } else {
                    Task::none()
                }
            }
            Message::RepositoryFilterChanged(value) => {
                self.repository_filter = value;
                Task::none()
            }
            Message::ForgetRepository(path) => {
                if self.settings.forget_recent(&path) {
                    self.persist_settings(self.settings.clone())
                } else {
                    Task::none()
                }
            }
            Message::HeaderMarkChosen(mark) => {
                self.settings.header_mark = mark;
                self.persist_settings(self.settings.clone())
            }
            Message::OpenCompare { path, against } => self.open_compare(path, against),
            Message::ComparePathChanged(value) => {
                if let Some(compare) = &mut self.compare {
                    compare.path = value;
                }
                Task::none()
            }
            Message::ComparePathSubmitted => self.run_compare(),
            Message::ComparePathPicked(path) => {
                // A conflicted file reopens on yours against theirs; anything else keeps the sides.
                let conflicted = self
                    .active_session()
                    .is_some_and(|session| crate::compare::is_conflicted(session.state.current_status.as_ref(), &path));
                let was_conflicted = self.compare.as_ref().is_some_and(|compare| {
                    compare.left == CompareSide::Revision(crate::compare::STAGE_YOURS.to_string())
                });
                if conflicted || was_conflicted {
                    return self.open_compare(Some(path), None);
                }
                if let Some(compare) = &mut self.compare {
                    compare.path = path;
                }
                self.run_compare()
            }
            Message::CompareSidePicked { left, side } => {
                if let Some(compare) = &mut self.compare {
                    if left {
                        compare.left = side;
                    } else {
                        compare.right = side;
                    }
                }
                self.run_compare()
            }
            Message::CompareDraftChanged { left, text } => {
                if let Some(compare) = &mut self.compare {
                    if left {
                        compare.left_draft = text;
                    } else {
                        compare.right_draft = text;
                    }
                }
                Task::none()
            }
            Message::CompareDraftSubmitted { left } => {
                if let Some(compare) = &mut self.compare {
                    let draft = if left { &compare.left_draft } else { &compare.right_draft };
                    let revision = draft.trim().to_string();
                    if !revision.is_empty() {
                        let side = CompareSide::Revision(revision);
                        if left {
                            compare.left = side;
                        } else {
                            compare.right = side;
                        }
                    }
                }
                self.run_compare()
            }
            Message::CompareSwap => {
                if let Some(compare) = &mut self.compare {
                    compare.swap();
                }
                self.run_compare()
            }
            Message::CompareLoaded { generation, result } => {
                if let Some(compare) = &mut self.compare {
                    if compare.generation == generation {
                        compare.loading = false;
                        compare.result = Some(result.map_err(|error| error_message(&error)));
                    }
                }
                Task::none()
            }
            Message::CloseCompare => {
                self.compare = None;
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
                        session.diff_change = None;
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
                // The selection when there is one, else the file shown in the diff.
                if !session.marked_files.is_empty() {
                    self.confirm_discard = Some(session.marked_files.clone());
                } else if let Selection::File { path, place } = session.shown_selection() {
                    self.confirm_discard = Some(vec![MarkedFile { path, place }]);
                }
                Task::none()
            }
            Message::CancelDiscard => {
                self.confirm_discard = None;
                Task::none()
            }
            Message::ConfirmDiscard => {
                let Some(files) = self.confirm_discard.clone() else {
                    return Task::none();
                };
                let Some(session) = self.active_session() else {
                    return Task::none();
                };
                if session.state.loading || files.is_empty() {
                    return Task::none();
                }
                let repo = session.adapter.clone();
                let session_path = session.path.clone();
                self.confirm_discard = None;
                self.session_mut().state.loading = true;
                self.error = None;
                Task::perform(
                    discard_paths_in_background(repo, files),
                    move |result| Message::DiscardFinished(session_path.clone(), result),
                )
            }
            Message::DiscardFinished(path, result) => self.finish_reload(&path, result),
            Message::IgnoreFinished(path, result) => self.finish_reload(&path, result),
            Message::StageHunk { path, index } => self.edit_hunk(path, index, true),
            Message::DiscardHunk { path, index } => self.edit_hunk(path, index, false),
            Message::HunkFinished(path, result) => self.finish_reload(&path, result),
            Message::StashPush => self.update(Message::AskStash(Vec::new())),
            Message::AskStash(paths) => {
                self.stash_prompt = Some(StashPrompt { message: String::new(), include_untracked: false, paths });
                iced::widget::text_input::focus(STASH_MESSAGE_ID)
            }
            Message::StashMessageChanged(message) => {
                if let Some(prompt) = self.stash_prompt.as_mut() {
                    prompt.message = message;
                }
                Task::none()
            }
            Message::StashUntrackedToggled(on) => {
                if let Some(prompt) = self.stash_prompt.as_mut() {
                    prompt.include_untracked = on;
                }
                Task::none()
            }
            Message::CancelStash => {
                self.stash_prompt = None;
                Task::none()
            }
            Message::ConfirmStash => {
                let Some(prompt) = self.stash_prompt.take() else {
                    return Task::none();
                };
                let message = prompt.message.trim().to_string();
                if prompt.paths.is_empty() {
                    self.run_stash(StashAction::Push { message, include_untracked: prompt.include_untracked })
                } else {
                    self.run_stash(StashAction::PushPaths { message, paths: prompt.paths })
                }
            }
            Message::MergeTargetPicked(target) => {
                if let Some(HistoryPrompt::Merge { name, .. }) = self.history_prompt.as_mut() {
                    *name = target;
                }
                Task::none()
            }
            Message::MergeNoFfToggled(on) => {
                if let Some(HistoryPrompt::Merge { no_ff, .. }) = self.history_prompt.as_mut() {
                    *no_ff = on;
                }
                Task::none()
            }
            Message::AbortOperation => match self.current_operation() {
                Some(operation) => self.run_history(HistoryAction::Abort(operation)),
                None => Task::none(),
            },
            Message::ContinueOperation => match self.current_operation() {
                Some(operation) => self.run_history(HistoryAction::Continue(operation)),
                None => Task::none(),
            },
            Message::ResolveConflict { path, side } => {
                self.compare = None;
                self.run_history(HistoryAction::Resolve { path, side })
            }
            Message::MarkResolved(path) => {
                self.compare = None;
                self.update(Message::EditWorktree(WorktreeEdit::Stage(vec![path])))
            }
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
                    let listed = session
                        .state
                        .stashes
                        .iter()
                        .find(|stash| stash.index == index && stash.message == message)
                        .cloned();
                    self.confirm_discard = None;
                    self.confirm_drop = listed;
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
            Message::AskCreateBranchFrom(start) => {
                self.branch_list_open = false;
                if self.active_session().is_none_or(|session| session.state.loading) {
                    return Task::none();
                }
                self.branch_prompt = Some(BranchPrompt::Create {
                    start,
                    name: String::new(),
                    checkout: true,
                });
                Task::none()
            }
            Message::BranchCheckoutToggled(on) => {
                if let Some(BranchPrompt::Create { checkout, .. }) = &mut self.branch_prompt {
                    *checkout = on;
                }
                Task::none()
            }
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
                        let message = error_message(&error);
                        self.error = Some(stopped_on_conflicts(&message).map_or(message, str::to_string));
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
                    start_point: None,
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
            Message::AskCreateWorktreeFrom {
                branch,
                create_branch,
                start_point,
            } => {
                if self.worktree_busy || self.active_session().is_none() {
                    return Task::none();
                }
                self.close_prompts();
                self.error = None;
                self.worktree_prompt = Some(WorktreePrompt::Create {
                    path: String::new(),
                    branch,
                    create_branch,
                    start_point,
                });
                Task::none()
            }
            Message::ToggleFocusCurrentBranch => {
                if let Some(session) = self.active_session_mut() {
                    session.focus_current_branch = !session.focus_current_branch;
                    session.branch_filter = None;
                }
                Task::none()
            }
            Message::OpenContextMenu { target } => {
                self.context_menu = Some(crate::context_menu::ContextMenuState {
                    target,
                    x: self.cursor.0,
                    y: self.cursor.1,
                });
                Task::none()
            }
            Message::CloseContextMenu => {
                self.context_menu = None;
                Task::none()
            }
            Message::CursorMoved { x, y } => {
                self.cursor = (x, y);
                if let Some((start_y, start_height)) = self.terminal_drag {
                    // The edge is on top, so dragging up (smaller y) makes the terminal taller.
                    self.terminal_height = (start_height + (start_y - y))
                        .clamp(TERMINAL_MIN_HEIGHT, TERMINAL_MAX_HEIGHT);
                }
                Task::none()
            }
            Message::ContextMenuAction(kind) => {
                self.context_menu = None;
                match kind {
                    crate::context_menu::ContextActionKind::OpenAsWorktree {
                        branch,
                        create_branch,
                        start_point,
                    } => self.update(Message::AskCreateWorktreeFrom {
                        branch,
                        create_branch,
                        start_point,
                    }),
                    crate::context_menu::ContextActionKind::DetachHead { commit_id } => {
                        self.update(Message::AskDetachHead { commit_id })
                    }
                    crate::context_menu::ContextActionKind::StageChange { path, unstage } => {
                        if unstage {
                            self.update(Message::EditWorktree(WorktreeEdit::Unstage(vec![path])))
                        } else {
                            self.update(Message::EditWorktree(WorktreeEdit::Stage(vec![path])))
                        }
                    }
                    crate::context_menu::ContextActionKind::DiscardChange {
                        path,
                        place,
                        is_dir,
                    } => self.ask_discard_change(path, place, is_dir),
                    crate::context_menu::ContextActionKind::IgnoreChange { path, is_dir } => {
                        self.run_ignore_change(path, is_dir)
                    }
                    crate::context_menu::ContextActionKind::StashChange { path } => {
                        self.update(Message::AskStash(vec![path]))
                    }
                    crate::context_menu::ContextActionKind::OpenInOs { path, is_dir } => {
                        self.open_change_in_os(&path, is_dir);
                        Task::none()
                    }
                    other => self.run_object_action(other),
                }
            }
            Message::AskDetachHead { commit_id } => self.ask_detach_head(commit_id),
            Message::ConfirmDetachHead => self.run_detach_head(),
            Message::CancelDetach => {
                self.confirm_detach = None;
                Task::none()
            }
            Message::IconTipEnter { key, title } => {
                self.tip_hover = Some((key, title, std::time::Instant::now()));
                self.tip_ready_key = None;
                Task::none()
            }
            Message::IconTipLeave { key } => {
                if self
                    .tip_hover
                    .as_ref()
                    .is_some_and(|(hover_key, _, _)| hover_key == &key)
                {
                    self.tip_hover = None;
                    self.tip_ready_key = None;
                }
                Task::none()
            }
            Message::IconTipTick => {
                if let Some((key, _, started)) = &self.tip_hover {
                    if started.elapsed() >= std::time::Duration::from_secs(2) {
                        self.tip_ready_key = Some(key.clone());
                    }
                }
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
        self.context_menu = None;
        self.confirm_detach = None;
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
            start_point,
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
            add_worktree_in_background(repo, path, branch, create_branch, start_point),
            move |result| Message::WorktreeCreated {
                session: session.clone(),
                path: created.clone(),
                result,
            },
        )
    }

    fn ask_detach_head(&mut self, commit_id: String) -> Task<Message> {
        let Some(session) = self.active_session() else {
            return Task::none();
        };
        if session.state.loading || commit_id.trim().is_empty() {
            return Task::none();
        }
        let dirty = session
            .state
            .current_status
            .as_ref()
            .is_some_and(|status| {
                !status.staged_files.is_empty()
                    || !status.unstaged_files.is_empty()
                    || !status.untracked_files.is_empty()
            });
        if dirty {
            self.confirm_detach = Some(commit_id);
            Task::none()
        } else {
            self.confirm_detach = Some(commit_id);
            self.run_detach_head()
        }
    }

    fn run_detach_head(&mut self) -> Task<Message> {
        let Some(commit_id) = self.confirm_detach.take() else {
            return Task::none();
        };
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
        use crate::jobs::checkout_detached_in_background;
        Task::perform(
            checkout_detached_in_background(repo, commit_id),
            move |result| Message::CheckoutFinished(path.clone(), result),
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
                    checkout: true,
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

    /// Lift the terminal out before the window is rebuilt for another repository.
    fn take_terminal(&mut self) -> KeptTerminal {
        KeptTerminal {
            console: std::mem::replace(&mut self.console, crate::terminal::Console::new()),
            log: std::mem::take(&mut self.terminal_log),
            open: self.terminal_open,
            height: self.terminal_height,
            input: std::mem::take(&mut self.shell_input),
            busy: self.shell_busy,
            pid: self.shell_pid,
            run: self.shell_run,
        }
    }

    /// Put the terminal back after a rebuild. The next command runs in the new repository,
    /// like opening a project in an editor; a command still running keeps streaming here.
    fn restore_terminal(&mut self, kept: KeptTerminal) {
        self.console = kept.console;
        self.terminal_log = kept.log;
        self.terminal_open = kept.open;
        self.terminal_height = kept.height;
        self.shell_input = kept.input;
        self.shell_busy = kept.busy;
        self.shell_pid = kept.pid;
        self.shell_run = kept.run;
        self.shell_cwd = None;
    }

    /// Copy a console change into the text editor. Appends go on the end, which also scrolls
    /// to the newest line; a trim redraws the whole log.
    fn mirror_console(&mut self, change: ConsoleChange) {
        use text_editor::{Action, Edit, Motion};
        match change {
            ConsoleChange::Trimmed => {
                self.terminal_log = text_editor::Content::with_text(&self.console.text());
                self.terminal_log.perform(Action::Move(Motion::DocumentEnd));
            }
            ConsoleChange::Appended { text, first } => {
                let text = if first { text } else { format!("\n{text}") };
                self.terminal_log.perform(Action::Move(Motion::DocumentEnd));
                self.terminal_log
                    .perform(Action::Edit(Edit::Paste(Arc::new(text))));
            }
        }
    }

    fn console_line(&mut self, kind: LineKind, text: &str) {
        let change = self.console.push(kind, text);
        self.mirror_console(change);
    }

    /// Where the next command runs: the folder the last `cd` left, else the active tab.
    pub(crate) fn shell_folder(&self) -> Option<String> {
        self.shell_cwd
            .clone()
            .or_else(|| self.active_session().map(|session| session.path.clone()))
    }

    /// The prompt for the folder the next command runs in, with the branch when that folder
    /// is inside the active tab's worktree.
    pub(crate) fn shell_prompt(&self) -> String {
        let folder = self.shell_folder().unwrap_or_default();
        let branch = self.active_session().and_then(|session| {
            let inside = folder_is_inside(&folder, &session.path);
            inside.then(|| crate::view::current_branch(&session.state.branches))
        });
        crate::terminal::prompt(&folder, branch.as_deref())
    }

    fn submit_shell(&mut self) -> Task<Message> {
        if self.shell_busy {
            return Task::none();
        }
        let command = self.shell_input.trim().to_string();
        let prompt = self.shell_prompt();
        self.shell_input.clear();
        // An empty line prints a fresh prompt, as a shell does.
        self.console_line(LineKind::Prompt, &format!("{prompt}{command}"));
        if command.is_empty() {
            return Task::none();
        }
        self.console.remember(&command);
        match crate::terminal::builtin(&command) {
            Some(crate::terminal::Builtin::Clear) => return self.update(Message::ClearTerminal),
            Some(crate::terminal::Builtin::Exit) => {
                self.terminal_open = false;
                return Task::none();
            }
            None => {}
        }
        let Some(cwd) = self.shell_folder() else {
            self.console_line(LineKind::Note, "Open a repository before running commands.");
            return Task::none();
        };
        self.shell_busy = true;
        self.shell_run = self.shell_run.wrapping_add(1);
        let run = self.shell_run;
        Task::run(shell_stream(cwd, command), move |event| Message::Shell(run, event))
    }

    fn shell_event(&mut self, run: u64, event: ShellEvent) -> Task<Message> {
        if run != self.shell_run {
            return Task::none();
        }
        match event {
            ShellEvent::Started(pid) => {
                self.shell_pid = Some(pid);
                Task::none()
            }
            ShellEvent::Line { error, text } => {
                let kind = if error { LineKind::Error } else { LineKind::Output };
                self.console_line(kind, &text);
                Task::none()
            }
            ShellEvent::Finished { cwd, code: _ } => {
                self.shell_busy = false;
                self.shell_pid = None;
                if let Some(cwd) = cwd {
                    self.shell_cwd = Some(cwd);
                }
                // The command may have changed the repository (checkout, commit, stash…).
                // Reload the tab whose worktree it ran in, so the window catches up.
                let folder = self.shell_folder().unwrap_or_default();
                let refresh = self
                    .sessions
                    .iter()
                    .find(|session| folder_is_inside(&folder, &session.path))
                    .map(|session| session.path.clone());
                let focus = focus_shell();
                match refresh {
                    Some(path) => Task::batch([focus, self.update(Message::RefreshSession(path))]),
                    None => focus,
                }
            }
        }
    }

    /// Open the Compare window on `path`, or on the selected file, or on the first change.
    fn open_compare(&mut self, path: Option<String>, against: Option<String>) -> Task<Message> {
        let Some(session) = self.active_session() else {
            return Task::none();
        };
        let status = session.state.current_status.as_ref();
        let selected = match session.selection.as_ref() {
            Some(crate::selection::Selection::File { path, .. }) => Some(path.clone()),
            _ => None,
        };
        let path = path
            .or(selected)
            .or_else(|| crate::compare::changed_paths(status).into_iter().next())
            .unwrap_or_default();
        let conflicted = crate::compare::is_conflicted(status, &path);
        let state = CompareState::open(session.path.clone(), path, conflicted, against);
        self.close_prompts();
        self.context_menu = None;
        self.compare = Some(state);
        self.run_compare()
    }

    /// Ask git for the open compare. A newer request makes an older answer stale.
    fn run_compare(&mut self) -> Task<Message> {
        let Some(compare) = &self.compare else {
            return Task::none();
        };
        let path = compare.path.trim().to_string();
        let query = crate::compare::query(&compare.left, &compare.right);
        let repo = self
            .sessions
            .iter()
            .find(|session| session.path == compare.session_path)
            .map(|session| session.adapter.clone());
        self.compare_generation = self.compare_generation.wrapping_add(1);
        let generation = self.compare_generation;
        let Some(compare) = &mut self.compare else {
            return Task::none();
        };
        compare.generation = generation;
        let (Some(repo), Some((left, right, reversed)), false) = (repo, query, path.is_empty()) else {
            compare.loading = false;
            compare.result = None;
            return Task::none();
        };
        compare.loading = true;
        Task::perform(compare_in_background(repo, path, left, right), move |result| {
            Message::CompareLoaded {
                generation,
                result: result.map(|patch| {
                    if reversed {
                        crate::compare::reverse(patch)
                    } else {
                        patch
                    }
                }),
            }
        })
    }

    /// Label for the status strip while work is in flight.
    pub(crate) fn activity_label(&self) -> &'static str {
        if self.shell_busy {
            "Running command…"
        } else if self.worktree_busy {
            "Worktree…"
        } else if self.agent_busy {
            "Agent…"
        } else if self.opening || self.worktree_opening.is_some() {
            "Opening…"
        } else if self.active_session().is_some_and(|s| s.state.loading) {
            "Loading…"
        } else {
            "Ready"
        }
    }

    pub(crate) fn activity_busy(&self) -> bool {
        self.activity_label() != "Ready"
    }

    fn confirm_branch(&mut self) -> Task<Message> {
        let Some(prompt) = self.branch_prompt.clone() else {
            return Task::none();
        };
        let action = match prompt {
            BranchPrompt::Create {
                start,
                name,
                checkout,
            } => {
                let name = name.trim().to_string();
                if name.is_empty() {
                    return Task::none();
                }
                if !self.start_point_exists(&start) {
                    return self.branch_is_gone();
                }
                return self.run_branch_then(BranchAction::Create { name, start }, checkout);
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

    /// A start point the window still lists: HEAD, a local or remote branch, a tag, a stash
    /// commit, or a loaded commit. Remote starts used to be refused here although the dialog
    /// offered them, so "New branch" from a remote failed with "no longer there".
    fn start_point_exists(&self, start: &str) -> bool {
        let Some(session) = self.active_session() else {
            return false;
        };
        start == "HEAD"
            || session.local_branch(start).is_some()
            || session.remote_branch(start).is_some()
            || session.state.tags.iter().any(|tag| tag.name == start)
            || session.state.stashes.iter().any(|stash| stash.commit_id == start)
            || session.commit_loaded(start)
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
        self.run_branch_then(action, true)
    }

    /// Runs a branch job. A create checks the new branch out afterwards only when `checkout`.
    fn run_branch_then(&mut self, action: BranchAction, checkout: bool) -> Task<Message> {
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
            BranchAction::Create { name, .. } if checkout => Some(name.clone()),
            BranchAction::Create { .. } | BranchAction::Rename { .. } | BranchAction::Delete { .. } => None,
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
        self.branch_list_open = false;
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
                // A local branch other than the current one, or a remote-tracking branch
                // (merging origin/development is the everyday case).
                let picked = match visible {
                    Selection::Branch { name, current } if !current && session.local_branch(&name).is_some() => Some(name),
                    Selection::Remote { name } if session.remote_branch(&name).is_some() => Some(name),
                    _ => None,
                };
                // Merge opens with no branch chosen and lets the user pick one in the dialog.
                let name = match (picked, ask) {
                    (Some(name), _) => name,
                    (None, HistoryAsk::Merge) => String::new(),
                    (None, _) => return Task::none(),
                };
                if matches!(ask, HistoryAsk::Merge) {
                    HistoryPrompt::Merge { name, no_ff: false }
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
        let Some(HistoryPrompt::Merge { name, no_ff }) = self.history_prompt.clone() else {
            return Task::none();
        };
        if name.is_empty() {
            return Task::none();
        }
        self.confirm_branch_history(name, Some(no_ff))
    }

    fn confirm_rebase(&mut self) -> Task<Message> {
        let Some(HistoryPrompt::Rebase { name }) = self.history_prompt.clone() else {
            return Task::none();
        };
        self.confirm_branch_history(name, None)
    }

    /// `merge` is `Some(no_ff)` for a merge, `None` for a rebase.
    fn confirm_branch_history(&mut self, name: String, merge: Option<bool>) -> Task<Message> {
        match self.local_branch_is_head(&name) {
            Some(true) => {
                self.history_prompt = None;
                self.error = Some(if merge.is_some() {
                    "The current branch cannot be merged.".to_string()
                } else {
                    "The current branch cannot be rebased.".to_string()
                });
                Task::none()
            }
            Some(false) => match merge {
                Some(no_ff) => self.run_history(HistoryAction::Merge { name, no_ff }),
                None => self.run_history(HistoryAction::Rebase { name }),
            },
            None if self
                .active_session()
                .is_some_and(|session| session.remote_branch(&name).is_some()) =>
            {
                match merge {
                    Some(no_ff) => self.run_history(HistoryAction::Merge { name, no_ff }),
                    None => self.run_history(HistoryAction::Rebase { name }),
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

    fn ask_discard_change(
        &mut self,
        path: String,
        place: FileDisposition,
        is_dir: bool,
    ) -> Task<Message> {
        let Some(session) = self.active_session() else {
            return Task::none();
        };
        let Some(status) = session.state.current_status.as_ref() else {
            return Task::none();
        };
        let paths = matching_change_paths(status, place, &path, is_dir);
        if paths.is_empty() {
            return Task::none();
        }
        self.confirm_drop = None;
        self.confirm_discard = Some(paths.into_iter().map(|path| MarkedFile { path, place }).collect());
        Task::none()
    }

    /// Append one `.gitignore` line per unversioned file.
    fn run_ignore_paths(&mut self, paths: Vec<String>) -> Task<Message> {
        let Some(session) = self.active_session() else {
            return Task::none();
        };
        if session.state.loading || paths.is_empty() {
            return Task::none();
        }
        let patterns = paths.iter().map(|path| gitignore_pattern(path, false)).collect();
        let repo = session.adapter.clone();
        let session_path = session.path.clone();
        self.session_mut().state.loading = true;
        self.error = None;
        Task::perform(append_gitignore_in_background(repo, patterns), move |result| {
            Message::IgnoreFinished(session_path.clone(), result)
        })
    }

    fn run_ignore_change(&mut self, path: String, is_dir: bool) -> Task<Message> {
        let Some(session) = self.active_session() else {
            return Task::none();
        };
        if session.state.loading {
            return Task::none();
        }
        let pattern = gitignore_pattern(&path, is_dir);
        let repo = session.adapter.clone();
        let session_path = session.path.clone();
        self.session_mut().state.loading = true;
        self.error = None;
        Task::perform(
            append_gitignore_in_background(repo, vec![pattern]),
            move |result| Message::IgnoreFinished(session_path.clone(), result),
        )
    }

    /// One click selects a branch and filters the log to it (JetBrains). A second click on the
    /// same local branch within [`DOUBLE_CLICK`] checks it out; a click never checks out alone.
    fn ref_clicked(&mut self, name: String, remote: bool) -> Task<Message> {
        let now = std::time::Instant::now();
        let double = self
            .last_ref_click
            .as_ref()
            .is_some_and(|(last, at)| *last == name && now.duration_since(*at) <= DOUBLE_CLICK);
        self.last_ref_click = Some((name.clone(), now));
        let Some(session) = self.active_session_mut() else {
            return Task::none();
        };
        let current = !remote
            && session
                .state
                .branches
                .iter()
                .any(|branch| !branch.is_remote && branch.is_head && branch.name == name);
        session.focus_current_branch = false;
        session.branch_filter = Some(name.clone());
        session.selection = Some(if remote {
            Selection::Remote { name: name.clone() }
        } else {
            Selection::Branch {
                name: name.clone(),
                current,
            }
        });
        if double && !remote && !current {
            self.last_ref_click = None;
            return self.update(Message::CheckoutBranch(name));
        }
        Task::none()
    }

    /// Context-menu actions on refs, stashes and worktrees. Each maps onto an existing message;
    /// actions that ask about "the selection" select their target first, so the dialog is
    /// about the row that was right-clicked.
    fn run_object_action(&mut self, kind: crate::context_menu::ContextActionKind) -> Task<Message> {
        use crate::context_menu::ContextActionKind as Kind;
        let select = |this: &mut Self, selection: Selection| {
            if let Some(session) = this.active_session_mut() {
                session.selection = Some(selection);
            }
        };
        match kind {
            Kind::StageFiles(paths) => self.update(Message::EditWorktree(WorktreeEdit::Stage(paths))),
            Kind::UnstageFiles(paths) => self.update(Message::EditWorktree(WorktreeEdit::Unstage(paths))),
            Kind::DiscardFiles(files) => {
                self.confirm_drop = None;
                self.confirm_discard = Some(files);
                Task::none()
            }
            Kind::StashFiles(paths) => self.update(Message::AskStash(paths)),
            Kind::IgnoreFiles(paths) => self.run_ignore_paths(paths),
            Kind::Untrack { paths, ignore } => {
                self.update(Message::EditWorktree(WorktreeEdit::Untrack { paths, ignore }))
            }
            Kind::ResolveConflict { path, side } => self.update(Message::ResolveConflict { path, side }),
            Kind::MarkResolved(path) => self.update(Message::MarkResolved(path)),
            Kind::CheckoutBranch { name } => self.update(Message::CheckoutBranch(name)),
            Kind::CheckoutRemote { remote, local } => {
                if self.has_local_branch(&local) {
                    self.update(Message::CheckoutBranch(local))
                } else {
                    // `git branch <local> <remote>` sets the upstream (branch.autoSetupMerge).
                    self.run_branch_then(BranchAction::Create { name: local, start: remote }, true)
                }
            }
            Kind::NewBranchFrom { start } => self.update(Message::AskCreateBranchFrom(start)),
            Kind::MergeIntoCurrent { name } => {
                select(self, branch_or_remote(&name, self.has_local_branch(&name)));
                self.update(Message::AskMerge)
            }
            Kind::RebaseCurrentOnto { name } => {
                select(self, branch_or_remote(&name, self.has_local_branch(&name)));
                self.update(Message::AskRebase)
            }
            Kind::RenameBranch { name } => {
                let current = self.local_branch_is_head(&name).unwrap_or(false);
                select(self, Selection::Branch { name, current });
                self.update(Message::AskRenameBranch)
            }
            Kind::DeleteBranch { name } => {
                select(self, Selection::Branch { name, current: false });
                self.update(Message::AskDeleteBranch)
            }
            Kind::CherryPick { id } => {
                select(self, Selection::Commit { id });
                self.update(Message::AskCherryPick)
            }
            Kind::ResetTo { id } => {
                select(self, Selection::Commit { id });
                self.update(Message::AskReset)
            }
            Kind::CopyText { text } => iced::clipboard::write(text),
            Kind::OpenWorktreeTab { path } => self.update(Message::OpenWorktree(path)),
            Kind::NewWorktree => self.update(Message::AskCreateWorktree),
            Kind::RemoveWorktree { path } => self.update(Message::AskRemoveWorktree { path }),
            Kind::OpenFolder { path } => {
                open_in_file_manager(std::path::Path::new(&path));
                Task::none()
            }
            Kind::StashApply { index, message } => {
                select(self, Selection::Stash { index, message });
                self.update(Message::StashApply(index))
            }
            Kind::StashPop { index, message } => {
                select(self, Selection::Stash { index, message });
                self.update(Message::StashPop(index))
            }
            Kind::StashDrop { index, message } => {
                select(self, Selection::Stash { index, message });
                self.update(Message::AskDropStash)
            }
            // Handled by the caller.
            Kind::OpenAsWorktree { .. }
            | Kind::DetachHead { .. }
            | Kind::StageChange { .. }
            | Kind::DiscardChange { .. }
            | Kind::IgnoreChange { .. }
            | Kind::StashChange { .. }
            | Kind::OpenInOs { .. } => Task::none(),
        }
    }

    fn open_change_in_os(&self, rel_path: &str, is_dir: bool) {
        let Some(session) = self.active_session() else {
            return;
        };
        let root = std::path::Path::new(&session.path);
        let target = root.join(rel_path.trim_end_matches(['/', '\\']));
        let open_path = if is_dir {
            target
        } else {
            target
                .parent()
                .unwrap_or(root)
                .to_path_buf()
        };
        open_in_file_manager(&open_path);
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
        // Hunk edits act on the working tree and index. A commit's or stash's hunks must never
        // reach them: reverse-applying HEAD's own hunk would silently undo that commit on disk.
        let place = match session.selection.as_ref() {
            Some(Selection::File { place, .. }) => *place,
            _ => return Task::none(),
        };
        if stage && place == FileDisposition::Staged {
            return Task::none();
        }
        let repo = session.adapter.clone();
        let session_path = session.path.clone();
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
        let keys = iced::keyboard::on_key_press(shortcut_message);
        // A focused text input captures every key but Tab and the vertical arrows, and
        // `on_key_press` only sees ignored keys. These few must work while typing.
        let typing_keys = iced::event::listen_with(|event, status, _id| match (event, status) {
            (
                iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, modifiers, .. }),
                iced::event::Status::Captured,
            ) => typing_shortcut(&key, modifiers),
            _ => None,
        });
        let cursor = iced::event::listen_with(|event, _status, _id| match event {
            iced::Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
                Some(Message::CursorMoved {
                    x: position.x,
                    y: position.y,
                })
            }
            iced::Event::Window(iced::window::Event::Resized(size)) => Some(Message::WindowResized(size.width)),
            iced::Event::Mouse(iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left)) => {
                Some(Message::PointerReleased)
            }
            iced::Event::Keyboard(iced::keyboard::Event::ModifiersChanged(modifiers)) => {
                Some(Message::ModifiersChanged(modifiers))
            }
            _ => None,
        });
        let waiting_tip = self.tip_hover.is_some()
            && self.tip_ready_key.as_ref()
                != self.tip_hover.as_ref().map(|(key, _, _)| key);
        if waiting_tip {
            iced::Subscription::batch([
                keys,
                typing_keys,
                cursor,
                iced::time::every(iced::time::Duration::from_millis(200))
                    .map(|_| Message::IconTipTick),
            ])
        } else {
            iced::Subscription::batch([keys, typing_keys, cursor])
        }
    }

    /// True when the hover tip for `key` should be drawn (two seconds elapsed).
    pub(crate) fn tip_ready(&self, key: &str) -> bool {
        self.tip_ready_key.as_deref() == Some(key)
    }

    fn handle_escape(&mut self) -> Task<Message> {
        if self.palette.take().is_some() {
            return Task::none();
        }
        if self.compare.take().is_some() {
            return Task::none();
        }
        if self.repositories_flyout {
            self.repositories_flyout = false;
            return Task::none();
        }
        if self.confirm_detach.is_some() {
            return self.update(Message::CancelDetach);
        }
        if self.context_menu.is_some() {
            return self.update(Message::CloseContextMenu);
        }
        if self.restore_focus_mode() {
            return Task::none();
        }
        if self.tip_hover.is_some() || self.tip_ready_key.is_some() {
            self.tip_hover = None;
            self.tip_ready_key = None;
            return Task::none();
        }
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
        if self.stash_prompt.is_some() {
            return self.update(Message::CancelStash);
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
        Key::Named(Named::ArrowDown) => Some(Message::PaletteMove(1)),
        Key::Named(Named::ArrowUp) => Some(Message::PaletteMove(-1)),
        Key::Character(_) if typing_shortcut(&key, modifiers).is_some() => {
            typing_shortcut(&key, modifiers)
        }
        Key::Character(c)
            if ctrl && modifiers.shift() && c.as_str().eq_ignore_ascii_case("l") =>
        {
            Some(Message::CycleLayout)
        }
        Key::Character(c)
            if ctrl && modifiers.shift() && c.as_str().eq_ignore_ascii_case("h") =>
        {
            Some(Message::CycleHunkMode)
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

/// Two clicks on the same branch within this window are a double-click.
const DOUBLE_CLICK: std::time::Duration = std::time::Duration::from_millis(400);

/// Put the caret on the terminal's command line.
fn focus_shell() -> Task<Message> {
    iced::widget::text_input::focus(iced::widget::text_input::Id::new(SHELL_INPUT_ID))
}

/// Ask which widget has focus, then `to_message(true)` when it is the command line.
/// Nothing focused sends nothing.
fn when_shell_focused(to_message: impl Fn(bool) -> Message + Send + 'static) -> Task<Message> {
    let shell = iced::advanced::widget::Id::from(iced::widget::text_input::Id::new(SHELL_INPUT_ID));
    iced::advanced::widget::operate(iced::advanced::widget::operation::focusable::find_focused())
        .map(move |focused| to_message(focused == shell))
}

/// True when `folder` is `root` or below it. Separators and (on Windows) case are ignored.
fn folder_is_inside(folder: &str, root: &str) -> bool {
    fn key(path: &str) -> String {
        let unified = path.trim().replace('\\', "/");
        let trimmed = unified.trim_end_matches('/').to_string();
        if cfg!(windows) {
            trimmed.to_lowercase()
        } else {
            trimmed
        }
    }
    let (folder, root) = (key(folder), key(root));
    !root.is_empty() && (folder == root || folder.starts_with(&format!("{root}/")))
}

/// Reveal `path` in the platform file manager. Fire and forget: a missing tool is not an error.
fn open_in_file_manager(path: &std::path::Path) {
        #[cfg(windows)]
        {
            let _ = std::process::Command::new("explorer")
                .arg(path.as_os_str())
                .spawn();
        }
        #[cfg(target_os = "macos")]
        {
            let _ = std::process::Command::new("open")
                .arg(path.as_os_str())
                .spawn();
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            let _ = std::process::Command::new("xdg-open")
                .arg(path.as_os_str())
                .spawn();
        }
}

/// The selection a merge/rebase dialog reads for `name`: a local branch, else a remote one.
/// Git's output when a merge, rebase, cherry-pick or revert stopped on conflicts starts with
/// whatever it was doing ("Auto-merging Config.cs"), which reads as the error. Say what happened.
const STOPPED_ON_CONFLICTS: &str =
    "Stopped on conflicts. Resolve the files under Merge Conflicts, then use the bar at the top to continue or abort.";

fn stopped_on_conflicts(git_output: &str) -> Option<&'static str> {
    let markers = ["CONFLICT (", "Automatic merge failed", "could not apply", "after resolving the conflicts"];
    markers.iter().any(|marker| git_output.contains(marker)).then_some(STOPPED_ON_CONFLICTS)
}

/// The working-tree selection after a click on `file`: alone, toggled with Ctrl (Cmd), or the
/// range from the last plain click with Shift.
fn select_files(
    session: &mut WorktreeSession,
    file: MarkedFile,
    modifiers: iced::keyboard::Modifiers,
    as_tree: bool,
) -> Vec<MarkedFile> {
    let mut marks = session.marked_files.clone();
    if modifiers.shift() {
        if let (Some(anchor), Some(status)) = (session.selection_anchor.clone(), session.state.current_status.as_ref()) {
            let order = crate::changes::visible_files(status, as_tree, &session.collapsed_sections, &session.collapsed_dirs);
            return crate::changes::selection_range(&order, &anchor, &file);
        }
    }
    session.selection_anchor = Some(file.clone());
    if modifiers.command() {
        match marks.iter().position(|mark| *mark == file) {
            Some(at) => {
                marks.remove(at);
            }
            None => marks.push(file),
        }
        marks
    } else {
        vec![file]
    }
}

fn branch_or_remote(name: &str, is_local: bool) -> Selection {
    if is_local {
        Selection::Branch {
            name: name.to_string(),
            current: false,
        }
    } else {
        Selection::Remote {
            name: name.to_string(),
        }
    }
}

/// Shortcuts honoured even while a text input has focus: Escape, Ctrl+P, Ctrl+K.
fn typing_shortcut(
    key: &iced::keyboard::Key,
    modifiers: iced::keyboard::Modifiers,
) -> Option<Message> {
    use iced::keyboard::key::Named;
    use iced::keyboard::Key;

    let ctrl = modifiers.control() || modifiers.logo();
    match key {
        Key::Named(Named::Escape) => Some(Message::Escape),
        Key::Character(c) if ctrl && !modifiers.shift() && c.as_str().eq_ignore_ascii_case("p") => {
            Some(Message::TogglePalette(String::new()))
        }
        Key::Character(c) if ctrl && !modifiers.shift() && c.as_str().eq_ignore_ascii_case("k") => {
            Some(Message::FocusHistorySearch)
        }
        Key::Character(c) if ctrl && !modifiers.shift() && c.as_str().eq_ignore_ascii_case("c") => {
            Some(Message::CtrlCPressed)
        }
        Key::Character(c) if ctrl && c.as_str() == "`" => Some(Message::ToggleTerminal),
        _ => None,
    }
}

/// Wrap-around move of the highlighted palette row. An empty list stays at 0.
fn step_selection(selected: usize, step: i32, count: usize) -> usize {
    if count == 0 {
        return 0;
    }
    let count = count as i64;
    (selected as i64 + step as i64).rem_euclid(count) as usize
}

/// The existing message each palette row stands for. Nothing here checks out or edits:
/// branches, tags, commits, and stashes are selected; worktrees and repositories open.
fn palette_message(target: crate::palette::PaletteTarget) -> Message {
    use crate::palette::PaletteTarget;
    match target {
        PaletteTarget::Branch { name, current } => {
            Message::Select(Selection::Branch { name, current })
        }
        PaletteTarget::Remote { name } => Message::Select(Selection::Remote { name }),
        PaletteTarget::Tag { name, commit_id } => {
            Message::Select(Selection::Tag { name, commit_id })
        }
        PaletteTarget::Stash { index, message } => {
            Message::Select(Selection::Stash { index, message })
        }
        PaletteTarget::Commit { id } => Message::Select(Selection::Commit { id }),
        PaletteTarget::File { path, place } => Message::FocusFile { path, place },
        PaletteTarget::Worktree { path } => Message::OpenWorktree(path),
        PaletteTarget::Repository { path } => Message::OpenRepository(path),
    }
}

impl GitApp {
    /// Palette index for the active session and its ranking for the current query.
    pub(crate) fn palette_results(&self) -> (Vec<crate::palette::PaletteItem>, Vec<usize>) {
        let (Some(palette), Some(session)) = (self.palette.as_ref(), self.active_session()) else {
            return (Vec::new(), Vec::new());
        };
        let items = crate::palette::palette_items(&crate::palette::PaletteSource {
            branches: &session.state.branches,
            tags: &session.state.tags,
            stashes: &session.state.stashes,
            worktrees: &self.worktree_catalogue,
            commits: &session.state.commits,
            status: session.state.current_status.as_ref(),
            recent_repositories: &self.settings.recent_repositories,
            current_path: &session.path,
        });
        let ranked = crate::palette::rank(&items, &palette.query);
        (items, ranked)
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
        ahead: 0,
        behind: 0,
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

    /// Same panes regardless of tree order (Log puts the Inspector before the Diff).
    fn same_panes(found: &[WorkspacePane], wanted: &[WorkspacePane]) -> bool {
        found.len() == wanted.len() && wanted.iter().all(|pane| found.contains(pane))
    }

    fn split_between(
        state: &pane_grid::State<WorkspacePane>,
        left: &[WorkspacePane],
        right: &[WorkspacePane],
    ) -> (pane_grid::Axis, f32, pane_grid::Split) {
        let found: Vec<_> = splits_of(state)
            .into_iter()
            .filter(|split| same_panes(&split.left, left) && same_panes(&split.right, right))
            .collect();
        assert_eq!(found.len(), 1, "expected one split {left:?} | {right:?}");
        (found[0].axis, found[0].ratio, found[0].id)
    }

    fn assert_open_preset(app: &GitApp, preset: LayoutPreset) {
        let ratios = app.ratios[preset.index()];
        let outer = match preset {
            LayoutPreset::Log | LayoutPreset::Columns | LayoutPreset::InspectorOverDiff => pane_grid::Axis::Vertical,
            LayoutPreset::Stacked => pane_grid::Axis::Horizontal,
        };
        let middle = match preset {
            LayoutPreset::Columns => pane_grid::Axis::Vertical,
            LayoutPreset::Log | LayoutPreset::InspectorOverDiff | LayoutPreset::Stacked => pane_grid::Axis::Horizontal,
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
            LayoutPreset::Log => {
                let (graph_axis, graph_ratio, _) =
                    split_between(&app.panes, &[WorkspacePane::Graph], &[WorkspacePane::Inspector]);
                let (middle_axis, middle_ratio, _) = split_between(
                    &app.panes,
                    &[WorkspacePane::Graph, WorkspacePane::Inspector],
                    &[WorkspacePane::Diff],
                );
                (graph_axis, graph_ratio, middle_axis, middle_ratio)
            }
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
    fn the_jetbrains_log_is_the_default_layout() {
        let app = GitApp::with_repo(Arc::new(FakeGitRepository::ok()));
        assert_eq!(app.layout, LayoutPreset::Log);
        assert!(!app.terminal_open, "the terminal starts collapsed");
        assert!(app.files_rail_open);
        assert_eq!(layout::GRAPH_RATIO, 0.68);
        assert_eq!(layout::COLUMNS_MIDDLE_RATIO, 0.50);
        assert_eq!(layout::STACKED_MIDDLE_RATIO, 0.62);
        assert_eq!(layout::FILES_RATIO, 0.22);
        assert_eq!(layout::MIN_GRAPH_RATIO, 0.50);
        assert_eq!(layout::LOG_MIDDLE_RATIO, 0.64);
        assert_open_preset(&app, LayoutPreset::Log);
    }

    #[test]
    fn the_log_middle_split_resizes_the_log_ratio_only() {
        let mut app = GitApp::with_repo(Arc::new(FakeGitRepository::ok()));
        let middle = mapped_slot(&app, RatioSlot::Middle);
        let _ = app.update(Message::PaneResized(pane_grid::ResizeEvent { split: middle, ratio: 0.55 }));
        assert_eq!(app.ratios[LayoutPreset::Log.index()].middle, 0.55);
        assert_eq!(
            app.ratios[LayoutPreset::InspectorOverDiff.index()].middle,
            layout::STACKED_MIDDLE_RATIO
        );
    }

    #[test]
    fn inspector_over_diff_is_still_available() {
        let mut app = GitApp::with_repo(Arc::new(FakeGitRepository::ok()));
        let _ = app.update(Message::SetLayout(LayoutPreset::InspectorOverDiff));
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
    fn cycle_layout_walks_the_four_presets() {
        let mut app = GitApp::with_repo(Arc::new(FakeGitRepository::ok()));
        assert_eq!(app.layout, LayoutPreset::Log);
        for expected in [
            LayoutPreset::Columns,
            LayoutPreset::InspectorOverDiff,
            LayoutPreset::Stacked,
            LayoutPreset::Log,
        ] {
            let _ = app.update(Message::CycleLayout);
            assert_eq!(app.layout, expected);
        }
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
        // Written against the InspectorOverDiff topology; Log has its own coverage.
        let _ = app.update(Message::SetLayout(LayoutPreset::InspectorOverDiff));
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
        // Written against the InspectorOverDiff topology; Log has its own coverage.
        let _ = app.update(Message::SetLayout(LayoutPreset::InspectorOverDiff));
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
    fn enter_focus_remembers_rails_and_escape_restores() {
        let mut app = GitApp::with_repo(Arc::new(FakeGitRepository::ok()));
        assert!(app.files_rail_open);
        assert!(app.inspector_open);
        assert_eq!(app.hunk_mode, HunkMode::Beside);

        let _ = app.update(Message::SetHunkMode(HunkMode::Focus));
        assert_eq!(app.hunk_mode, HunkMode::Focus);
        assert!(!app.files_rail_open);
        assert!(!app.inspector_open);
        assert!(splits_of(&app.panes)
            .iter()
            .all(|split| !split.left.contains(&WorkspacePane::Inspector)
                && !split.right.contains(&WorkspacePane::Inspector)
                && !split.left.contains(&WorkspacePane::Files)
                && !split.right.contains(&WorkspacePane::Files)));

        let _ = app.update(Message::Escape);
        assert_eq!(app.hunk_mode, HunkMode::Beside);
        assert!(app.files_rail_open);
        assert!(app.inspector_open);
    }

    #[test]
    fn cycle_hunk_mode_skips_focus() {
        let mut app = GitApp::with_repo(Arc::new(FakeGitRepository::ok()));
        assert_eq!(app.hunk_mode, HunkMode::Beside);
        let _ = app.update(Message::CycleHunkMode);
        assert_eq!(app.hunk_mode, HunkMode::Wide);
        let _ = app.update(Message::CycleHunkMode);
        assert_eq!(app.hunk_mode, HunkMode::Beside);
        let _ = app.update(Message::SetHunkMode(HunkMode::Focus));
        assert_eq!(app.hunk_mode, HunkMode::Focus);
        let _ = app.update(Message::CycleHunkMode);
        assert_eq!(app.hunk_mode, HunkMode::Beside);
        assert!(app.files_rail_open);
    }

    #[test]
    fn wide_mode_expands_the_diff_split() {
        let mut app = GitApp::with_repo(Arc::new(FakeGitRepository::ok()));
        // Written against the InspectorOverDiff topology; Log has its own coverage.
        let _ = app.update(Message::SetLayout(LayoutPreset::InspectorOverDiff));
        let (_, beside_middle, _) = split_between(
            &app.panes,
            &[WorkspacePane::Graph],
            &[WorkspacePane::Diff],
        );
        let _ = app.update(Message::SetHunkMode(HunkMode::Wide));
        assert_eq!(app.hunk_mode, HunkMode::Wide);
        assert!(app.files_rail_open);
        assert!(app.inspector_open);
        let (_, wide_middle, _) = split_between(
            &app.panes,
            &[WorkspacePane::Graph],
            &[WorkspacePane::Diff],
        );
        assert!(wide_middle < beside_middle);
    }

    #[test]
    fn toggling_the_files_rail_hides_and_restores_it() {
        let mut app = GitApp::with_repo(Arc::new(FakeGitRepository::ok()));
        // Written against the InspectorOverDiff topology; Log has its own coverage.
        let _ = app.update(Message::SetLayout(LayoutPreset::InspectorOverDiff));
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
    fn creating_a_branch_then_switches_to_it() {
        let mut app = on_branch("main", true);
        app.branch_prompt = Some(BranchPrompt::Create {
            start: "main".to_string(),
            name: "topic".to_string(),
            checkout: true,
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
            ahead: 0,
            behind: 0,
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
                checkout: true,
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
            head_commit_id: None,
            kinds: Default::default(),
            operation: None,
            conflicted_files: vec![],
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
        assert!(app.stash_prompt.is_some(), "asks for a message first");
        let _ = app.update(Message::ConfirmStash);
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
            commit_id: "5ta5h0".to_string(),
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
    fn hunk_edits_refuse_anything_but_a_working_tree_file() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.session_mut().patches = vec![core_domain::Patch {
            path: "a.txt".to_string(),
            hunks: vec![core_domain::Hunk {
                header: "@@ -1 +1 @@".to_string(),
                lines: Vec::new(),
            }],
            binary: false,
        }];
        app.session_mut().selection = Some(Selection::Commit {
            id: "aaaaaaa1".to_string(),
        });
        let _ = app.update(Message::DiscardHunk {
            path: "a.txt".to_string(),
            index: 0,
        });
        assert!(!app.session().state.loading, "a commit's hunk reached the working tree");
        let _ = app.update(Message::StageHunk {
            path: "a.txt".to_string(),
            index: 0,
        });
        assert!(!app.session().state.loading, "a commit's hunk reached the index");

        app.session_mut().selection = Some(Selection::File {
            path: "a.txt".to_string(),
            place: FileDisposition::Unstaged,
        });
        let _ = app.update(Message::DiscardHunk {
            path: "a.txt".to_string(),
            index: 0,
        });
        assert!(app.session().state.loading, "a working-tree hunk is still discardable");
    }

    #[test]
    fn selecting_a_stash_loads_its_commit_patch() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.session_mut().state.stashes = vec![Stash {
            index: 0,
            message: "On dev: wip".to_string(),
            commit_id: "abc123".to_string(),
        }];
        app.session_mut().selection = Some(Selection::Stash {
            index: 0,
            message: "On dev: wip".to_string(),
        });
        assert!(matches!(
            app.session().patch_query(),
            Some(crate::jobs::PatchQuery::Commit(id)) if id == "abc123"
        ));
    }

    #[test]
    fn drop_refuses_a_stash_whose_slot_moved() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.confirm_drop = Some(Stash {
            index: 0,
            message: "WIP on dev".to_string(),
            commit_id: "5ta5h0".to_string(),
        });
        app.session_mut().state.stashes = vec![Stash {
            index: 0,
            message: "On main: other".to_string(),
            commit_id: "5ta5h0".to_string(),
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
                checkout: true,
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
            checkout: true,
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
        // Rebase needs another branch selected. Merge opens and lets the user pick one.
        let mut current = on_branch("main", true);
        let _ = current.update(Message::AskRebase);
        assert!(current.history_prompt.is_none());
        let _ = current.update(Message::AskMerge);
        assert_eq!(current.history_prompt, Some(HistoryPrompt::Merge { name: String::new(), no_ff: false }));

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
                no_ff: false,
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

    #[test]
    fn focus_current_toggles_on_the_active_session() {
        let mut app = two_session_app();
        assert!(!app.session().focus_current_branch);

        let _ = app.update(Message::ToggleFocusCurrentBranch);
        assert!(app.session().focus_current_branch);

        let _ = app.update(Message::SelectWorktreeTab(1));
        assert!(!app.session().focus_current_branch);
        assert!(app.sessions[0].focus_current_branch);
    }

    #[test]
    fn create_from_context_prefills_start_point() {
        let mut app = catalogued_app();
        let _ = app.update(Message::AskCreateWorktreeFrom {
            branch: "wip/abcdef0".to_string(),
            create_branch: true,
            start_point: Some("abcdef012345".to_string()),
        });

        assert!(matches!(
            &app.worktree_prompt,
            Some(WorktreePrompt::Create {
                branch,
                create_branch: true,
                start_point: Some(start),
                ..
            }) if branch == "wip/abcdef0" && start == "abcdef012345"
        ));
    }

    #[test]
    fn dirty_detach_asks_for_confirm() {
        let mut app = GitApp::with_repo(Arc::new(FakeGitRepository::ok()));
        app.session_mut().state.current_status = Some(core_domain::RepoStatus {
            staged_files: vec![],
            unstaged_files: vec!["a.rs".to_string()],
            untracked_files: vec![],
            tracking: None,
            head_commit_id: Some("abc".into()),
            kinds: Default::default(),
            operation: None,
            conflicted_files: vec![],
        });

        let _ = app.update(Message::AskDetachHead {
            commit_id: "deadbeef".to_string(),
        });

        assert_eq!(app.confirm_detach.as_deref(), Some("deadbeef"));
        assert!(!app.session().state.loading);
    }

    #[test]
    fn clean_detach_starts_the_job() {
        let mut app = GitApp::with_repo(Arc::new(FakeGitRepository::ok()));
        app.session_mut().state.current_status = Some(core_domain::RepoStatus {
            staged_files: vec![],
            unstaged_files: vec![],
            untracked_files: vec![],
            tracking: None,
            head_commit_id: Some("abc".into()),
            kinds: Default::default(),
            operation: None,
            conflicted_files: vec![],
        });

        let _ = app.update(Message::AskDetachHead {
            commit_id: "deadbeef".to_string(),
        });

        assert!(app.confirm_detach.is_none());
        assert!(app.session().state.loading);
    }

    #[test]
    fn ctrl_p_and_ctrl_k_work_while_typing() {
        use iced::keyboard::{Key, Modifiers};
        assert!(matches!(
            typing_shortcut(&Key::Character("p".into()), Modifiers::CTRL),
            Some(Message::TogglePalette(_))
        ));
        assert!(matches!(
            typing_shortcut(&Key::Character("K".into()), Modifiers::CTRL),
            Some(Message::FocusHistorySearch)
        ));
        assert!(typing_shortcut(&Key::Character("p".into()), Modifiers::empty()).is_none());
    }

    #[test]
    fn the_palette_toggles_and_escape_closes_it_first() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.error = Some("kept".to_string());
        let _ = app.update(Message::TogglePalette("b: ".to_string()));
        assert_eq!(app.palette.as_ref().map(|p| p.query.as_str()), Some("b: "));
        let _ = app.update(Message::Escape);
        assert!(app.palette.is_none());
        assert_eq!(app.error.as_deref(), Some("kept"), "Escape only closed the palette");
        let _ = app.update(Message::TogglePalette(String::new()));
        let _ = app.update(Message::TogglePalette(String::new()));
        assert!(app.palette.is_none());
    }

    #[test]
    fn palette_rows_wrap_around() {
        assert_eq!(step_selection(0, -1, 3), 2);
        assert_eq!(step_selection(2, 1, 3), 0);
        assert_eq!(step_selection(0, 1, 0), 0);
    }

    #[test]
    fn activating_a_commit_row_selects_it_and_closes_the_palette() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.session_mut().state.commits = vec![commit("aaaaaaa1")];
        let _ = app.update(Message::TogglePalette("c:".to_string()));
        let _ = app.update(Message::PaletteSubmit);
        assert!(app.palette.is_none());
        assert_eq!(
            app.session().selection,
            Some(Selection::Commit {
                id: "aaaaaaa1".to_string()
            })
        );
    }

    #[test]
    fn typing_resets_the_highlighted_row() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.session_mut().state.commits = vec![commit("aaaaaaa1"), commit("bbbbbbb2")];
        let _ = app.update(Message::TogglePalette(String::new()));
        let _ = app.update(Message::PaletteMove(1));
        assert_eq!(app.palette.as_ref().map(|p| p.selected), Some(1));
        let _ = app.update(Message::PaletteQueryChanged("b".to_string()));
        assert_eq!(app.palette.as_ref().map(|p| p.selected), Some(0));
    }

    #[test]
    fn opening_a_repository_puts_it_first_in_the_recent_list() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        app.settings.recent_repositories = vec![r"D:\old".to_string()];
        let _ = app.update(Message::RepositoryOpened(
            r"D:\new".to_string(),
            Ok(OpenedRepository(std::sync::Arc::new(FakeGitRepository::ok()))),
        ));
        assert_eq!(
            app.settings.recent_repositories,
            vec![r"D:\new".to_string(), r"D:\old".to_string()]
        );
    }

    #[test]
    fn the_history_query_is_kept_as_typed() {
        let mut app = GitApp::with_repo(std::sync::Arc::new(FakeGitRepository::ok()));
        let _ = app.update(Message::HistoryQueryChanged("fork ".to_string()));
        assert_eq!(app.history_query, "fork ");
    }

    #[test]
    fn one_click_on_a_branch_filters_the_log_and_never_checks_out() {
        let mut app = on_branch("main", true);
        app.session_mut().state.branches.push(branch("feature", false));
        let _ = app.update(Message::RefClicked {
            name: "feature".to_string(),
            remote: false,
        });
        assert_eq!(app.session().branch_filter.as_deref(), Some("feature"));
        assert!(!app.session().state.loading, "a single click must not start a checkout");
        assert_eq!(
            app.session().selection,
            Some(Selection::Branch {
                name: "feature".to_string(),
                current: false,
            })
        );
    }

    #[test]
    fn a_double_click_on_a_local_branch_checks_it_out() {
        let mut app = on_branch("main", true);
        app.session_mut().state.branches.push(branch("feature", false));
        for _ in 0..2 {
            let _ = app.update(Message::RefClicked {
                name: "feature".to_string(),
                remote: false,
            });
        }
        assert!(app.session().state.loading, "the second click checks out");
    }

    #[test]
    fn a_double_click_on_a_remote_only_filters() {
        let mut app = on_branch("main", true);
        for _ in 0..2 {
            let _ = app.update(Message::RefClicked {
                name: "origin/feature".to_string(),
                remote: true,
            });
        }
        assert!(!app.session().state.loading);
        assert_eq!(app.session().branch_filter.as_deref(), Some("origin/feature"));
    }

    #[test]
    fn the_explorer_keeps_its_width_when_the_window_narrows() {
        // Ratios are of the pane grid, which is the window less the Repositories column.
        let mut app = GitApp::with_repo(Arc::new(FakeGitRepository::ok()));
        let _ = app.update(Message::WindowResized(1600.0));
        let wide = app.ratios[app.layout.index()].files;
        let wide_grid = app.grid_width();
        let _ = app.update(Message::WindowResized(1100.0));
        let narrow = app.ratios[app.layout.index()].files;
        let narrow_grid = app.grid_width();
        assert!((wide * wide_grid - layout::FILES_PX).abs() < 1.0);
        assert!((narrow * narrow_grid - layout::FILES_PX).abs() < 1.0);
        let files = mapped_slot(&app, RatioSlot::Files);
        let _ = app.update(Message::PaneResized(pane_grid::ResizeEvent { split: files, ratio: 0.30 }));
        let dragged = 0.30 * narrow_grid;
        let _ = app.update(Message::WindowResized(1000.0));
        let grid = app.grid_width();
        assert!((app.ratios[app.layout.index()].files * grid - dragged).abs() < 1.0, "keeps the dragged width");
    }

    #[test]
    fn pinning_the_repositories_column_keeps_the_explorer_width() {
        let mut app = GitApp::with_repo(Arc::new(FakeGitRepository::ok()));
        assert!(app.settings.repositories_pinned, "pinned by default");
        let _ = app.update(Message::WindowResized(1400.0));
        let pinned_grid = app.grid_width();
        let _ = app.update(Message::ToggleRepositoriesPinned);
        let rail_grid = app.grid_width();
        assert!(rail_grid > pinned_grid, "the rail is narrower than the column");
        assert!((app.ratios[app.layout.index()].files * rail_grid - layout::FILES_PX).abs() < 1.0);
    }

    #[test]
    fn forgetting_a_repository_drops_it_from_the_list() {
        let mut app = GitApp::with_repo(Arc::new(FakeGitRepository::ok()));
        app.settings.recent_repositories = vec![r"D:\a".to_string(), r"D:\b".to_string()];
        let _ = app.update(Message::ForgetRepository(r"D:\a\".to_string()));
        assert_eq!(app.settings.recent_repositories, vec![r"D:\b".to_string()]);
    }

    #[test]
    fn a_conflicted_file_opens_compare_on_yours_against_theirs() {
        let mut app = GitApp::with_repo(Arc::new(FakeGitRepository::ok()));
        app.session_mut().state.current_status = Some(core_domain::RepoStatus {
            staged_files: vec![],
            unstaged_files: vec!["plain.rs".into()],
            untracked_files: vec![],
            tracking: None,
            head_commit_id: None,
            kinds: Default::default(),
            operation: None,
            conflicted_files: vec!["clash.rs".into()],
        });
        let _ = app.update(Message::OpenCompare { path: None, against: None });
        let compare = app.compare.as_ref().expect("open");
        assert_eq!(compare.path, "clash.rs", "conflicts come first");
        assert_eq!(compare.left, CompareSide::Revision(crate::compare::STAGE_YOURS.into()));
        assert!(compare.loading);

        let _ = app.update(Message::ComparePathPicked("plain.rs".into()));
        let compare = app.compare.as_ref().expect("still open");
        assert_eq!(compare.left, CompareSide::Revision(crate::compare::HEAD.into()));
        assert_eq!(compare.right, CompareSide::WorkingTree);

        let _ = app.update(Message::Escape);
        assert!(app.compare.is_none(), "Escape closes it");
    }

    #[test]
    fn a_stale_compare_answer_is_dropped() {
        let mut app = GitApp::with_repo(Arc::new(FakeGitRepository::ok()));
        let _ = app.update(Message::OpenCompare { path: Some("a.rs".into()), against: Some("main".into()) });
        let first = app.compare.as_ref().expect("open").generation;
        let _ = app.update(Message::CompareSwap);
        let patch = core_domain::Patch { path: "a.rs".into(), hunks: vec![], binary: false };
        let _ = app.update(Message::CompareLoaded { generation: first, result: Ok(patch) });
        let compare = app.compare.as_ref().expect("open");
        assert!(compare.loading && compare.result.is_none(), "the older answer must not land");
    }

    #[test]
    fn typed_commands_echo_with_a_prompt_and_builtins_stay_local() {
        let mut app = GitApp::with_repo(Arc::new(FakeGitRepository::ok()));
        app.terminal_open = true;
        let _ = app.update(Message::ShellInputChanged("cls".into()));
        let _ = app.update(Message::ShellSubmit);
        assert!(app.console.text().is_empty(), "cls clears, the echo included");
        assert!(!app.shell_busy);
        let _ = app.update(Message::ShellInputChanged("exit".into()));
        let _ = app.update(Message::ShellSubmit);
        assert!(!app.terminal_open, "exit folds the terminal");
        assert!(app.console.text().contains("exit"));
    }

    #[test]
    fn output_of_a_stopped_command_cannot_land_in_the_next() {
        let mut app = GitApp::with_repo(Arc::new(FakeGitRepository::ok()));
        app.shell_run = 2;
        app.shell_busy = true;
        let _ = app.update(Message::Shell(1, ShellEvent::Line { error: false, text: "old".into() }));
        assert!(!app.console.text().contains("old"));
        let _ = app.update(Message::Shell(2, ShellEvent::Line { error: true, text: "fatal".into() }));
        let _ = app.update(Message::Shell(2, ShellEvent::Finished { cwd: Some(r"D:\elsewhere".into()), code: Some(1) }));
        assert!(app.console.text().contains("fatal"));
        assert!(!app.shell_busy);
        assert_eq!(app.shell_folder().as_deref(), Some(r"D:\elsewhere"), "cd carries over");
    }

    #[test]
    fn a_folder_is_inside_its_root_and_not_a_sibling() {
        assert!(folder_is_inside(r"D:\Repos\x\src", r"D:\Repos\x"));
        assert!(folder_is_inside("D:/Repos/x/", r"D:\Repos\x"));
        assert!(!folder_is_inside(r"D:\Repos\xy", r"D:\Repos\x"));
    }

    fn dirty_app() -> GitApp {
        let mut app = GitApp::with_repo(Arc::new(FakeGitRepository::ok()));
        app.files_as_tree = false;
        app.session_mut().state.current_status = Some(core_domain::RepoStatus {
            staged_files: vec!["s.rs".into()],
            unstaged_files: vec!["a.rs".into(), "b.rs".into(), "c.rs".into()],
            untracked_files: vec!["n.rs".into()],
            tracking: None,
            head_commit_id: None,
            kinds: Default::default(),
            operation: None,
            conflicted_files: vec![],
        });
        app
    }

    fn click(app: &mut GitApp, path: &str, modifiers: iced::keyboard::Modifiers) {
        let _ = app.update(Message::ModifiersChanged(modifiers));
        let _ = app.update(Message::FocusFile { path: path.into(), place: FileDisposition::Unstaged });
    }

    fn marked(app: &GitApp) -> Vec<&str> {
        app.session().marked_files.iter().map(|mark| mark.path.as_str()).collect()
    }

    #[test]
    fn click_ctrl_click_and_shift_click_build_the_selection() {
        use iced::keyboard::Modifiers;
        let mut app = dirty_app();
        click(&mut app, "a.rs", Modifiers::default());
        assert_eq!(marked(&app), ["a.rs"]);
        click(&mut app, "c.rs", Modifiers::CTRL);
        assert_eq!(marked(&app), ["a.rs", "c.rs"]);
        click(&mut app, "a.rs", Modifiers::SHIFT);
        assert_eq!(marked(&app), ["a.rs", "b.rs", "c.rs"], "range from the Ctrl-click anchor");
        click(&mut app, "b.rs", Modifiers::default());
        assert_eq!(marked(&app), ["b.rs"]);
    }

    #[test]
    fn right_click_on_a_selection_offers_actions_for_all_of_it() {
        use iced::keyboard::Modifiers;
        let mut app = dirty_app();
        click(&mut app, "a.rs", Modifiers::default());
        click(&mut app, "b.rs", Modifiers::CTRL);
        let _ = app.update(Message::ChangeContextMenu { path: "b.rs".into(), place: FileDisposition::Unstaged });
        assert!(matches!(
            app.context_menu.as_ref().map(|menu| &menu.target),
            Some(crate::context_menu::ContextTarget::Changes { files }) if files.len() == 2
        ));
        // Outside the selection: that row alone.
        let _ = app.update(Message::ChangeContextMenu { path: "c.rs".into(), place: FileDisposition::Unstaged });
        assert_eq!(marked(&app), ["c.rs"]);
    }

    #[test]
    fn discarding_a_selection_asks_once_for_every_file() {
        use iced::keyboard::Modifiers;
        let mut app = dirty_app();
        click(&mut app, "a.rs", Modifiers::default());
        click(&mut app, "c.rs", Modifiers::SHIFT);
        let _ = app.update(Message::AskDiscard);
        assert_eq!(app.confirm_discard.as_ref().map(Vec::len), Some(3));
    }

    #[test]
    fn stash_opens_a_dialog_and_keeps_the_options() {
        let mut app = dirty_app();
        let _ = app.update(Message::StashPush);
        let _ = app.update(Message::StashMessageChanged("wip: half done".into()));
        let _ = app.update(Message::StashUntrackedToggled(true));
        assert_eq!(
            app.stash_prompt,
            Some(StashPrompt { message: "wip: half done".into(), include_untracked: true, paths: vec![] })
        );
        let _ = app.update(Message::ConfirmStash);
        assert!(app.stash_prompt.is_none());
        assert!(app.session().state.loading);
    }

    #[test]
    fn merge_opens_without_a_selection_and_waits_for_a_branch() {
        let mut app = GitApp::with_repo(Arc::new(FakeGitRepository::ok()));
        app.session_mut().state.branches = vec![branch("main", true), branch("feature", false)];
        let _ = app.update(Message::AskMerge);
        assert_eq!(app.history_prompt, Some(HistoryPrompt::Merge { name: String::new(), no_ff: false }));
        let _ = app.update(Message::ConfirmMerge);
        assert!(!app.session().state.loading, "no branch picked yet");
        let _ = app.update(Message::MergeTargetPicked("feature".into()));
        let _ = app.update(Message::MergeNoFfToggled(true));
        let _ = app.update(Message::ConfirmMerge);
        assert!(app.session().state.loading);
    }

    #[test]
    fn a_conflict_reads_as_a_stop_not_as_gits_first_line() {
        let git = "Auto-merging Config.cs\nCONFLICT (content): Merge conflict in Config.cs\nAutomatic merge failed; fix conflicts and then commit the result.";
        assert_eq!(stopped_on_conflicts(git), Some(STOPPED_ON_CONFLICTS));
        assert_eq!(stopped_on_conflicts("fatal: not something we can merge"), None);
    }

    #[test]
    fn stepping_through_differences_stops_at_the_last() {
        use core_domain::{DiffLine, DiffLineKind, Hunk, Patch};
        let line = |kind, text: &str| DiffLine { kind, text: text.into() };
        let mut app = GitApp::with_repo(Arc::new(FakeGitRepository::ok()));
        app.session_mut().patches = vec![Patch {
            path: "a.rs".into(),
            binary: false,
            hunks: vec![Hunk {
                header: "@@ -1,3 +1,3 @@".into(),
                lines: vec![
                    line(DiffLineKind::Removed, "a"),
                    line(DiffLineKind::Added, "A"),
                    line(DiffLineKind::Context, "b"),
                    line(DiffLineKind::Added, "c"),
                ],
            }],
        }];
        let _ = app.update(Message::StepDiffChange(true));
        assert_eq!(app.session().diff_change, Some(0));
        let _ = app.update(Message::StepDiffChange(true));
        assert_eq!(app.session().diff_change, Some(1));
        let _ = app.update(Message::StepDiffChange(true));
        assert_eq!(app.session().diff_change, Some(1), "stays on the last difference");
        let _ = app.update(Message::ToggleDiffSplit);
        assert!(!app.diff_split);
        assert_eq!(app.session().diff_change, None);
    }

    #[test]
    fn the_branch_picker_sets_focus_or_a_branch_filter() {
        use crate::log_filter::LogChoice;
        let mut app = GitApp::with_repo(Arc::new(FakeGitRepository::ok()));
        let _ = app.update(Message::BranchChoicePicked(LogChoice::Named("dev".into())));
        assert_eq!(app.session().branch_filter.as_deref(), Some("dev"));
        assert!(!app.session().focus_current_branch);
        let _ = app.update(Message::BranchChoicePicked(LogChoice::Head));
        assert!(app.session().focus_current_branch);
        assert!(app.session().branch_filter.is_none());
        let _ = app.update(Message::BranchChoicePicked(LogChoice::All));
        assert!(!app.session().focus_current_branch);
    }

    #[test]
    fn the_user_picker_sets_and_clears_the_author_filter() {
        use crate::log_filter::LogChoice;
        let mut app = GitApp::with_repo(Arc::new(FakeGitRepository::ok()));
        let _ = app.update(Message::AuthorChoicePicked(LogChoice::Named("Ada".into())));
        assert_eq!(app.session().author_filter.as_deref(), Some("Ada"));
        let _ = app.update(Message::AuthorChoicePicked(LogChoice::All));
        assert!(app.session().author_filter.is_none());
    }

    #[test]
    fn focus_and_show_all_clear_the_branch_filter() {
        let mut app = on_branch("main", true);
        app.session_mut().branch_filter = Some("feature".to_string());
        let _ = app.update(Message::ToggleFocusCurrentBranch);
        assert!(app.session().branch_filter.is_none());
        app.session_mut().branch_filter = Some("feature".to_string());
        let _ = app.update(Message::ClearBranchFilter);
        assert!(app.session().branch_filter.is_none());
        assert!(!app.session().focus_current_branch);
    }

    #[test]
    fn a_new_branch_can_start_from_a_remote_and_skip_checkout() {
        let mut app = GitApp::with_repo(Arc::new(FakeGitRepository::ok()));
        app.session_mut().state.branches = vec![
            branch("main", true),
            Branch {
                name: "origin/feature".to_string(),
                is_remote: true,
                is_head: false,
                tip: "abc".to_string(),
                ahead: 0,
                behind: 0,
            },
        ];
        let _ = app.update(Message::AskCreateBranchFrom("origin/feature".to_string()));
        let _ = app.update(Message::BranchNameChanged("feature".to_string()));
        let _ = app.update(Message::BranchCheckoutToggled(false));
        let _ = app.update(Message::ConfirmBranch);
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.session().state.loading, "the create started");
        assert!(app.session().pending_checkout.is_none(), "checkout was unticked");
    }

    #[test]
    fn checking_out_a_remote_creates_the_tracking_branch_first() {
        let mut app = GitApp::with_repo(Arc::new(FakeGitRepository::ok()));
        app.session_mut().state.branches = vec![branch("main", true)];
        let _ = app.update(Message::ContextMenuAction(
            crate::context_menu::ContextActionKind::CheckoutRemote {
                remote: "origin/feature/x".to_string(),
                local: "feature/x".to_string(),
            },
        ));
        assert!(app.session().state.loading);
        assert_eq!(app.session().pending_checkout.as_deref(), Some("feature/x"));
    }
}
