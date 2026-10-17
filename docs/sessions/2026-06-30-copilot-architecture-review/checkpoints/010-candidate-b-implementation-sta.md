<overview>
Gregory Mifsud (principal technical architect) is evolving GitCthulhu from a Rust/iced proof-of-concept Git GUI into a production-grade application. The `improve-codebase-architecture` skill identified 7 architectural friction candidates (A–G); Candidate B (gui_app seam bypass — `gui_app` directly hardwires `GitoxideAdapter` violating hexagonal/ports-and-adapters boundaries) was selected, grilled through 5 design questions, and approved for implementation. Implementation has now begun: all key files have been read and the approach is fully planned.
</overview>

<history>
1. User invoked `improve-codebase-architecture` skill to evolve GitCthulhu from PoC to production-ready.
   - Session renamed "Architecture review"; branch renamed `gmifsud-architecture-review` (later renamed `gmifsud-candidate-b-seam-fix`).
   - Full codebase exploration across all 8 crates.
   - Critical discovery: `src/App.tsx` is an AI Studio web preview (NOT Tauri) — real app is `gui_app/` iced GUI.
   - 7 architectural friction candidates (A–G) identified.
   - HTML report written to `docs/architecture-review-20260630.html`, committed at `714bcff`.

2. User selected Candidate B (gui_app seam bypass) for grilling.
   - `grilling` skill loaded; all 5 questions answered and locked.
   - Q1: `GitApp` holds `Option<Box<dyn GitRepository>>` (`None` → welcome screen, `Some` → repo view).
   - Q2: `gui_app` lib drops `git_adapter` dep at the code level; only the `[[bin]]` target imports it.
   - Q3: Split into `src/lib.rs` (pure library) + `src/main.rs` (`[[bin]]` compositor that constructs `GitoxideAdapter` and calls `gui_app::run()`).
   - Q4: Sync `GitRepository` trait (no `async_trait`); blocking calls wrapped with `tokio::task::spawn_blocking` inside `iced::Task::perform`.
   - Q5: New `test_support` crate with `FakeGitRepository` — reusable by all adapter crates as `[dev-dependency]`.

3. User approved implementation ("Yes").
   - Branch renamed to `gmifsud-candidate-b-seam-fix`.
   - All key source files read: `gui_app/src/main.rs` (226 lines), `gui_app/Cargo.toml`, root `Cargo.toml`, `core_domain/src/lib.rs` (246 lines), `git_adapter/src/lib.rs` (135 lines).
   - Implementation plan fully formed; no code written yet.
</history>

<work_done>
Files created/committed (pre-existing):
- `docs/architecture-review-20260630.html` — committed at `714bcff`. 7 candidates, Mermaid diagrams, before/after visuals. No changes since.

Work completed:
- [x] Loaded `improve-codebase-architecture` skill
- [x] Full codebase exploration, 7 candidates identified
- [x] HTML report written and committed
- [x] Grilling skill loaded; Q1–Q5 answered and locked
- [x] User approved implementation
- [x] Branch renamed to `gmifsud-candidate-b-seam-fix`
- [x] All key files read; implementation plan fully formed
- [ ] **IN PROGRESS: Create `test_support/` crate** — not yet written
- [ ] **PENDING: Split `gui_app/src/main.rs` → `lib.rs` + `main.rs`** — not yet written
- [ ] **PENDING: Update `gui_app/Cargo.toml`** — not yet written
- [ ] **PENDING: Update root `Cargo.toml`** to add `test_support` member

Current state: All planning complete; zero implementation files touched. Codebase is identical to `714bcff`.
</work_done>

<technical_details>
**Architecture being implemented — Candidate B:**
- Hexagonal/ports-and-adapters: `core_domain` defines `GitRepository` trait (the port); `git_adapter` implements it; `gui_app` lib depends only on `core_domain`.
- `gui_app/src/main.rs` currently hardwires `GitoxideAdapter` directly in the `GitApp` struct — a latent violation (adapter stored but never called; all `update()` arms return `Task::none()`).
- Fix is safe to land with zero behaviour regression because the adapter is never used.

