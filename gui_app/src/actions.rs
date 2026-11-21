//! The action catalogue: every command the window can run by key, from the command palette
//! or from the shortcuts sheet. One table, so a new action is added in one place.
//!
//! Pure: no adapter calls and no window. `availability` says whether an action can run
//! right now and, when it cannot, why in a sentence the user can read.

use crate::tracking::{
    PushKind, BUSY_REASON, DIVERGED_REASON, HAS_UPSTREAM_REASON, LOADING_REASON, NOT_DIVERGED_REASON,
    NO_UPSTREAM_REASON,
};
use core_domain::RepoOperation;
use iced::keyboard::key::Named;
use iced::keyboard::{Key, Modifiers};

/// Everything the catalogue can run. Add a variant here and a row in [`ACTIONS`]; the
/// compiler then points at `availability` and `GitApp::message_for` for the rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ActionId {
    Fetch,
    Pull,
    Push,
    /// Push a branch that has no upstream and track it.
    Publish,
    /// Ask, then force push with a lease. Never bound to a key that skips the dialog.
    ForcePush,
    Refresh,
    StageMarked,
    UnstageMarked,
    Stash,
    Commit,
    AmendLastCommit,
    UndoLastCommit,
    /// Search for stashes that were dropped and offer to apply them again.
    FindLostStashes,
    ToggleTerminal,
    CycleLayout,
    CycleHunkMode,
    QuickOpen,
    CommandPalette,
    FocusHistorySearch,
    ShowShortcuts,
    ToggleDiffSplit,
    ToggleFileTree,
    OpenSettings,
}

/// The key of a chord. Letters are stored lower-case and match either case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChordKey {
    Char(char),
    F5,
    Enter,
    Backquote,
    Slash,
}

impl ChordKey {
    fn label(self) -> String {
        match self {
            Self::Char(c) => c.to_ascii_uppercase().to_string(),
            Self::F5 => "F5".to_string(),
            Self::Enter => "Enter".to_string(),
            Self::Backquote => "`".to_string(),
            Self::Slash => "/".to_string(),
        }
    }

    fn matches(self, key: &Key) -> bool {
        match (self, key) {
            (Self::Char(wanted), Key::Character(c)) => {
                let mut chars = c.chars();
                chars
                    .next()
                    .is_some_and(|got| got.eq_ignore_ascii_case(&wanted))
                    && chars.next().is_none()
            }
            (Self::Backquote, Key::Character(c)) => c.as_str() == "`",
            (Self::Slash, Key::Character(c)) => c.as_str() == "/",
            (Self::F5, Key::Named(Named::F5)) => true,
            (Self::Enter, Key::Named(Named::Enter)) => true,
            _ => false,
        }
    }
}

/// A key with the modifiers that must be held. Ctrl also means Cmd on macOS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Chord {
    pub ctrl: bool,
    pub shift: bool,
    pub key: ChordKey,
}

