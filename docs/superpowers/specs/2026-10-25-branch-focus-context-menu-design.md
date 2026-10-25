# Branch focus + context menu — design

Status: Approved 2026-10-25  
Related: ADR 0001, ADR 0002, ADR 0003, worktree sessions WP1

## Problem

The history graph shows the full loaded DAG (all tips in the page). Focusing on “just my branch” takes mental filtering. Opening a linked worktree from a branch or commit, or parking HEAD at a commit (detached), requires the ribbon or the create-worktree dialog with no contextual shortcut.

## Goals

- Toggle **Focus current** so the graph shows only commits reachable from the active session’s HEAD tip (client-side filter of the already-loaded DAG).
- Right-click on **graph commit rows** and **local branch** rows for:
  - **Open as worktree…** — open the existing Create Worktree modal prefilled (when available).
  - **Detach HEAD here** / **Detach at tip** — `git checkout --detach` in the active session (confirm if dirty).
- Keep ADR 0001 (`gui_app` never names `git_adapter`), path-routed finishes, and WorktreeOpener without `remember`.

## Non-goals

- Persist the focus toggle across restarts.
- Activate an existing worktree tab instead of create when the branch is already checked out elsewhere (item is disabled instead).
- Context menus on remotes, tags, or stashes.
- Native OS context menus.
- Changing `git log` / first-parent reload when focus is on.

## Approach

**Client filter + iced overlay menu + thin port extensions.**

Rejected: git-side `--first-parent` reload on toggle; ribbon-only without right-click; one-click create without a modal.

## Focus current branch

- Control: toggle chip in the history/graph chrome (“Focus current”).
- State: `focus_current_branch: bool` on each `WorktreeSession` (ephemeral; default off). Not cleared by Partial retention.
- Filter: from the active HEAD tip (current local branch `tip`, or detached HEAD commit), keep DAG nodes reachable by walking parents within the loaded commits. Apply `COMMIT_PAGE_SIZE` after filtering. Re-layout with existing `graph_rows`.
- WIP rows: while focused, show only the **active** session’s dirty WIP row.
- Toggle does not trigger a repository reload.
- Empty tip / empty DAG: empty graph (existing empty states).

## Context menu

- Targets: graph commit rows; local branch rows in the refs list.
- Secondary click opens one overlay (`context_menu`); selects the target where applicable.
- Close: Escape, click away, choosing an item, worktree tab switch.
- Items from a pure `context_actions(target, catalogue, active_head, loading)` helper (unit-tested).
- Does not duplicate the full selection ribbon.

| Target | Items |
|--------|--------|
| Commit | Open as worktree… · Detach HEAD here |
| Local branch | Open as worktree… · Detach at tip |

### Availability

- **Open as worktree…:** disabled when that **branch** is already checked out in `worktree_catalogue`. For a **commit** (no branch yet): always enabled.
- **Detach…:** disabled when the commit/tip is already the active session’s HEAD, or while `loading`.

## Open as worktree…

- Opens `WorktreePrompt::Create` prefilled:
  - Branch: `branch = name`, `create_branch = false`, `start_point = None`, path empty.
  - Commit: `branch = "wip/<short7>"` (editable), `create_branch = true`, `start_point = Some(commit_id)`, path empty.
- Port: `add_worktree` accepts an optional start tip; with `create_branch` + start → `git worktree add -b <branch> <path> <commit>`.
- Success: refresh catalogue + `OpenWorktree` (unchanged).

## Detach HEAD

- Port: `checkout_detached(commit_id)` → `git checkout --detach <id>` (same id/option guards as other checkout paths).
- Dirty working tree: confirm before running. Clean: run immediately.
- Finish: path-routed reload like branch checkout; no force-overwrite.

## Errors and copy

- British English. Git failures via existing status/error paths; create modal stays open on add failure.
- Detach blocked by git → surface message; do not force.

## Testing

- Reachability filter unit tests.
- `context_actions` enable/disable matrix.
- Prefill create from branch vs commit (including start_point).
- Adapter: detach checkout; worktree add `-b` at commit.
- App: detach dirty → confirm; clean → job; finish path routing.

## Out of scope

Activate-existing-tab; remote menus; persist focus; native menus; WP2 compare / WP3 split diff.
