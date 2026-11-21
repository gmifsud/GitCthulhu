//! The action catalogue: every command the window can run by key, from the command palette
//! or from the shortcuts sheet. One table, so a new action is added in one place.
//!
//! Pure: no adapter calls and no window. `availability` says whether an action can run
//! right now and, when it cannot, why in a sentence the user can read.

use crate::tracking::{BUSY_REASON, LOADING_REASON, NO_UPSTREAM_REASON};
use iced::keyboard::key::Named;
use iced::keyboard::{Key, Modifiers};

/// Everything the catalogue can run. Add a variant here and a row in [`ACTIONS`]; the
/// compiler then points at `availability` and `GitApp::message_for` for the rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ActionId {
    Fetch,
    Pull,
    Push,
    Refresh,
    StageMarked,
    UnstageMarked,
    Stash,
    Commit,
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
/// Undo has no default chord: a key that rewrites history must be chosen on purpose.
pub(crate) const ACTIONS: &[ActionSpec] = &[
    spec(ActionId::Fetch, "Fetch", "Git", bare(ChordKey::F5)),
    spec(ActionId::Pull, "Pull", "Git", None),
    spec(ActionId::Push, "Push", "Git", None),
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

/// What the window knows that decides whether an action can run now.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ActionContext {
    pub has_session: bool,
    /// The repository has not been read yet.
    pub loading: bool,
    /// A git action or a reload is running.
    pub busy: bool,
    pub head_born: bool,
    pub has_upstream: bool,
    pub has_staged: bool,
    pub has_marked: bool,
    /// Staged or unstaged tracked changes.
    pub has_changes: bool,
}

/// `Ok` when `id` can run, else the reason in words. The reasons for Fetch, Pull and Push
/// are the ones the top bar shows on its disabled buttons.
pub(crate) fn availability(id: ActionId, cx: &ActionContext) -> Result<(), &'static str> {
    if !cx.has_session {
        return Err("No repository open");
    }
    let touches_git = matches!(
        id,
        ActionId::Fetch
            | ActionId::Pull
            | ActionId::Push
            | ActionId::Refresh
            | ActionId::StageMarked
            | ActionId::UnstageMarked
            | ActionId::Stash
            | ActionId::Commit
    );
    if touches_git {
        if cx.busy {
            return Err(BUSY_REASON);
        }
        if cx.loading {
            return Err(LOADING_REASON);
        }
    }
    match id {
        ActionId::Pull | ActionId::Push if !cx.has_upstream => Err(NO_UPSTREAM_REASON),
        ActionId::StageMarked | ActionId::UnstageMarked if !cx.has_marked => {
            Err("No files are marked")
        }
        ActionId::Stash if !cx.head_born => Err("Nothing to stash before the first commit"),
        ActionId::Stash if !cx.has_changes => Err("No tracked changes to stash"),
        ActionId::Commit if !cx.has_staged => Err("Nothing is staged"),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
