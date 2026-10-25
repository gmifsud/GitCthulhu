# Worktree tabs — design

Status: Approved for planning (2026-10-25); **amended 2026-10-25** per ADR 0003  
Related: ADR 0001, ADR 0002, ADR 0003, `CONTEXT.md` “Later: several repositories and worktrees”

## Problem

GitCthulhu opens one repository path at a time. Linked git worktrees are ordinary folders today: Open… replaces the session. There is no catalogue, no tab strip, and no in-app create/remove. Parallel feature work needs several checkouts of the same repo without juggling windows.

## Goals

- List every worktree for the open repository in the left explorer (Worktrees section).
- Open a worktree into a tab; close a tab without deleting the worktree.
- Default: one tab after Open/Clone (the path just opened).
- Full lifecycle in-app: create and remove (remove always confirms; force is explicit).
- Tab retention mode is configurable in Settings and persisted; default keeps commit message and file marks, clears graph/file selection when leaving a tab.
- Preserve ADR 0001: `gui_app` never names `git_adapter`; paths open through composition-root openers.
- Switch tabs without reloading repository state (session owns `AppState`).

## Non-goals (this cut)

- Tabs for unrelated repositories (other clones).
- Document tabs (Commit Graph / Changes-in-range as separate strip entries) — see ADR 0003 Decision 1.
- Worktree lock management beyond showing locked and failing with git’s message.
- `git worktree prune` UI.
- Changing the integrated terminal into a worktree manager.
- Compare / split-diff (WP2 / WP3 in ADR 0003) — planned after this WP1 lands.

## Approach

**Multi-session tabs.** Each open tab owns its own `Arc<dyn GitRepository>`, path, **`AppState`**, and UI bag. The explorer always lists `git worktree list`. Tab close drops the session only. Create/remove run through new port methods and refresh the catalogue. Switching tabs is a pointer move, not a `load_repository`.

Rejected: single-adapter path hopping; shared `AppState` reloaded on activate; one OS window per worktree; a second strip of document tabs.

## Window chrome

### Tab strip (sessions only)

- Sits under the top bar, full width. This is the **only** tab strip in the window.
- One tab per open **session** (worktree path). Label: folder basename; tooltip: full path and branch.
- Active tab is visually distinct (raised background + underline).
- `×` closes that tab (session only). Closing the last tab is refused while a repository is loaded.
- `+` opens a picker of catalogue entries not already open (same data as the explorer list).
- Compare and multi-file diffs are **not** tabs: Compare is an inspector mode (WP2); the Diff pane stays the Diff pane (WP3).

### Explorer — Worktrees section

- Lives in the left Changes / Files column (with branch switcher and staging).
- Header: “Worktrees” plus count.
- Rows: basename, checked-out branch, locked marker when present; active session highlighted.
- Click closed entry → open as tab and focus. Click already-open entry → focus that tab.
- Actions: New worktree…, Remove… (not available on the main worktree).

### Active session

Everything below the tab strip binds to `sessions[active]`: adapter, `AppState`, selection, commit message, marks, patches, loading for that path.

## Domain and port

### Model

```text
Worktree {
  path: String,       // absolute working directory
  branch: String,     // checked-out branch name, or empty / "(detached)" when detached
  is_main: bool,      // primary worktree (not a linked one)
  locked: bool,       // from porcelain lock
}
```

Catalogue is refreshed on repository load, after create/remove, and when the user refreshes.

### `GitRepository` additions

| Method | Behaviour |
|---|---|
| `worktrees() -> Result<Vec<Worktree>, DomainError>` | `git worktree list --porcelain` |
| `add_worktree(path, branch, create_branch: bool) -> Result<(), DomainError>` | `git worktree add` or `git worktree add -b <branch>` |
| `remove_worktree(path, force: bool) -> Result<(), DomainError>` | `git worktree remove` [`--force`]; UI never offers remove for `is_main` |

Implemented in `git_adapter` via the user’s `git`. Porcelain is preferred over gix for this cut (checked-out branch per worktree otherwise needs each tree opened). `test_support::FakeGitRepository` grows matching stubs.

Create/remove may be invoked on any open session for that repository (shared gitdir); prefer the active session. Results refresh the shared catalogue; successful create then opens a tab for the new path.

## Session model in `gui_app`

Amended per ADR 0003: the session is the unit of everything repository-scoped.

```text
WorktreeSession {
  path: String,
  adapter: SharedRepository,
  state: AppState,                 // status, commits, dag, branches, stashes, tags, loading
  selection: Option<Selection>,
  commit_message: String,
  marked_files: Vec<MarkedFile>,
  collapsed_dirs, collapsed_sections,
  patches, patch_error, patch_pending, patch_generation,
  focused_diff_path, diff_lines_limit,
  // WP2 later: compare: Option<Comparison>, compare_generation
}

GitApp {
  sessions: Vec<WorktreeSession>,
  active: usize,
  worktree_catalogue: Vec<Worktree>,
  catalogue_error: Option<String>,
  // Global (not per session):
  settings: AppSettings,           // fanned out into every session's AppState via apply_settings
  panes, layout, ratios, files_rail_open,
  branch_list_open, branch_query,
  error, error_expanded,
  prompts (branch / history / confirm / worktree),
  settings_open, terminal, agent fields,
  open_repository: RepositoryOpener,   // remembers last_repo_path
  open_worktree: WorktreeOpener,       // opens without remember
  …
}
```

