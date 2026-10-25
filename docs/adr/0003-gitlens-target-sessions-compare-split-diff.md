# ADR 0003: Reaching the GitLens target — worktree sessions, compare, and the split diff

**Status:** Accepted (2026-10-25), with one implementation amendment on finish routing (see below)
**Date:** 2026-10-25
**Deciders:** grego
**Related:** ADR 0001 (window does not name the adapter), ADR 0002 (reducer), `docs/superpowers/specs/2026-10-25-worktree-tabs-design.md`, `docs/proposals/three-column-window.md`

## Context

The reference image is VS Code with GitLens: a commit graph with lanes, avatars, ref pills and several *Working Changes* rows; a **Comparing References** panel (base ↔ compare, ahead/behind, commits, files changed); a **multi-file side-by-side diff** ("Changes in … (7 files)") with per-file headers, folded context, line numbers on both sides and word-level highlights; a changed-files navigator; and editor tabs.

`dev` at `0e6e633` has shipped Phases 1–3 of the three-column proposal and part of Phase 4. The worktree-tabs spec is approved for planning and has **no code on `dev`**: there is no `Worktree` model, no `worktrees()` / `add_worktree()` / `remove_worktree()` on the port, no session vector, no tab strip (confirmed by grep across every crate). This ADR reviews that spec, maps the image onto what exists, and fixes the build order.

### Gap matrix — image surface versus `dev`

| Image surface | On `dev` today | Where | Gap |
|---|---|---|---|
| Lane graph, ref pills, hash / author / when columns | Done. `Branch.tip` landed (`baf6a67`), `refs_at` draws head / local / remote / tag pills, canvas rows cached | `view/history.rs`, `ref_pills.rs`, `commit_graph.rs`, `core_domain/graph_layout.rs` | Straight segments only; no avatar; no stash rows |
| *Working Changes* row | One WIP row for the active checkout when status is dirty | `view/history.rs:51-91` | Image shows one row **per worktree**; needs the session model below |
| Graph search / filter | Field is drawn but inert: `text_input(SEARCH_PLACEHOLDER, "")` has no `on_input` | `view/top_bar.rs:143-192` | `graph_query` state + `matches_commit` + row dimming |
| Comparing References | Nothing. Port has no range query; inspector has Working-tree and Commit modes only | `view/workspace.rs::detail_body` | Port methods, compare state, inspector mode |
| Files changed with `+n −m` | Done for a commit, derived from loaded hunks via `patch_stats`; click narrows the diff (`FocusDiffPath`) | `view/workspace.rs:240-359`, `view/patch.rs:161` | No A/M/D/R letter; stats need every patch loaded |
| Multi-file diff | Unified, stacked per file, both line numbers (`parse_hunk_header`, `number_hunk_lines`), 400-line page cap, hunk stage/discard | `view/patch.rs`, `app.rs:28` | No side-by-side, no word highlights, no "n hidden lines" bars, no per-file header with stats |
| Changed-files navigator | Inspector file list + `focused_diff_path` | `view/workspace.rs::commit_files_block` | Equivalent; no change needed |
| Editor tabs | None | — | **Not the same thing as the spec's tabs** — see Decision 1 |
| Worktrees | None | — | Whole spec outstanding |

### Spec review — `2026-10-25-worktree-tabs-design.md`

**Verdict: Needs Attention.** The approach (one adapter per tab, catalogue from porcelain, no second adapter type) is right and preserves ADR 0001. Four issues must be settled before planning, two of them correctness.

1. **Location:** spec §"Session model", `WorktreeSession` fields. **Issue:** the session omits `AppState`. Status, commits, DAG, branches, stashes, tags and `loading` are all *per repository view*; the spec instead says "refresh status/history for its path if stale" on activate, which means one shared `AppState` reloaded on every tab switch. **Impact:** every switch costs a full `load_repository` (five port calls, gix rev-walk) and shows the previous tab's graph until it lands — the "slow, fights retention" failure the spec itself rejected for path hopping. **Remediation:** `WorktreeSession` owns its `AppState`; `AppSettings` stays global on `GitApp` and is fanned out to every session's reducer by one `apply_settings` helper. Switching is then a pointer move. **Verification:** test that activating a second session leaves the first session's `commits` untouched and issues no port call.

