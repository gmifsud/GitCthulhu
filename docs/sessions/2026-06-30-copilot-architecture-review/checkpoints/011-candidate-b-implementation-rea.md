<overview>
Gregory Mifsud (principal technical architect) is evolving GitCthulhu from a Rust/iced proof-of-concept Git GUI into a production-grade application. The `improve-codebase-architecture` skill identified 7 architectural friction candidates (A–G); Candidate B (gui_app seam bypass — `gui_app` directly hardwires `GitoxideAdapter` violating hexagonal/ports-and-adapters boundaries) was selected, grilled through 5 design questions, approved, and implementation is now starting. The overall approach is a surgical hexagonal inversion: split `gui_app/src/main.rs` into a pure library (`lib.rs`) and a thin binary compositor (`main.rs`), with a new `test_support` crate providing `FakeGitRepository`.
</overview>

<history>
1. User invoked `improve-codebase-architecture` skill to evolve GitCthulhu from PoC to production-ready.
   - Session renamed "Architecture review"; branch renamed `gmifsud-architecture-review`.
   - Full codebase exploration across all 8 crates.
   - Critical discovery: `src/App.tsx` is an AI Studio web preview (NOT Tauri) — real app is `gui_app/` iced GUI.
   - 7 architectural friction candidates (A–G) identified.
   - HTML report written to `docs/architecture-review-20260630.html`, committed at `714bcff`.

2. User selected Candidate B (gui_app seam bypass) for grilling.
   - `grilling` skill loaded; all 5 questions answered and locked.
   - Q1: `GitApp` holds `Option<Box<dyn GitRepository>>` (`None` → welcome screen, `Some` → repo view).
   - Q2: `gui_app` lib drops `git_adapter` usage at the code level; the dep stays in `[dependencies]` (Cargo limitation — no per-target deps for lib vs bin in same crate without feature flags).
   - Q3: Split into `src/lib.rs` (pure library) + `src/main.rs` (`[[bin]]` compositor that constructs `GitoxideAdapter` and calls `gui_app::run()`).
   - Q4: Sync `GitRepository` trait (no `async_trait`); blocking calls wrapped with `tokio::task::spawn_blocking` inside `iced::Task::perform` — deferred to future PR (still `Task::none()` stubs).
   - Q5: New `test_support` crate with `FakeGitRepository` — reusable by all adapter crates as `[dev-dependency]`.

3. User approved implementation ("Yes").
   - Branch renamed to `gmifsud-candidate-b-seam-fix`.
   - All key source files read in full: `gui_app/src/main.rs` (226 lines), `core_domain/src/lib.rs` (246 lines).
   - SQL todo tracking table created with 5 tasks.
   - Implementation is starting — no files written yet.
</history>

<work_done>
Files created/committed (pre-existing):
- `docs/architecture-review-20260630.html` — committed at `714bcff`. No changes since.

Work completed:
- [x] Loaded `improve-codebase-architecture` skill
- [x] Full codebase exploration, 7 candidates identified
- [x] HTML report written and committed at `714bcff`
- [x] Grilling skill loaded; Q1–Q5 answered and locked
- [x] User approved implementation
- [x] Branch renamed to `gmifsud-candidate-b-seam-fix`
- [x] All key files read in full; implementation plan fully formed
- [x] SQL todo tracking set up (5 tasks, all `pending`)
- [ ] **NEXT: Create `test_support/` crate** — `Cargo.toml` + `src/lib.rs` with `FakeGitRepository`
- [ ] **PENDING: Create `gui_app/src/lib.rs`** — pure library, zero `git_adapter` import
- [ ] **PENDING: Rewrite `gui_app/src/main.rs`** as thin `[[bin]]` compositor
- [ ] **PENDING: Update `gui_app/Cargo.toml`** — add `[lib]`, `[[bin]]`, `test_support` dev-dep
- [ ] **PENDING: Update root `Cargo.toml`** — add `test_support` to workspace members
- [ ] **PENDING: `cargo build` verification**

Current state: Zero implementation files touched. Codebase identical to `714bcff`.
</work_done>

<technical_details>
**Architecture being implemented — Candidate B:**
- Hexagonal/ports-and-adapters: `core_domain` defines `GitRepository` trait (the port); `git_adapter` implements it; `gui_app` lib depends only on `core_domain`.
- `gui_app/src/main.rs` currently hardwires `GitoxideAdapter` directly in the `GitApp` struct — a latent violation (adapter stored but never called; all `update()` arms return `Task::none()`). Safe to land with zero behaviour regression.