- Welcome screen: `sessions.is_empty()` (replaces `adapter.is_none()`).
- Opening a repository (welcome, Open…, clone, init, CLI) replaces `sessions` with **one** session and resets the catalogue.
- `GitApp::session()` / `session_mut()` are the accessors; tests that used `app.state.*` become `app.session().state.*`.

### Openers (composition root)

| Alias | Behaviour |
|---|---|
| `RepositoryOpener` | Existing: construct adapter **and** `remember(path)` into `last_repo_path`. Used for Open… / clone / init / CLI / startup. |
| `WorktreeOpener` | New: construct adapter for a path **without** `remember`. Used when opening a linked worktree into a tab. |

Both are `Arc<dyn Fn(&str) -> Result<SharedRepository, DomainError> + Send + Sync>` (or equivalent). `gui_app` still does not name `GitoxideAdapter`.

Verification: after opening a linked worktree as a tab, `last_repo_path` remains the main path that was remembered on the original Open.

### Tab retention (`AppSettings`)

```text
WorktreeTabRetention {
  Full,       // keep message, marks, selection
  Partial,    // keep message + marks; clear selection on deactivate (DEFAULT)
  PathOnly,   // clear message, marks, selection on deactivate
}
```

Stored on `AppSettings`, edited in Settings, persisted with the existing saver.

`fn apply_retention(&mut WorktreeSession, WorktreeTabRetention)` is pure and unit-tested without iced.

- **Switching** away from a tab: `apply_retention` on the leaving session; activate the other (no port call; other session’s `commits` untouched).
- **Closing** a tab: destroy the session; bag discarded. Re-open starts fresh. (Closing is not hibernate.)

### Close / remove interaction

- Tab `×`: refuse if last tab or if that session `state.loading`; else drop session; if it was active, activate another (prefer main worktree’s session if open, else neighbour).
- Remove worktree (git): after confirm success, drop any session whose path matches, refresh catalogue, activate a remaining session. Never offered for `is_main`.

## Finish routing (concurrency)

Background jobs must not assume `sessions[active]`.

Target shape (ADR 0003):

```text
Message::Finished { path: String, outcome: Outcome }

Outcome {
  Checkout(Result), Fetch(Result), Push(Result), Pull(Result),
  Edit(WorktreeEdit, Result), Discard(Result), Hunk(Result),
  Stash(Result), Branch(Result), History(Result),
  Loaded(Vec<ViewEvent>),
  Patch(generation, …),
  WorktreeList(Result<Vec<Worktree>>),
  WorktreeMutate(Result),  // add/remove
}
```

`update` resolves `session_mut(&path)`; a missing session drops the outcome (no panic, no state change). Modal prompts stay global and are cleared on tab switch.

**Implementation note:** WP1-b may introduce `path` on each existing `*Finished` first and consolidate into one `Finished` enum in a follow-up commit, as long as every finish handler resolves by path and never by `active` alone. Fitness function: no `*Finished` handler reads `self.active` without a `path`.

## Create / remove UI

### New worktree…

Modal (same family as branch prompts):

- Path (text + folder browse).
- Branch: existing local branch, or create new (`-b`) with name + start tip (default current HEAD / active branch).
- Confirm → `add_worktree` on an open session → refresh catalogue → `WorktreeOpener` → new session/tab → focus.

Failures: status bar shows git’s message.

### Remove…

- From explorer only — never from tab `×`.
- Confirm: path, branch; Force remove checkbox → `--force`.
- Block remove of the main worktree in the UI before calling git.

## Errors

- Create/remove/open failures set `error` from `DomainError`; do not clear the catalogue on failure.
- Catalogue load failure sets `catalogue_error` without wiping open sessions.

## Testing

**Adapter (temp repo)**

- `worktrees()` returns main after init; after `add`, lists both.
- `add_worktree` creates path and checks out the branch.
- `remove_worktree` deletes a linked worktree; UI/tests do not remove main.
- Force remove succeeds when the linked tree is dirty.

**App / fake**

- Open second worktree → `sessions.len() == 2`; activating it leaves the first session’s `commits` untouched and issues no load for the first path.
- Finish for a closed / unknown path is dropped.
- Close non-last tab → session removed; close last tab refused; close while `loading` refused.
- Remove drops matching session and reselects.
- Retention round-trips through Settings save/load; `apply_retention` unit tests for Full / Partial / PathOnly.
- `last_repo_path` unchanged after a tab open via `WorktreeOpener`.
- Fake implements the three worktree methods.

## Implementation sketch (WP1 only)

Aligned with ADR 0003 action items 2–5:

1. **WP1-a** Domain + port + porcelain parser + fake + adapter tests.
2. **WP1-b** `WorktreeSession`, accessors, retention, settings, path-routed finishes.
3. **WP1-c** Tab strip + explorer Worktrees + create/remove UI + `WorktreeOpener` in `app`.
4. **WP1-d** WIP row per open session in the graph; rewrite `CONTEXT.md` worktrees paragraph.

Then stop; WP2–WP4 are separate plans under ADR 0003.

## Success criteria

From one clone, the user can create a linked worktree, open it in a second tab, stage and draft a commit message there, switch back to main without losing main’s marks (Partial) and **without** reloading main’s history, close the linked tab without deleting the worktree, then remove the worktree from the explorer with confirm. Launching the app still opens the main path last remembered via Open…, not a linked tab path.