2. **Location:** `app/src/main.rs:29-34`, `opener()`. **Issue:** the only `RepositoryOpener` calls `remember(store, path)` on every open, so opening a linked worktree into a tab overwrites `last_repo_path` with the linked path. **Impact:** next launch reopens the linked worktree, or the welcome screen once it has been removed. **Remediation:** the composition root supplies a second closure for worktree tabs that constructs `GitoxideAdapter::new(path)` without `remember`; `gui_app` gains a `WorktreeOpener` type alias beside `RepositoryOpener`. ADR 0001 is intact: still no adapter name in `gui_app`. **Verification:** `startup::launch_path` test with a saved main path after a tab open.

3. **Location:** spec §"Errors and concurrency", finish handlers. **Issue:** every `*Finished(Result<…>)` variant today assumes the single adapter (`app.rs:654-725`). The spec says "target that session by path" but does not say how. **Impact:** a checkout finishing on tab A after the user switched to tab B would clear B's `loading` and refresh B. **Remediation:** finish handlers resolve by **path**, never by `active` alone. Target shape: `Message::Finished { path: String, outcome: Outcome }` where `Outcome` covers the existing result payloads (`Checkout`, `Fetch`, `Push`, `Pull`, `Edit(WorktreeEdit)`, `Discard`, `Hunk`, `Stash`, `Branch`, `History`, `Loaded(Vec<ViewEvent>)`, `Patch(u64, …)`). `update` resolves `self.session_mut(&path)`; a missing session drops the outcome. Modal prompts (`branch_prompt`, `history_prompt`, `confirm_*`) stay global and are cleared on tab switch. **Amendment:** WP1-b may add `path` to each existing `*Finished` first and consolidate into one `Finished` enum in a follow-up, provided the fitness function holds (no finish handler reads `self.active` without a path). **Verification:** finish a job for a closed session and assert no panic and no state change.

4. **Location:** spec §"Window chrome / Tab strip". **Issue:** the image's tabs are *document* tabs (Commit Graph, Changes in …), the spec's tabs are *session* tabs. **Impact:** reading the image literally would add a second tab concept. **Remediation:** Decision 1 below.

Minor: the spec's `GitRepository::worktrees()` via `git worktree list --porcelain` is correct; gix 0.66 exposes `Repository::worktrees()` but the checked-out branch per worktree still needs each one opened, so porcelain is both simpler and consistent with the checkout rationale in `CONTEXT.md`.

## Decision

Four work packages, in this order. Each is a `GitRepository` method set, an adapter implementation, a fake field, and a `Message` + background job, exactly as ADR 0001's consequences describe. ADR 0002 holds: the reducer gains no scheduling, and compare / range results live in the session's UI bag beside `patches`, not in `AppState`.

### 1. One tab strip: worktree sessions, not documents

The window keeps one tab strip and it holds **sessions** (the spec). Compare becomes an **inspector mode**; the multi-file diff is the existing **Diff pane**. No document tabs.

`WorktreeSession` becomes the unit of everything repository-scoped. Fields that move out of `GitApp` into it:

```text
WorktreeSession {
  path, adapter,
  state: AppState,                       // status, commits, dag, branches, stashes, tags, loading
  selection, commit_message, marked_files,
  collapsed_dirs, collapsed_sections,
  patches, patch_error, patch_pending, patch_generation, focused_diff_path, diff_lines_limit,
  compare: Option<Comparison>, compare_generation,   // WP2
}
GitApp {
  sessions: Vec<WorktreeSession>, active: usize,
  worktree_catalogue: Vec<Worktree>, catalogue_error: Option<String>,
  settings: AppSettings,                 // global; fanned out to every session's AppState
  panes, layout, ratios, files_rail_open, branch_list_open, branch_query,
  error, error_expanded, prompts, settings_open, terminal, agent fields, openers
}
```

`adapter.is_none()` (welcome screen) becomes `sessions.is_empty()`. `RepositoryOpened` (`app.rs:873-893`), which today rebuilds the whole `GitApp`, instead replaces `sessions` with one session and resets the catalogue. Retention on deactivate is a pure `fn apply_retention(&mut WorktreeSession, WorktreeTabRetention)`, tested without iced.

Later increment, once sessions exist: the graph draws a WIP row for **each open session** whose checked-out branch tip is among the shown nodes, using that session's `current_status`. This is the image's several *Working Changes* rows and needs no port change (`Worktree.branch` + `Branch.tip` + per-session status).

### 2. Compare: two port methods, one inspector mode