impl Chord {
    /// "Ctrl+Shift+P".
    pub(crate) fn label(&self) -> String {
        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("Ctrl".to_string());
        }
        if self.shift {
            parts.push("Shift".to_string());
        }
        parts.push(self.key.label());
        parts.join("+")
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ActionSpec {
    pub id: ActionId,
    pub label: &'static str,
    /// Groups the rows of the shortcuts sheet.
    pub category: &'static str,
    pub chord: Option<Chord>,
}

const fn ctrl(key: ChordKey) -> Option<Chord> {
    Some(Chord {
        ctrl: true,
        shift: false,
        key,
    })
}

const fn ctrl_shift(key: ChordKey) -> Option<Chord> {
    Some(Chord {
        ctrl: true,
        shift: true,
        key,
    })
}

const fn bare(key: ChordKey) -> Option<Chord> {
    Some(Chord {
        ctrl: false,
        shift: false,
        key,
    })
}

const fn spec(
    id: ActionId,
    label: &'static str,
    category: &'static str,
    chord: Option<Chord>,
) -> ActionSpec {
    ActionSpec {
        id,
        label,
        category,
        chord,
    }
}

/// Every action, in the order the palette and the shortcuts sheet list them.
/// Undo and force push have no default chord: a key that rewrites history must be chosen on
/// purpose.
pub(crate) const ACTIONS: &[ActionSpec] = &[
    spec(ActionId::Fetch, "Fetch", "Git", bare(ChordKey::F5)),
    spec(ActionId::Pull, "Pull", "Git", None),
    spec(ActionId::Push, "Push", "Git", None),
    spec(ActionId::Publish, "Publish branch", "Git", None),
    spec(ActionId::ForcePush, "Force push", "Git", None),
    spec(
        ActionId::Refresh,
        "Refresh repository",
        "Git",
        ctrl(ChordKey::Char('r')),
    ),
    spec(
        ActionId::StageMarked,
        "Stage marked files",
        "Changes",
        ctrl(ChordKey::Char('s')),
    ),
    spec(ActionId::UnstageMarked, "Unstage marked files", "Changes", None),
    spec(
        ActionId::Stash,
        "Stash changes",
        "Changes",
        ctrl_shift(ChordKey::Char('s')),
    ),
    spec(
        ActionId::Commit,
        "Commit staged changes",
        "Changes",
        ctrl(ChordKey::Enter),
    ),
    spec(ActionId::AmendLastCommit, "Amend last commit", "Changes", None),
    spec(ActionId::UndoLastCommit, "Undo last commit", "Changes", None),
    spec(ActionId::FindLostStashes, "Find lost stashes", "Changes", None),
    spec(
        ActionId::QuickOpen,
        "Go to branch, file, commit…",
        "Navigate",
        ctrl(ChordKey::Char('p')),
    ),
    spec(
        ActionId::CommandPalette,
        "Command palette",
        "Navigate",
        ctrl_shift(ChordKey::Char('p')),
    ),
    spec(
        ActionId::FocusHistorySearch,
        "Search history",
        "Navigate",
        ctrl(ChordKey::Char('k')),
    ),
    spec(
        ActionId::ToggleTerminal,
        "Toggle terminal",
        "View",
        ctrl(ChordKey::Backquote),
    ),
    spec(
        ActionId::CycleLayout,
        "Cycle layout",
        "View",
        ctrl_shift(ChordKey::Char('l')),
    ),
    spec(
        ActionId::CycleHunkMode,
        "Cycle hunk view",
        "View",
        ctrl_shift(ChordKey::Char('h')),
    ),
    spec(ActionId::ToggleDiffSplit, "Toggle split diff", "View", None),
    spec(
        ActionId::ToggleFileTree,
        "Toggle changes as tree or list",
        "View",
        None,
    ),
    spec(
        ActionId::ShowShortcuts,
        "Keyboard shortcuts",
        "App",
        ctrl(ChordKey::Slash),
    ),
    spec(ActionId::OpenSettings, "Settings", "App", None),
];

/// The action a key press stands for, or `None`. Modifiers must match exactly.
pub(crate) fn action_for(key: &Key, modifiers: Modifiers) -> Option<ActionId> {
    let ctrl = modifiers.control() || modifiers.logo();
    let shift = modifiers.shift();
    ACTIONS
        .iter()
        .find(|spec| {
            spec.chord.is_some_and(|chord| {
                chord.ctrl == ctrl && chord.shift == shift && chord.key.matches(key)
            })
        })
        .map(|spec| spec.id)
}

/// Actions that must also fire while a text field has focus, because a text field would
/// otherwise swallow the key. Everything else waits until no field has it.
pub(crate) fn works_while_typing(id: ActionId) -> bool {
    matches!(
        id,
        ActionId::QuickOpen
            | ActionId::CommandPalette
            | ActionId::FocusHistorySearch
            | ActionId::ToggleTerminal
            | ActionId::ShowShortcuts
            | ActionId::Commit
    )
}

/// Why a git action waits: a prompt or dialog is open, and it is what the user is acting on.
pub(crate) const MODAL_OPEN_REASON: &str = "Close the open dialog first";

/// Actions that run git or reload the repository. Busy, loading and an open dialog block
/// these only; UI actions (palette, terminal, layout, settings) stay available.
pub(crate) fn touches_git(id: ActionId) -> bool {
    matches!(
        id,
        ActionId::Fetch
            | ActionId::Pull
            | ActionId::Push
            | ActionId::Publish
            | ActionId::ForcePush
            | ActionId::Refresh
            | ActionId::StageMarked
            | ActionId::UnstageMarked
            | ActionId::Stash
            | ActionId::Commit
            | ActionId::AmendLastCommit
            | ActionId::UndoLastCommit
            | ActionId::FindLostStashes
    )
}

/// Why a merge commit is not undone here: a soft reset to its first parent would stage
/// everything the merge brought in as one change.
pub(crate) const MERGE_HEAD_REASON: &str = "HEAD is a merge commit: undo it from the terminal";

/// Why undo waits while `operation` is in progress: moving the branch under it would corrupt
/// what `--continue` does. The adapter refuses with the same words.
pub(crate) fn finish_operation_reason(operation: RepoOperation) -> &'static str {
    match operation {
        RepoOperation::Merge => "Finish or abort the merge first.",
        RepoOperation::Rebase => "Finish or abort the rebase first.",
        RepoOperation::CherryPick => "Finish or abort the cherry-pick first.",
        RepoOperation::Revert => "Finish or abort the revert first.",
    }
}

