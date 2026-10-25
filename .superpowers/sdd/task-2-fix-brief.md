### Task 2 fix: Path-routed finishes must not act on active without path

Review verdict: **Needs fixes**. Fix Critical + Important items 1–4 before WP1-c.

Work from: `d:\Repos\GitCthulhu` on branch `feat/worktree-sessions-wp1` (HEAD `5e619a0`).

Do **not** start Task 3. Do not add tab strip / explorer / WorktreeOpener.

## Required fixes

### 1. Critical — `BranchFinished` / `HistoryFinished` (fitness)

In `gui_app/src/app.rs`:
- Early-return `Task::none()` when `session_mut_by_path(&path)` is `None` (same as `finish_reload`). Do not clear loading on a miss then fall through.
- On success, do **not** drive checkout/refresh/error against the active session blindly.
- Scope `pending_checkout` to the finishing session: store it on `WorktreeSession` (or clear in `activate` **and** only consume when finishing path == session that owns the pending name). Prefer session-scoped `pending_checkout`.
- Checkout continuation must be path-aware: only call `CheckoutBranch` / refresh for the session that finished (e.g. `begin_refresh_for(&path)`).

### 2. Important — Success continuation path-routed

- Replace bare `Task::done(Message::RefreshRepository)` from finish handlers with a path-carrying refresh.
- Add `begin_refresh_for(path: &str, clear_error: bool)` (or `RefreshRepository(Option<String>)` / `RefreshRepository { path }`) that reloads **that** session, not `active`.
- Keep UI-triggered refresh (F5 / toolbar) on the active session via the existing pathless entry if desired.
- `WorktreeEditFinished` success path must use the same path-aware refresh.

### 3. Important — Multi-session routing tests

Add tests with **two** sessions where finish targets session 1 while `active == 0`:
- `finish_reload` / `CheckoutFinished`: session 1 loading cleared; session 0 untouched; load job targets session 1's path.
- `WorktreeEditFinished` Ok commit: message cleared on session 1 only.
- `RepositoryLoaded` / `PatchLoaded`: events/patches land on the finishing path's session.
- Unknown-path `BranchFinished` / `HistoryFinished`: no checkout, no error, no refresh, no pending consume.

### 4. Important — `CommitSuggested`

- Carry session `path` in `CommitSuggested` (capture at spawn).
- Route write of `commit_message` by path; drop if unknown.

## Out of scope (defer Minor)

- Stale `patches` on activate (WP1-c).
- `OpenRepository` vs in-flight jobs.
- `session()` vs `session_mut()` in tests.
- Doc comment / import order nits.

## Verify

```bash
cargo test -p gui_app -p config_adapter -p core_domain
cargo clippy -p gui_app -- -D warnings
```

## Commit + report

Commit with a focused message (fix path-routed Branch/History/refresh/CommitSuggested).
Write `d:\Repos\GitCthulhu\.superpowers\sdd\task-2-fix-report.md` with Status, What changed, Tests, Concerns.
