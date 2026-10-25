# Worktree tabs — design

Status: Approved for planning (2026-10-25)  
Related: ADR 0001 (window does not name the adapter), ADR 0002 (reducer), `CONTEXT.md` “Later: several repositories and worktrees”

## Problem

GitCthulhu opens one repository path at a time. Linked git worktrees are ordinary folders today: Open… replaces the session. There is no catalogue, no tab strip, and no in-app create/remove. Parallel feature work needs several checkouts of the same repo without juggling windows.

## Goals

- List every worktree for the open repository in the left explorer (Worktrees section).
- Open a worktree into a tab; close a tab without deleting the worktree.
- Default: one tab after Open/Clone (the path just opened).
- Full lifecycle in-app: create and remove (remove always confirms; force is explicit).
- Tab retention mode is configurable in Settings and persisted; default keeps commit message and file marks, clears graph/file selection when leaving a tab.
- Preserve ADR 0001: `gui_app` never names `git_adapter`; paths open through the existing `RepositoryOpener`.

## Non-goals (this cut)

- Tabs for unrelated repositories (other clones).
- Worktree lock management beyond showing locked and failing with git’s message.
- `git worktree prune` UI.
- Changing the integrated terminal into a worktree manager.

## Approach

**Multi-session tabs.** Each open tab owns its own `Arc<dyn GitRepository>`, path, and a small UI bag. The explorer always lists `git worktree list`. Tab close drops the session only. Create/remove run through new port methods and refresh the catalogue.

Rejected: single-adapter path hopping (slow; fights retention default); one OS window per worktree (not tabbed).

## Window chrome

### Tab strip

- Sits under the top bar, full width.
- One tab per open session. Label: folder basename; tooltip: full path and branch.
- Active tab is visually distinct (raised background + underline).
- `×` closes that tab (session only). Closing the last tab is refused; the window keeps one open session while a repository is loaded.
- `+` opens a picker of catalogue entries not already open (same data as the explorer list).

### Explorer — Worktrees section

- Lives in the left Changes column (with branch switcher and staging).
- Header: “Worktrees” plus count.
- Rows: basename, checked-out branch, locked marker when present; active session highlighted.
- Click closed entry → open as tab and focus. Click already-open entry → focus that tab.
- Actions: New worktree…, Remove… (not available on the main worktree).

### Active session

Everything below the tab strip (path chrome, graph, changes, diff, inspector, commit message, marks, loading, errors for git ops) binds to the active session’s adapter and UI bag.

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

Implemented in `git_adapter` via the user’s `git` (same rationale as checkout). `test_support::FakeGitRepository` and any in-test fakes grow matching stubs.

Create/remove may be invoked on any open session for that repository (shared gitdir); prefer the active session. Results refresh the shared catalogue; successful create then opens a tab for the new path.

## Session model in `gui_app`

```text
WorktreeSession {
  path: String,
  adapter: SharedRepository,
  commit_message: String,
  marked_files: Vec<MarkedFile>,
  selection: Option<Selection>,   // cleared or kept per retention
  // other per-tab UI that must not leak across tabs (scroll/focus as needed)
}

GitApp {
  sessions: Vec<WorktreeSession>,
  active: usize,
  worktree_catalogue: Vec<Worktree>,
  // existing fields remain; path/adapter/commit_message/marks/selection
  // become views of sessions[active] or are synced on activate
}
```

Opening a repository (welcome, Open…, clone, init, CLI) creates exactly one session and one tab.

`RepositoryOpener` from the composition root opens each worktree path. No second adapter type.

### Tab retention (`AppSettings`)

```text
WorktreeTabRetention {
  Full,       // keep message, marks, selection
  Partial,    // keep message + marks; clear selection on deactivate (DEFAULT)
  PathOnly,   // clear message, marks, selection on deactivate
}
```

Stored on `AppSettings`, edited in the Settings modal under a Worktrees (or equivalent) section, persisted with the existing settings saver.

On tab deactivate: apply retention to the leaving session. On activate: bind the window to that session and refresh status/history for its path if stale.

### Close / remove interaction

- Tab `×`: apply retention, remove session; if it was active, activate another open session (prefer main worktree’s session if open, else neighbour).
- Remove worktree (git): after confirm success, drop any session whose path matches, refresh catalogue, activate a remaining session. Never offered for `is_main`.

## Create / remove UI

### New worktree…

Modal (same family as branch prompts):

- Path (text + folder browse under a chosen parent).
- Branch: existing local branch, or create new (`-b`) with name + start tip (default current HEAD / active branch).
- Confirm → `add_worktree` → refresh catalogue → open tab → focus.

Failures: status bar shows git’s message (path exists, branch already checked out in another worktree, etc.).

### Remove…

- From explorer row action / context only — never from tab `×`.
- Confirm: path, branch; if the tree may be dirty or locked, show a Force remove checkbox mapped to `--force`.
- Block remove of the main worktree in the UI before calling git.

## Errors and concurrency

- Create/remove/open failures set `error` from `DomainError` text; do not clear the catalogue on failure.
- A session with `loading == true` cannot be closed until the op finishes (same gate as checkout).
- Switching tabs does not cancel the background job on the previous session; its finish handler must target that session by path (ignore or apply only if the session still exists).

## Testing

**Adapter (temp repo)**

- `worktrees()` returns main after init; after `add`, lists both.
- `add_worktree` creates path and checks out the branch.
- `remove_worktree` deletes a linked worktree; removing main fails or is not attempted.
- Force remove succeeds when the linked tree is dirty.

**App / fake**

- Open second worktree → `sessions.len() == 2`, active points at new path.
- Close non-last tab → session removed; Partial retention restores message/marks when reopened only if we reopen (close drops session — retention applies when *switching* tabs, not when destroying; see clarification below).
- Close last tab refused.
- Remove drops matching session and reselects.
- Retention setting round-trips through Settings save/load.
- Fake implements the three methods.

### Retention clarification

- **Switching** away from a tab applies retention to that session (Full / Partial / PathOnly) while the session remains in `sessions`.
- **Closing** a tab destroys the session; message/marks for that path are discarded. Re-open starts a fresh bag. (Closing is not “hibernate”.)

## Implementation sketch (for the plan)

1. Domain model + port + fake + adapter porcelain parse + tests.
2. `WorktreeSession` / active swap / retention enum on settings + settings UI.
3. Tab strip + explorer Worktrees section (list/open/focus/close).
4. Create modal + remove confirm + jobs wiring.
5. Finish-handler path targeting; CONTEXT.md update.

## Success criteria

- From one clone, the user can create a linked worktree, open it in a second tab, stage and draft a commit message there, switch back to main without losing main’s marks (Partial), close the linked tab without deleting the worktree, then remove the worktree from the explorer with confirm.