/// What the window knows that decides whether an action can run now.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ActionContext {
    pub has_session: bool,
    /// The repository has not been read yet.
    pub loading: bool,
    /// A git action or a reload is running.
    pub busy: bool,
    pub head_born: bool,
    pub has_upstream: bool,
    pub has_staged: bool,
    /// The commit summary has some text; a body alone never makes a commit.
    pub has_summary: bool,
    pub has_marked: bool,
    /// Staged or unstaged tracked changes.
    pub has_changes: bool,
    /// The Inspector shows another worktree, so the commit box is not the one on screen.
    pub inspecting_other: bool,
    /// The palette is open: Ctrl+Enter belongs to it.
    pub palette_open: bool,
    /// A prompt, dialog, menu or sheet is open (`GitApp::modal_open`; the palette is not one).
    /// Git actions wait until it closes.
    pub modal_open: bool,
    /// The Amend box is ticked: committing replaces HEAD, so nothing need be staged.
    pub amending: bool,
    /// The HEAD commit has more than one parent (false while it is not loaded).
    pub head_is_merge: bool,
    /// The merge, rebase, cherry-pick or revert in progress, if any.
    pub operation: Option<RepoOperation>,
    /// Which push the branch is ready for, or why none: `tracking::push_kind`, which the top
    /// bar's Push button reads too.
    pub push: Result<PushKind, &'static str>,
}

impl Default for ActionContext {
    fn default() -> Self {
        Self {
            has_session: false,
            loading: false,
            busy: false,
            head_born: false,
            has_upstream: false,
            has_staged: false,
            has_summary: false,
            has_marked: false,
            has_changes: false,
            inspecting_other: false,
            palette_open: false,
            modal_open: false,
            amending: false,
            head_is_merge: false,
            operation: None,
            push: Err(NO_UPSTREAM_REASON),
        }
    }
}