```rust
pub struct FileChange { pub path: String, pub status: ChangeKind /* Added|Modified|Deleted|Renamed */, pub added: usize, pub removed: usize }
pub struct Comparison { pub base: String, pub head: String, pub ahead: usize, pub behind: usize, pub commits: Vec<Commit>, pub files: Vec<FileChange> }

fn compare(&self, base: &str, head: &str) -> Result<Comparison, DomainError>;
fn range_patch(&self, base: &str, head: &str) -> Result<Vec<Patch>, DomainError>;
```

Adapter: `git rev-list --left-right --count base...head` for ahead/behind; `git rev-list base..head` for ids, then each `Commit` built through the same gix object read `log()` uses, so author / timestamp parsing has one code path; `git diff --numstat --name-status -M base...head` (merge-base form, matching the image's semantics) for files; `git diff --unified=3 base...head` through the existing `parse_patches`. Ref names are validated with `git check-ref-format --branch`-equivalent rules before being passed, as `is_hex_revision` does for ids.

Window: `Comparison` is loaded with the `patch_generation` pattern (stale results dropped). Entry points: a `Compare` ribbon command on `Selection::Branch` / `Selection::Remote` when not current (`selection.rs::ribbon_commands`, tested) comparing the current branch against the selected one; and two `pick_list`s plus a swap button in the inspector's Compare mode. `PatchQuery` gains `Range { base, head }` so the Diff pane and the files list reuse `commit_files_block` and `FocusDiffPath` unchanged. Merge / Rebase buttons in Compare mode reuse `AskMerge` / `AskRebase`.

### 3. Split diff: pure pairing over the existing `Patch`

No port change. In `gui_app`:

- `pair_lines(&Hunk) -> Vec<PairedLine { left: Option<(u32, &DiffLine)>, right: Option<(u32, &DiffLine)> }>`: each run of `Removed` is zipped with the following run of `Added`, the shorter side padded. Context lines pair with themselves. Pure, tested on pure add, pure delete, equal and unequal replace.
- `word_diff(left: &str, right: &str) -> (Vec<Span>, Vec<Span>)`: token LCS (Hunt–Szymanski is unnecessary; plain O(n·m) DP over tokens, capped at 200 tokens a side, above which the line gets no intra-line marks). Rendered with iced 0.13 `rich_text` / `span`.
- Hidden-line bars: the gap between consecutive hunk ranges from `parse_hunk_header` (exists) and the lead-in from `old_start − 1`.
- Per-file header: path, `+n −m` from `patch_stats` (exists), `ChangeKind` letter once WP2's `FileChange` exists; before that, derived (all-added → A, all-removed → D, else M).
- `diff_split: bool` on `AppSettings`, persisted through `SettingsSaver`, toggled in the Diff head.

Widget count doubles in split mode, so `DIFF_LINES_PER_PAGE` budgets **pairs**, not lines. Both columns live in one `row` inside one vertical `scrollable`, so they cannot drift; long lines stay `Wrapping::None` and clip, as today.

### 4. Graph polish

Wire the top-bar search to `graph_query` with a pure `matches_commit(&Commit, &str)`; non-matching rows are dimmed, not removed, so lanes stay continuous. Avatar: 16 px initial-in-a-circle from `Commit.author` (no e-mail, no network). Stash rows in the graph are deferred: `Stash` has no commit id; `git stash list --format=%H %gd %s` can supply one later.

## Options Considered

### Session state (WP1)

| Option | Complexity | Switch cost | Determinism | Verdict |
|---|---|---|---|---|
| **A. Session owns `AppState` + UI bag** | Medium (field move, `session()` accessor) | O(1) | One source of truth per tab | **Chosen** |
| B. Shared `AppState`, reload on activate (spec literal) | Low | Full `load_repository` | Stale frame between switch and load | Rejected |
| C. One OS window per worktree | Low | n/a | Fine | Rejected by spec (not tabbed) |

### Compare data (WP2)

| Option | Complexity | Correctness | Notes | Verdict |
|---|---|---|---|---|
| **A. Port methods; ids via `git rev-list`, objects via gix** | Medium | Exact | Mirrors `commit_patch` precedent (`git show` + `parse_patches`) | **Chosen** |
| B. Pure gix rev-walk with hidden tips | Medium | Exact | gix 0.66 hiding API not verified in this review — below the 95 % bar, revisit when gix is bumped | Deferred |
| C. Client-side from the loaded 50 commits | Low | **Wrong** | Truncated log gives wrong ahead/behind and misses files | Rejected |

### Side-by-side diff (WP3)

| Option | Complexity | Port change | Verdict |
|---|---|---|---|
| **A. Pure pairing in `gui_app` over `Patch`** | Low | None | **Chosen** |
| B. Adapter returns a paired structure | Medium | Yes | Rejected: view layout leaking into the port |
| C. Two `text_editor`s | Low | None | Rejected: no per-line tint or spans |

## Trade-off Analysis

- **Sessions first, compare second.** Compare and the split diff are independent of sessions in data terms, but every field they add (`compare`, `diff_split` pairing state) must land *inside* `WorktreeSession`. Doing WP2/WP3 before WP1 means moving them twice. WP1 is therefore the gate even though WP3 is the most visible win.
- **Porcelain over gix for new reads.** Consistent with checkout / stash / diff precedent; costs a process spawn per call and requires `git` on `PATH` (already true). Pure-gix parity is a later migration, not a blocker.
- **Pairing in the window.** Keeps `core_domain` free of presentation and the fake unchanged, at the cost of recomputing pairs each `view`. Pairs are O(lines); the existing 400-line page cap bounds it.
- **No document tabs.** Loses the image's "Changes in …" tab affordance; gains one navigation model. The Diff head title (`Working tree · 4 files`, `11f6db4 · 2 files`, `main…feature · 7 files`) carries the same information.
- **Avatars without e-mail.** No `Commit.email`, no gravatar, no network. An initial in a circle is deterministic and offline.

## Consequences

- Easier: a second worktree is a tab; compare and range diffs reuse the file list and focus mechanics already on `dev`; every new surface is unit-testable with `FakeGitRepository` and pure functions.
- Harder: `GitApp` tests that reach `app.state.*` directly (`app.rs` tests module) become `app.session().state.*`; a one-off mechanical edit. The `Message` enum shrinks by consolidating `*Finished` variants but every job callsite changes once.
- Revisit: gix-native compare when gix exposes a stable hidden-tip walk; stash rows in the graph; per-file sticky headers if iced gains sticky scroll children; syntax colouring (iced's `highlighter` is `text_editor`-only, as the proposal already noted).

## Action Items

Each step compiles and passes `cargo test` alone. Order is a dependency order, not a preference.

1. [x] **Spec amendments** — worktree spec updated 2026-10-25 with the four review items (session owns `AppState`; `WorktreeOpener` without `remember`; path-routed finishes; tab strip is sessions only).
2. [x] **WP1-a Domain + port** — `Worktree` model; `worktrees` / `add_worktree` / `remove_worktree` on `GitRepository`; porcelain parser with tests; fake fields; adapter temp-repo tests (list after init, add, remove, force remove of a dirty tree).
3. [x] **WP1-b Sessions** — `WorktreeSession`, `GitApp::session()/session_mut()`, `apply_retention`, `WorktreeTabRetention` on `AppSettings` + Settings UI, path-routed finishes (incremental or consolidated `Finished`). Tests: switch leaves the other session untouched; finish for a closed session is dropped; retention round-trips through save/load.
4. [x] **WP1-c Chrome** — tab strip under the top bar; explorer Worktrees section in the Files column; `+` picker; create modal; remove confirm with force checkbox; composition-root `WorktreeOpener`. Test: `last_repo_path` unchanged after a tab open.
5. [x] **WP1-d** — WIP row per open session in the graph; `CONTEXT.md` rewrite of "Later: several repositories and worktrees".
6. [ ] **WP2 Compare** — `FileChange`, `Comparison`, `compare`, `range_patch`; adapter + fake + temp-repo tests (ahead/behind, rename detection, empty range); `PatchQuery::Range`; `Compare` ribbon command; inspector Compare mode.
7. [ ] **WP3 Split diff** — `pair_lines`, `word_diff`, hidden-line bars, per-file header, `diff_split` setting. Tests on the pure functions; pair-budget test.
8. [ ] **WP4 Graph polish** — `graph_query` wiring, `matches_commit`, avatar initial, optional curved lane joins.
9. [x] Record outcome: **Accepted** (with finish-routing amendment above); decision logged in `CLAUDE.md` / `AGENTS.md` §9.

## Verification

- `cargo test -p core_domain -p git_adapter -p gui_app -p app` after each step.
- Fitness functions: `gui_app` depends on `core_domain` and `iced` only (`gui_app/Cargo.toml`); `AppState::apply_event` has no new variants beyond data events; no `*Finished` handler reads `self.active` without a `path`.
- Manual: from one clone, create a linked worktree, open it as a tab, stage and draft a message there, switch back without losing marks (Partial), compare the two branches, read the split diff, close the tab, remove the worktree with confirm — the spec's success criterion plus WP2/WP3.
