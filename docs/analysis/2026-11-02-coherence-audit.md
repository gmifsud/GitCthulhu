# Coherence audit — 2 November 2026

Scope: the ten-point coherence brief (repository open, job routing, inspect vs activate, context
retention, worktree labels, tracking counts, selectable diff, centred header group, real terminal,
terminal settings). Audited on `main` at `b3166c4` (clean tree; the activity log is already merged).

## Verified architecture

- **Crates.** `core_domain` (models, `GitRepository` / `ConfigStore` / `ActivityStore` ports, pure
  `AppState` reducer), `git_adapter` (gix reads + `git` CLI writes), `config_adapter` (confy TOML
  settings, JSON-lines activity), `gui_app` (iced 0.13 window; depends on `core_domain`, `iced`, `rfd`
  only — ADR 0001), `app` (composition root: builds adapters and the opener closures), `test_support`
  (`FakeGitRepository`).
- **State.** `GitApp` (`gui_app/src/app.rs`, ~7 100 lines) owns `sessions: Vec<WorktreeSession>` and
  `active`. Each `WorktreeSession` (`session.rs`) owns its adapter, `AppState` and UI bag (ADR 0003).
  `AppState::apply_event` stays a pure reducer (ADR 0002); `GitApp::update` starts every job.
- **Jobs.** `jobs::in_background` runs a blocking port call on a `std::thread` and answers through a
  oneshot; if the thread dies, an `interrupted` fallback answers instead. Finishes come back as
  `Message::*Finished(path, …)` and are routed with `session_mut_by_path`.
- **Opening a repository.** `OpenRepository` → `RepositoryOpener` off-thread → `RepositoryOpened` →
  `*self = Self::new(…)` (terminal, activity log and settings carried by hand) → `RefreshRepository`.

## Findings

| # | Problem | Evidence | User impact | Proposed change | Regression coverage |
|---|---|---|---|---|---|
| F1 | A worker-thread panic aborts the whole app in release | `Cargo.toml` `[profile.release] panic = "abort"`; `jobs.rs::in_background` relies on the sender being dropped by unwinding | A gix/git panic while opening or loading closes the window with no message (debug builds survive with "interrupted") | Release `panic = "unwind"` so the existing thread boundary turns a worker panic into an error; panic hook in `app` writes a crash report. No blanket catching. | Existing `in_background` behaviour; crash-report formatting unit tests |
| F2 | Opening a folder discards the workspace before anything is validated | `GitoxideAdapter::new` stores the path only; `RepositoryOpened(Ok)` runs `*self = Self::new` before the first load | An invalid or broken folder wipes the open repository, its tabs and drafts, then shows an error | Opener validates (`discover`); the background open also runs the first load; the swap happens only on success; a failure keeps the workspace and shows Retry | `gui_app` tests: failure keeps sessions and drafts; success swaps; retry re-issues |
| F3 | A bad path is remembered before it is validated | `app/src/main.rs::opener` calls `remember` before building the adapter | Recent list and `last_repo_path` collect folders that never opened | `remember` only after `discover` succeeds | Composition-root behaviour (manual) |
| F4 | No loading state or diagnostics for open | `opening: bool` only disables buttons; no stage, timing or identity captured | "Nothing happens" while a large repository opens; a failure says only git's message | `OpenProgress { path, started }` shown in the header/welcome; failure carries diagnostics (version, stage, path, error, duration, last completed operation) and is logged to Activity | Unit tests on the diagnostics text (no secrets) |
| F5 | Results are routed by path only | `session_mut_by_path` (13 sites); every `*Finished(String, …)` | A late result from a closed tab or from the previous opening of the same repository lands in the new session (stale status/patch, cleared `loading`) | `SessionId` from a process-wide counter; jobs carry `SessionKey { id, path }`; lookups match the id | Tests: obsolete session's delayed result is dropped after reopen; close+reopen tab drops old results |
| F6 | Single-click on a WIP row or worktree activates it | `CONTEXT.md` "Pressing a row switches to its session"; worktree list → `OpenWorktree` | Inspecting another worktree moves the mutation target and the terminal | Inspect target separate from `active`; double-click / "Open in new tab" activates | Done 2026-11-04: `gui_app` tests (single click keeps `active`/target/terminal, double-click opens, clean tip, Quick open, filters kept) |
| F7 | A→B→A loses everything | `RepositoryOpened` replaces `sessions`; `Partial` retention clears selection | Drafts, marks, selection, filters and scroll are lost on every repository switch | Per-repository snapshot store keyed by repository root, reconciled on return | Not started |
| F8 | WIP pill is ambiguous | `view/history.rs::wip_label` → branch or folder name | A WIP row reads like a branch pill; duplicate folder names collide | "WIP · branch · folder" label with tooltip and active marker | Done 2026-11-04: `wip.rs` label tests; duplicate folders, unopened worktree rows, stale reads, removed worktree |
| F9 | Zero and unknown ahead/behind are the same | `core_domain::Branch.ahead/behind` "0 when unknown"; no fetch time | "Up to date" may mean "never computed" | Tracking state enum (Loading/NoUpstream/Known/Error) + last fetch time, shared by chip and toolbar | Not started |
| F10 | Diff text is not selectable | `view/patch.rs` draws one `text` widget per piece | Cannot copy from the diff | Document-position selection model over `diff_view` rows | Not started |
| F11 | Header group is not centred | `view/top_bar.rs` branch row: chip, tracking, then actions after `horizontal_space` | Actions shift with the branch name length | Three-slot row with equal flexible sides | Not started |
| F12 | Terminal is one process per command | `terminal.rs`, `jobs.rs::shell_stream`, `CONTEXT.md` "It is not a terminal emulator" | No completion, prompts, interactive apps, resize | ConPTY + VT parser behind a port (new dependencies — needs approval) | Not started |
| F13 | An API token lives in the settings TOML | `core_domain::AppSettings.agent_token` | A credential in a plain config file | Move to `keyring_adapter` (exists, unwired) when settings move behind the new store | Not started |