**Key design decisions (locked):**
1. `GitApp.adapter` becomes `Option<Box<dyn GitRepository + Send>>` — `None` = welcome screen, `Some` = repo view.
2. `gui_app` lib's `src/lib.rs` has zero `git_adapter` import at the source level. `[dependencies]` still lists it (Cargo can't do per-target lib/bin deps without feature flags) — isolation is at the source/code level.
3. `gui_app/src/main.rs` (`[[bin]]` compositor): reads CLI arg for repo path, constructs `GitoxideAdapter`, calls `gui_app::run(Box::new(adapter))`. Falls back to `gui_app::run_welcome()` if no path given.
4. Async deferred: all `update()` arms still return `Task::none()` — add `// TODO: [DEBT] Wire spawn_blocking` comments.
5. `test_support` crate: `FakeGitRepository` with pre-configured `Result<..>` fields (clone-based). All adapter crates use it via `[dev-dependencies]`.

**iced 0.13 API notes:**
- Current code uses `.run()` which requires `State: Default`.
- To inject initial state: `.run_with(move || (state, Task::none()))` — does NOT require `Default`.
- `run_with` closure must be `Send + 'static` (iced uses tokio internally).
- `GitApp: Send` is achievable because all fields are `Send`: `AppState` (all `Vec<T: Send>`), `Option<Box<dyn GitRepository + Send>>`, `DesignTokens` (iced `Color` is `f32`-based, hence `Send`).
- Functional API: `iced::application("title", GitApp::update, GitApp::view)` — state type inferred from fn signatures. `GitApp::update: fn(&mut GitApp, Message) -> Task<Message>` ✓, `GitApp::view: fn(&GitApp) -> Element<Message>` ✓.

**Cargo limitation confirmed:**
- Can't restrict a dep to only the `[[bin]]` target within the same crate without feature flags.
- Workaround: `git_adapter` stays in `[dependencies]`; `lib.rs` simply never imports it.
- The architectural isolation is enforced at the source level, not the Cargo level.

**`FakeGitRepository` type safety:**
- `DomainError: Clone` ✓, `RepoStatus: Clone` ✓, `Commit: Clone` ✓, `Branch: Clone` ✓.
- `Result<T, DomainError>: Clone` when `T: Clone, E: Clone` ✓.
- Clone-based fields (not queue-based) suffice for test scenarios.

**Other codebase issues (not in this PR):**
- `config_adapter/src/lib.rs` compile error: `fn From` (capital F) on lines 23 and 32 — should be `fn from`. May need drive-by fix if it blocks `cargo build`.
- `core_domain` God module (Candidate C — future work): ALL port traits + domain models + `AppState` + `build_dag` + `ViewEvent` + `UserCommand` in one 246-line file.
- `auth_adapter/src/lib.rs`: all logic commented out.
- Hardcoded branch buttons `"main"` and `"feature/iced-ui"` at `gui_app/src/main.rs` lines 162–163 — must be replaced with `self.state.branches` iteration in `lib.rs`.

**PowerShell ENAMETOOLONG workaround:** Passing multi-KB strings as inline arguments to the `powershell` tool fails. Use the `create` tool to write directly to target file paths.

**`text(reason)` borrow issue:**
- In `view()`, `if let TransportState::Degraded(ref reason)` borrows from `self.state`. 
- Solution: `let reason = reason.clone();` before building the widget tree to free the borrow.

**`gui_app/src/lib.rs` visibility:**
- `GitApp` struct and `Message` enum can be private (no `pub`) — only `run()` and `run_welcome()` need `pub`.
- Binary target in same package automatically links the lib target and can call `gui_app::run()`.

**Confirmed `GitRepository` trait (from `core_domain/src/lib.rs` lines 74–81):**
```rust
pub trait GitRepository {
    fn status(&self) -> Result<RepoStatus, DomainError>;
    fn log(&self, max_count: Option<usize>) -> Result<Vec<Commit>, DomainError>;
    fn branches(&self) -> Result<Vec<Branch>, DomainError>;
    fn checkout_branch(&self, branch_name: &str) -> Result<(), DomainError>;
    fn fetch(&self, store: &dyn CredentialStore) -> Result<(), DomainError>;
    fn fetch_ssh(&self, ssh_manager: &dyn SshManager) -> Result<(), DomainError>;
}
```

**`FakeGitRepository` complication:** `fetch` and `fetch_ssh` take `&dyn CredentialStore` and `&dyn SshManager` respectively — these are additional ports. `FakeGitRepository` must implement these methods too, but can simply ignore the arguments and return the pre-configured `Result`.
</technical_details>