/// Push, Publish or Force push against the branch's one push decision. Each is available
/// only when it is what the Push button would do; otherwise the reason says what is.
fn push_availability(wanted: PushKind, plan: Result<PushKind, &'static str>) -> Result<(), &'static str> {
    match (wanted, plan?) {
        (wanted, ready) if wanted == ready => Ok(()),
        (_, PushKind::Publish) => Err(NO_UPSTREAM_REASON),
        (PushKind::Publish, _) => Err(HAS_UPSTREAM_REASON),
        (PushKind::Push, _) => Err(DIVERGED_REASON),
        (PushKind::ForcePush, _) => Err(NOT_DIVERGED_REASON),
    }
}

/// `Ok` when `id` can run, else the reason in words. The reasons for Fetch, Pull, Push,
/// Publish and Force push are the ones the top bar shows on its disabled buttons.
pub(crate) fn availability(id: ActionId, cx: &ActionContext) -> Result<(), &'static str> {
    if !cx.has_session {
        return Err("No repository open");
    }
    if touches_git(id) {
        if cx.modal_open {
            return Err(MODAL_OPEN_REASON);
        }
        if cx.busy {
            return Err(BUSY_REASON);
        }
        if cx.loading {
            return Err(LOADING_REASON);
        }
    }
    if let (ActionId::UndoLastCommit, Some(operation)) = (id, cx.operation) {
        return Err(finish_operation_reason(operation));
    }
    match id {
        ActionId::Pull if !cx.has_upstream => Err(NO_UPSTREAM_REASON),
        ActionId::Push => push_availability(PushKind::Push, cx.push),
        ActionId::Publish => push_availability(PushKind::Publish, cx.push),
        ActionId::ForcePush => push_availability(PushKind::ForcePush, cx.push),
        ActionId::StageMarked | ActionId::UnstageMarked if !cx.has_marked => {
            Err("No files are marked")
        }
        ActionId::Stash if !cx.head_born => Err("Nothing to stash before the first commit"),
        ActionId::Stash if !cx.has_changes => Err("No tracked changes to stash"),
        ActionId::Commit if cx.palette_open => Err("Close the palette first"),
        ActionId::Commit if cx.inspecting_other => {
            Err("Showing another worktree: open its tab to commit")
        }
        ActionId::Commit if !cx.has_staged && !cx.amending => Err("Nothing is staged"),
        ActionId::Commit if !cx.has_summary => Err("Write a commit summary first"),
        ActionId::AmendLastCommit | ActionId::UndoLastCommit if !cx.head_born => {
            Err("No commits yet")
        }
        ActionId::AmendLastCommit if cx.amending => Err("Amend is already ticked"),
        ActionId::UndoLastCommit if cx.head_is_merge => Err(MERGE_HEAD_REASON),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_actions_wait_for_an_open_dialog_and_ui_actions_do_not() {
        let ready = ActionContext {
            has_session: true,
            head_born: true,
            has_staged: true,
            has_summary: true,
            ..ActionContext::default()
        };
        assert_eq!(availability(ActionId::Commit, &ready), Ok(()));
        let behind_a_dialog = ActionContext { modal_open: true, ..ready };
        for spec in ACTIONS {
            if touches_git(spec.id) {
                assert_eq!(availability(spec.id, &behind_a_dialog), Err(MODAL_OPEN_REASON), "{:?}", spec.id);
            } else {
                assert_eq!(availability(spec.id, &behind_a_dialog), availability(spec.id, &ready), "{:?}", spec.id);
            }
        }
    }

    #[test]
    fn finding_lost_stashes_needs_a_repository_and_waits_for_git() {
        let none = ActionContext::default();
        assert_eq!(availability(ActionId::FindLostStashes, &none), Err("No repository open"));
        let open = ActionContext { has_session: true, ..ActionContext::default() };
        assert_eq!(availability(ActionId::FindLostStashes, &open), Ok(()));
        let busy = ActionContext { busy: true, ..open };
        assert_eq!(availability(ActionId::FindLostStashes, &busy), Err(BUSY_REASON));
        let loading = ActionContext { loading: true, ..open };
        assert_eq!(availability(ActionId::FindLostStashes, &loading), Err(LOADING_REASON));
        let spec = ACTIONS.iter().find(|spec| spec.id == ActionId::FindLostStashes).expect("listed");
        assert!(spec.chord.is_none(), "no default key");
    }

    #[test]
    fn every_action_is_listed_once_and_no_two_share_a_chord() {
        for (at, spec) in ACTIONS.iter().enumerate() {
            assert!(
                ACTIONS[..at].iter().all(|earlier| earlier.id != spec.id),
                "{:?} is listed twice",
                spec.id
            );
            if let Some(chord) = spec.chord {
                assert!(
                    ACTIONS[..at]
                        .iter()
                        .all(|earlier| earlier.chord != Some(chord)),
                    "{} is bound twice",
                    chord.label()
                );
            }
        }
    }

    #[test]
    fn todays_keys_still_resolve() {
        assert_eq!(
            action_for(&Key::Named(Named::F5), Modifiers::empty()),
            Some(ActionId::Fetch)
        );
        assert_eq!(
            action_for(&Key::Character("r".into()), Modifiers::CTRL),
            Some(ActionId::Refresh)
        );
        assert_eq!(
            action_for(&Key::Character("S".into()), Modifiers::CTRL | Modifiers::SHIFT),
            Some(ActionId::Stash)
        );
        assert_eq!(
            action_for(&Key::Character("p".into()), Modifiers::CTRL | Modifiers::SHIFT),
            Some(ActionId::CommandPalette)
        );
        assert_eq!(
            action_for(&Key::Character("s".into()), Modifiers::CTRL),
            Some(ActionId::StageMarked)
        );
        assert_eq!(
            action_for(&Key::Character("p".into()), Modifiers::CTRL),
            Some(ActionId::QuickOpen)
        );
        assert_eq!(
            action_for(&Key::Character("`".into()), Modifiers::CTRL),
            Some(ActionId::ToggleTerminal)
        );
        assert_eq!(
            action_for(&Key::Character("/".into()), Modifiers::CTRL),
            Some(ActionId::ShowShortcuts)
        );
        assert_eq!(
            action_for(&Key::Named(Named::Enter), Modifiers::CTRL),
            Some(ActionId::Commit)
        );
        assert_eq!(
            action_for(&Key::Character("p".into()), Modifiers::empty()),
            None
        );
    }

    #[test]
    fn a_chord_reads_as_its_keys() {
        let find = |id| ACTIONS.iter().find(|spec| spec.id == id).unwrap();
        assert_eq!(
            find(ActionId::CommandPalette).chord.unwrap().label(),
            "Ctrl+Shift+P"
        );
        assert_eq!(find(ActionId::Fetch).chord.unwrap().label(), "F5");
    }

    #[test]
    fn an_unavailable_action_says_why() {
        let idle = ActionContext {
            has_session: true,
            ..Default::default()
        };
        assert_eq!(availability(ActionId::Commit, &idle), Err("Nothing is staged"));
        let staged = ActionContext {
            has_staged: true,
            ..idle
        };
        assert_eq!(
            availability(ActionId::Commit, &staged),
            Err("Write a commit summary first")
        );
        let ready = ActionContext {
            has_summary: true,
            ..staged
        };
        assert_eq!(availability(ActionId::Commit, &ready), Ok(()));
        assert_eq!(
            availability(ActionId::Pull, &idle),
            Err("No upstream: this branch does not track a remote branch")
        );
        let busy = ActionContext { busy: true, ..idle };
        assert_eq!(
            availability(ActionId::Fetch, &busy),
            Err("Another git action is running")
        );
    }

    #[test]
    fn undo_and_amend_are_unavailable_before_the_first_commit() {
        let cx = ActionContext {
            has_session: true,
            head_born: false,
            ..Default::default()
        };
        assert_eq!(availability(ActionId::UndoLastCommit, &cx), Err("No commits yet"));
        assert_eq!(availability(ActionId::AmendLastCommit, &cx), Err("No commits yet"));
        let born = ActionContext {
            head_born: true,
            ..cx
        };
        assert_eq!(availability(ActionId::UndoLastCommit, &born), Ok(()));
        assert_eq!(availability(ActionId::AmendLastCommit, &born), Ok(()));
        let busy = ActionContext { busy: true, ..born };
        assert_eq!(availability(ActionId::UndoLastCommit, &busy), Err(BUSY_REASON));
        assert_eq!(availability(ActionId::AmendLastCommit, &busy), Err(BUSY_REASON));
    }

    #[test]
    fn a_merge_commit_is_not_undone_but_can_be_amended() {
        let merge = ActionContext {
            has_session: true,
            head_born: true,
            head_is_merge: true,
            ..Default::default()
        };
        assert_eq!(
            availability(ActionId::UndoLastCommit, &merge),
            Err("HEAD is a merge commit: undo it from the terminal")
        );
        assert_eq!(availability(ActionId::AmendLastCommit, &merge), Ok(()));
    }

    #[test]
    fn undo_waits_for_an_operation_in_progress() {
        let merging = ActionContext {
            has_session: true,
            head_born: true,
            operation: Some(RepoOperation::Merge),
            ..Default::default()
        };
        assert_eq!(
            availability(ActionId::UndoLastCommit, &merging),
            Err("Finish or abort the merge first.")
        );
        let rebasing = ActionContext {
            operation: Some(RepoOperation::Rebase),
            ..merging
        };
        assert_eq!(
            availability(ActionId::UndoLastCommit, &rebasing),
            Err("Finish or abort the rebase first.")
        );
        for operation in [RepoOperation::Merge, RepoOperation::Rebase, RepoOperation::CherryPick, RepoOperation::Revert] {
            assert_eq!(
                finish_operation_reason(operation),
                format!("Finish or abort the {} first.", operation.noun())
            );
        }
    }

    #[test]
    fn push_publish_and_force_push_agree_with_the_top_bar() {
        use crate::tracking::{push_kind, sync_buttons, SyncAction};
        use core_domain::{RepoStatus, TrackingStatus};
        let status = |tracking: Option<(usize, usize, bool)>, born: bool| RepoStatus {
            staged_files: vec![],
            unstaged_files: vec![],
            untracked_files: vec![],
            tracking: tracking.map(|(ahead, behind, rewritten_locally)| TrackingStatus {
                upstream: "origin/main".into(),
                ahead,
                behind,
                rewritten_locally,
            }),
            head_commit_id: born.then(|| "abc".to_string()),
            kinds: Default::default(),
            operation: None,
            conflicted_files: vec![],
        };
        let remotes = |names: &[&str]| names.iter().map(|name| name.to_string()).collect::<Vec<_>>();
        let cases = [
            (status(None, true), remotes(&["origin"])),
            (status(None, true), remotes(&["fork"])),
            (status(None, true), remotes(&["fork", "upstream"])),
            (status(None, true), remotes(&[])),
            (status(None, false), remotes(&["origin"])),
            (status(Some((1, 2, true)), true), remotes(&["origin"])),
            (status(Some((1, 2, false)), true), remotes(&["origin"])),
            (status(Some((2, 0, false)), true), remotes(&["origin"])),
            (status(Some((0, 0, false)), true), remotes(&["origin"])),
        ];
        for (status, remotes) in cases {
            let [_, _, button] = sync_buttons(Some(&status), &remotes, None);
            let cx = ActionContext {
                has_session: true,
                head_born: status.head_commit_id.is_some(),
                has_upstream: status.tracking.is_some(),
                push: push_kind(Some(&status), &remotes),
                ..Default::default()
            };
            let pressed = match &button.action {
                SyncAction::Push => ActionId::Push,
                SyncAction::Publish(_) => ActionId::Publish,
                SyncAction::ForcePush => ActionId::ForcePush,
                other => panic!("the push button never does {other:?}"),
            };
            let expected = if button.enabled { Ok(()) } else { Err(button.reason.expect("a disabled button says why")) };
            assert_eq!(availability(pressed, &cx), expected, "{:?} {remotes:?}", status.tracking);
            // The other two are unavailable whenever the button does not do them.
            for other in [ActionId::Push, ActionId::Publish, ActionId::ForcePush] {
                if other != pressed {
                    assert!(availability(other, &cx).is_err(), "{other:?} with {:?} {remotes:?}", status.tracking);
                }
            }
        }
    }

    #[test]
    fn publish_and_force_push_wait_like_every_git_action() {
        let ready = ActionContext {
            has_session: true,
            head_born: true,
            push: Ok(crate::tracking::PushKind::ForcePush),
            ..Default::default()
        };
        assert_eq!(availability(ActionId::ForcePush, &ready), Ok(()));
        let busy = ActionContext { busy: true, ..ready };
        let loading = ActionContext { loading: true, ..ready };
        for id in [ActionId::Publish, ActionId::ForcePush] {
            assert_eq!(availability(id, &busy), Err(BUSY_REASON));
            assert_eq!(availability(id, &loading), Err(LOADING_REASON));
        }
    }

    #[test]
    fn no_rewrite_has_a_default_chord() {
        for id in [ActionId::AmendLastCommit, ActionId::UndoLastCommit, ActionId::ForcePush, ActionId::Publish] {
            let spec = ACTIONS.iter().find(|spec| spec.id == id).expect("listed");
            assert_eq!(spec.chord, None, "{id:?}");
        }
    }

    #[test]
    fn while_amending_a_commit_needs_nothing_staged() {
        let amending = ActionContext {
            has_session: true,
            head_born: true,
            has_summary: true,
            amending: true,
            ..Default::default()
        };
        assert_eq!(availability(ActionId::Commit, &amending), Ok(()));
        assert_eq!(
            availability(ActionId::AmendLastCommit, &amending),
            Err("Amend is already ticked")
        );
    }

    #[test]
    fn without_a_repository_nothing_runs_and_loading_is_said_before_the_action() {
        assert_eq!(
            availability(ActionId::QuickOpen, &ActionContext::default()),
            Err("No repository open")
        );
        let loading = ActionContext {
            has_session: true,
            loading: true,
            ..Default::default()
        };
        assert_eq!(
            availability(ActionId::Fetch, &loading),
            Err("Waiting for the repository to load")
        );
        assert_eq!(availability(ActionId::ToggleTerminal, &loading), Ok(()));
    }
}