## Prioritised plan

1. **This slice:** F1–F5 (open safety, diagnostics, identity routing) plus adapter fixtures for empty,
   detached, missing upstream, linked worktree, removed worktree path, non-ASCII paths, init failure.
2. F6 + F8 (inspect vs activate, labels) — one UI model change, no port change.
3. F7 (per-repository snapshots) — depends on F5's identity and F6's inspect target.
4. F9 (tracking state) and F11 (header group).
5. F10 (diff selection).
6. F12 + terminal settings + settings store migration (new crates; needs a decision).

## Done in this slice (uncommitted on `main`)

- F1: release `panic = "unwind"`; `app/src/crash_report.rs` panic hook (chains the default hook, writes `%LOCALAPPDATA%\git_cthulhu\crashes\crash-<unix>.txt`, redacts URL credentials).
- F2/F4: `gui_app/src/opening.rs` (`OpenProgress`, `OpenFailure`, `open_and_read`, `redact`); `jobs::open_repository_in_background` now opens and reads before the swap; `GitApp.open_progress` / `open_failure`; `RetryOpen`, `DismissOpenFailure`, `CopyOpenDiagnostics`; `view/opening.rs` banner on the welcome screen and under the tabs. Worktree tabs keep their own `open_worktree_in_background`.
- F3: `app/src/main.rs` openers call `GitoxideAdapter::discover` and `remember` only on success.
- F5: `session::SessionId` / `SessionKey`; every session-scoped finish (`RepositoryLoaded`, all `*Finished`, `PatchLoaded`, `WorktreesLoaded`, `CommitSuggested`, `WorktreeCreated/Removed`, `RefreshSession`) carries a key; `GitApp::session_mut_for` matches the id.
- Fixtures (`git_adapter/src/tests.rs`, "Opening" section): unborn, detached, no upstream, linked worktree, removed worktree folder, non-ASCII path, init failure, plain folder.
- Tests: `gui_app` 318 → 328, `git_adapter` 73 → 81, `app` 17 → 18; workspace 447 passing.

## Continuation checklist

1. **Mandatory end-to-end scenario** — needs F6 and F7 first. Covered today: the "obsolete session delivers a delayed result" step (`delayed_results_from_an_obsolete_session_are_dropped_after_reopening_the_same_folder`).
2. [x] **F6 inspect vs activate** (done 2026-11-04: `GitApp.inspect: Option<wip::InspectTarget>`, `Message::WorktreeClicked` / `InspectWorktree` / `ClearInspect`, `view/inspect.rs`; probes in `GitApp.worktree_probes`, `jobs::probe_worktree_in_background`) — add `inspect: Option<InspectTarget { worktree_path, session: Option<SessionId> }>` beside `active`; single-click on a WIP row / worktree row sets it; inspector header names the inspected worktree; stage/commit actions hidden unless inspect == active; unopened worktree reads its status through a transient adapter built by `WorktreeOpener` (read-only), keyed by path + a generation. Double-click and "Open worktree in new tab" call `OpenWorktree`.
3. [x] **F8 labels** (done 2026-11-04: `wip::wip_label`, `wip::folder_labels`, `wip::wip_tooltip`; ref-pill hover lists full names via `ref_pills::pill_tooltip`) — `wip_label` → "WIP · branch · folder", folder disambiguated with parent segments when two worktrees share a basename; tooltip with path/branch/HEAD; mark the active one.
4. **F7 per-repository snapshots** — before `*self = Self::new` in `RepositoryOpened(Ok)`, store `RepositorySnapshot { sessions' UI bags (not adapters), active path, graph anchor = commit id, filters, diff mode, pane ratios }` keyed by repository root in a `HashMap` carried like the activity log; on reopen, restore after the first load and reconcile (drop missing commits/files/worktrees, never check out).
5. **F9 tracking** — `TrackingState { Loading, NoUpstream, Known{ahead,behind,upstream}, Error(String) }` + `last_fetch: Option<SystemTime>` (read `FETCH_HEAD` mtime in the adapter); one value feeds the branch chip and Pull ↓n / Push ↑n.
6. **F11 header** — three-slot branch row (`Fill`, centred group, `Fill`), disabled buttons with a tooltip reason, progress from `activity.running()`.
7. **F10 diff selection** — selection as `(row, side, char)` over `diff_view` rows; copy builds text from rows only (no gutters/fillers).
8. **F12 terminal + settings store** — needs a decision on new dependencies (ConPTY via `portable-pty` or `windows` crate, `vte`/`alacritty_terminal` parser) and on SQLite (`rusqlite` bundled) with a v1 migration that imports the confy TOML once and keeps it as a fallback. Move `agent_token` to `keyring_adapter` at the same time.

## Manual Windows verification

1. `cargo build --release -p app` (with the running app closed, or `CARGO_TARGET_DIR=target/verify`).
2. Open a repository, type a commit draft, then Open… a plain folder: the banner says "Could not open … while opening the repository", the draft and tabs are still there; Retry repeats; Copy diagnostics puts the report on the clipboard; Dismiss hides it.
3. Open a large repository: the banner counts seconds until the new workspace appears all at once.
4. Check `last_repo_path` / Recent did not gain the plain folder.
5. Open a folder under a non-ASCII path (e.g. `D:\tmp\répo-日本`).