<important_files>
- `gui_app/src/main.rs` (226 lines)
  - Central subject of Candidate B — the file being split into lib + bin
  - Line 5: `use git_adapter::GitoxideAdapter;` — remove from lib target
  - Lines 53–57: `GitApp` struct — `adapter: GitoxideAdapter` → `Option<Box<dyn GitRepository + Send>>`
  - Lines 67–74: `Default` impl hardwires `GitoxideAdapter::new(".")` — replace with `run_with` injection
  - Lines 78–96: ALL `update()` arms return `Task::none()` — adapter never called (safe, latent violation)
  - Lines 162–163: hardcoded `"main"` and `"feature/iced-ui"` branch buttons — replace with `self.state.branches` iteration
  - **No changes yet**

- `gui_app/Cargo.toml`
  - Line 9: `git_adapter = { path = "../git_adapter" }` — stays in `[dependencies]`; lib.rs won't import it
  - Needs `[lib]` section (`name = "gui_app"`, `path = "src/lib.rs"`) added
  - Needs `[[bin]]` section (`name = "git_cthulhu"`, `path = "src/main.rs"`) added
  - Needs `test_support = { path = "../test_support" }` in `[dev-dependencies]`
  - **No changes yet**

- `Cargo.toml` (workspace root)
  - Needs `"test_support"` added to `[workspace] members`
  - **No changes yet**

- `core_domain/src/lib.rs` (246 lines)
  - Defines `GitRepository` trait (lines 74–81) — the port `gui_app` lib depends on
  - `TransportState` (lines 106–109), `ViewEvent` (lines 162–173), `AppState` (lines 175–245)
  - `AppState.branches: Vec<Branch>` — available for dynamic sidebar rendering
  - `DomainError`, `Branch`, `Commit`, `RepoStatus` all derive `Clone` — safe for `FakeGitRepository`
  - **No changes needed**

- `git_adapter/src/lib.rs` (135 lines)
  - `GitoxideAdapter::new(repo_path: &str)` — constructed in `[[bin]]` compositor
  - Implements all 6 `GitRepository` methods (sync stubs)
  - **No changes needed**

- `docs/architecture-review-20260630.html`
  - Committed at `714bcff`; full 7-candidate report with Mermaid diagrams
  - **No further changes needed**

- `test_support/` (does not exist yet)
  - New workspace crate to be created
  - Will contain `FakeGitRepository` implementing `GitRepository` with pre-configured `Result<T, DomainError>` fields
</important_files>

<next_steps>
Remaining implementation work (Candidate B):

**Step 1 — Create `test_support/` crate (NEXT):**
- `test_support/Cargo.toml`: `[package]` + `core_domain = { path = "../core_domain" }`
- `test_support/src/lib.rs`: `FakeGitRepository` struct with pre-configured `Result<..>` fields; implement all 6 `GitRepository` methods (ignore `CredentialStore`/`SshManager` args, return pre-configured value)

**Step 2 — Update root `Cargo.toml`:**
- Add `"test_support"` to `[workspace] members`

**Step 3 — Create `gui_app/src/lib.rs`:**
- Move all content from `main.rs` EXCEPT `fn main()` and `use git_adapter::GitoxideAdapter`
- `GitApp.adapter` → `Option<Box<dyn GitRepository + Send>>`
- Remove `Default` impl; add `GitApp::with_repo(repo: Box<dyn GitRepository + Send>)` and `GitApp::welcome()`
- Add `pub fn run(repo: Box<dyn GitRepository + Send>) -> iced::Result` using `.run_with()`
- Add `pub fn run_welcome() -> iced::Result` using `.run_with()`
- In `view()`: add welcome screen early return (`adapter.is_none()`), dynamic branch list from `self.state.branches`, clone `reason` before building degraded widget tree
- Add `Message::OpenRepository` variant with `// TODO` stub
- All `update()` arms still `Task::none()` — add `// TODO: [DEBT] Wire spawn_blocking` comments

**Step 4 — Rewrite `gui_app/src/main.rs` as `[[bin]]` compositor:**
```rust
use git_adapter::GitoxideAdapter;
fn main() -> iced::Result {
    // TODO: [DEBT] Read config_adapter to restore last_repo_path
    match std::env::args().nth(1) {
        Some(path) => gui_app::run(Box::new(GitoxideAdapter::new(&path))),
        None => gui_app::run_welcome(),
    }
}
```

**Step 5 — Update `gui_app/Cargo.toml`:**
- Add `[lib]` and `[[bin]]` sections
- Add `test_support` to `[dev-dependencies]`

**Step 6 — Verify build:**
- `cargo build` from workspace root
- Drive-by fix `config_adapter` compile error (`fn From` → `fn from`) if it blocks compilation

**Step 7 — Commit:**
- Message: `feat: invert gui_app seam — lib target has zero git_adapter dependency (Candidate B)`

**Post-implementation (after commit):**
- Run `domain-modeling` skill to name new terms
- Create `CONTEXT.md`
- Write ADR for Candidate B decisions
- Grill Candidate C (AppState God state machine in `core_domain/src/lib.rs`)
</next_steps>