**Key design decisions (locked):**
1. `GitApp.adapter` becomes `Option<Box<dyn GitRepository>>` — `None` = welcome screen, `Some` = repo view.
2. `gui_app/src/lib.rs` (library target): zero `git_adapter` import at the source level. `[dependencies]` in `Cargo.toml` can still list it (Cargo doesn't support per-target deps for lib vs bin in the same crate without feature flags) — the architectural isolation is at the code level.
3. `gui_app/src/main.rs` (`[[bin]]` compositor): reads config (TODO), constructs `GitoxideAdapter`, calls `gui_app::run(Box::new(adapter))`. Falls back to `gui_app::run_welcome()` if no saved repo.
4. Async pattern: sync `GitRepository` trait; `gui_app` wraps blocking calls with `tokio::task::spawn_blocking` inside `iced::Task::perform`. Deferred for this PR (still `Task::none()` stubs with `// TODO: [DEBT]` comments).
5. `test_support` crate: `FakeGitRepository` with pre-configured `Result<..>` fields (clone-based, not queue-based). All adapter crates use it via `[dev-dependencies]`.

**iced 0.13 API notes:**
- Current code uses `.run()` which requires `Default` on the state struct.
- To inject initial state, use `.run_with(|| (state, Task::none()))` — does NOT require `Default`.
- `GitApp` needs to be `'static` for iced's runtime. `Box<dyn GitRepository>` is `'static` by default.
- `Send` on state: iced 0.13 single-threaded GUI likely doesn't require `Send` on state itself; only `Task::perform` futures need `Send`. Plan: start without `+ Send`, add if compiler requires it.

**Cargo limitation:** Can't have per-target `[lib]`/`[[bin]]` dependencies within the same crate without feature flags. Workaround: `git_adapter` stays in `[dependencies]` but `lib.rs` simply never imports it. The isolation is enforced at the source level.

**Other codebase issues (not addressed in this PR):**
- `config_adapter/src/lib.rs` compile error: `fn From` (capital F) on lines 23 and 32 — should be `fn from`. Drive-by fix if it blocks compilation.
- `core_domain` God module (Candidate C — future grilling): ALL port traits + domain models + `AppState` + `build_dag` + `ViewEvent` + `UserCommand` in one file.
- `auth_adapter/src/lib.rs`: all logic commented out.
- Hardcoded branch names `"main"` and `"feature/iced-ui"` at `gui_app/src/main.rs` lines 162–163 must be removed; replace with `self.state.branches` iteration.

**Confirmed types in `core_domain`:**
- `GitRepository` trait: sync, 6 methods — `status`, `log`, `branches`, `checkout_branch`, `fetch`, `fetch_ssh`.
- `ViewEvent`: `LogUpdated(Vec<Commit>)`, `BranchesUpdated(Vec<Branch>)`, `StatusUpdated(RepoStatus)`, etc.
- `AppState.branches: Vec<Branch>` — available for dynamic sidebar rendering.
- `DomainError`, `Branch`, `Commit`, `RepoStatus` all derive `Clone` — safe to use in `FakeGitRepository`'s clone-based response fields.

**PowerShell ENAMETOOLONG workaround:** Passing multi-KB strings as inline arguments to the `powershell` tool fails. Use the `create` tool to write directly to the target file path.
</technical_details>

<important_files>
- `gui_app/src/main.rs`
  - Central subject of Candidate B — the file being split
  - Line 5: `use git_adapter::GitoxideAdapter;` — remove from lib target
  - Lines 53–57: `GitApp` struct — `adapter: GitoxideAdapter` → `Option<Box<dyn GitRepository>>`
  - Lines 67–74: `Default` impl hardwires `GitoxideAdapter::new(".")` — replace with `run_with` injection
  - Lines 78–96: ALL `update()` arms return `Task::none()` — adapter never called (safe, latent)
  - Lines 162–163: hardcoded `"main"` and `"feature/iced-ui"` branch buttons — remove, replace with `self.state.branches` iteration
  - **No changes yet**

- `gui_app/Cargo.toml`
  - Line 9: `git_adapter = { path = "../git_adapter" }` — stays in `[dependencies]` but lib.rs won't import it
  - Needs `[lib]` section (`path = "src/lib.rs"`) + `[[bin]]` target (`name = "git_cthulhu"`, `path = "src/main.rs"`) added
  - Needs `test_support = { path = "../test_support" }` in `[dev-dependencies]`
  - **No changes yet**

- `Cargo.toml` (workspace root)
  - Needs `"test_support"` added to `[workspace] members`
  - **No changes yet**

- `core_domain/src/lib.rs`
  - Lines 74–81: `GitRepository` trait — the port that `gui_app` lib depends on
  - Lines 162–173: `ViewEvent` enum — events for updating `AppState`
  - Lines 175–245: `AppState` struct + `apply_event()` — state machine (Candidate C, future)
  - **No changes needed**

- `git_adapter/src/lib.rs`
  - `GitoxideAdapter::new(repo_path: &str)` — what the `[[bin]]` compositor constructs
  - Implements all 6 `GitRepository` methods (sync, stub implementations)
  - **No changes needed**

- `docs/architecture-review-20260630.html`
  - Committed at `714bcff`; full 7-candidate report with Mermaid diagrams
  - **No further changes needed**
</important_files>

<next_steps>
Remaining work for Candidate B implementation:

**Step 1 — Create `test_support/` crate:**
- `test_support/Cargo.toml`: `[package]` + `core_domain = { path = "../core_domain" }` dependency
- `test_support/src/lib.rs`: `FakeGitRepository` struct with pre-configured `Result<..>` fields (all `Clone`-able); implement all 6 `GitRepository` methods returning the pre-configured values
- Add `"test_support"` to workspace `members` in root `Cargo.toml`

**Step 2 — Create `gui_app/src/lib.rs`:**
- Move all of current `main.rs` content here EXCEPT `fn main()`
- Change `GitApp.adapter` to `Option<Box<dyn GitRepository>>`
- Remove `Default` impl; add named constructors `GitApp::with_repo(repo)` and `GitApp::welcome()`
- Remove `use git_adapter::GitoxideAdapter`
- Replace hardcoded branch buttons with `self.state.branches` iteration
- Add welcome screen branch in `view()` when `self.adapter.is_none()`
- Add `pub fn run(repo: Box<dyn GitRepository>) -> iced::Result` using `.run_with()`
- Add `pub fn run_welcome() -> iced::Result` using `.run_with()`
- All `update()` arms still return `Task::none()` — add `// TODO: [DEBT] Wire spawn_blocking` comments

**Step 3 — Rewrite `gui_app/src/main.rs` as `[[bin]]` compositor:**
```rust
use git_adapter::GitoxideAdapter;
fn main() -> iced::Result {
    // TODO: Wire config_adapter to restore last_repo_path
    match std::env::args().nth(1) {
        Some(path) => gui_app::run(Box::new(GitoxideAdapter::new(&path))),
        None => gui_app::run_welcome(),
    }
}
```

**Step 4 — Update `gui_app/Cargo.toml`:**
- Add `[lib]` section, `[[bin]]` target
- Add `test_support` to `[dev-dependencies]`

**Step 5 — Verify build:**
- `cargo build` from workspace root
- Drive-by fix `config_adapter` compile error (`fn From` → `fn from`) if it blocks compilation

**Step 6 — Commit and post-implementation:**
- Commit with message: `feat: invert gui_app seam — lib target has zero git_adapter dependency (Candidate B)`
- Run `domain-modeling` skill to name new terms
- Create `CONTEXT.md`
- Write ADR for Candidate B decisions
- Proceed to grill Candidate C (AppState God state machine)
</next_steps>