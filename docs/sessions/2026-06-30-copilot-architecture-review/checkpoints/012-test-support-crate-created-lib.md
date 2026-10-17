<overview>
Gregory Mifsud (principal technical architect) is evolving GitCthulhu from a Rust/iced proof-of-concept Git GUI into a production-grade application. The `improve-codebase-architecture` skill identified 7 architectural friction candidates (A–G); Candidate B (gui_app seam bypass) was selected and grilled to lock 5 design decisions. Implementation is now underway: the approach is a hexagonal inversion — splitting `gui_app/src/main.rs` into a pure library (`lib.rs`) that depends only on `core_domain`, and a thin binary compositor (`main.rs`) that wires up the concrete `GitoxideAdapter`. A new `test_support` crate provides `FakeGitRepository` for all adapter dev-dependencies.
</overview>

<history>
1. User invoked `improve-codebase-architecture` skill to evolve GitCthulhu from PoC to production-ready.
   - Full codebase exploration across all 8 workspace crates.
   - Critical discovery: `src/App.tsx` is an AI Studio web preview (NOT Tauri) — real app is the `gui_app/` iced GUI.
   - 7 architectural friction candidates (A–G) identified and documented.
   - HTML report written to `docs/architecture-review-20260630.html`, committed at `714bcff`.

2. User selected Candidate B (gui_app seam bypass) for grilling.
   - `grilling` skill loaded; all 5 design questions answered and locked (see Technical Details).
   - User approved implementation.
   - Branch renamed to `gmifsud-candidate-b-seam-fix`.

3. Implementation started.
   - All key source files read: `gui_app/src/main.rs` (226 lines), `core_domain/src/lib.rs` (246 lines), `gui_app/Cargo.toml`, root `Cargo.toml`, `config_adapter/src/lib.rs`.
   - `test_support/` crate created: `Cargo.toml` + `src/lib.rs` with `FakeGitRepository`.
   - Work stopped mid-step; remaining Cargo changes and `lib.rs`/`main.rs` split not yet written.
</history>

<work_done>
Files created:
- `test_support/Cargo.toml` — new workspace crate declaration, depends on `core_domain`
- `test_support/src/lib.rs` — `FakeGitRepository` implementing all 6 `GitRepository` methods with pre-configured `Result<T, DomainError>` fields; `FakeGitRepository::ok()` constructor for all-success baseline

Files committed previously (unchanged):
- `docs/architecture-review-20260630.html` — committed at `714bcff`

Work completed:
- [x] Loaded `improve-codebase-architecture` skill
- [x] Full codebase exploration, 7 candidates identified
- [x] HTML report written and committed at `714bcff`
- [x] Grilling skill loaded; Q1–Q5 answered and locked
- [x] User approved implementation
- [x] Branch renamed to `gmifsud-candidate-b-seam-fix`
- [x] All key files read in full; implementation plan fully formed
- [x] `test_support/Cargo.toml` created
- [x] `test_support/src/lib.rs` created with `FakeGitRepository`
- [ ] **IN PROGRESS: Update root `Cargo.toml`** — add `"test_support"` to workspace members
- [ ] **PENDING: Update `gui_app/Cargo.toml`** — add `[lib]`, `[[bin]]`, `test_support` dev-dep
- [ ] **PENDING: Create `gui_app/src/lib.rs`** — pure library, zero `git_adapter` import
- [ ] **PENDING: Rewrite `gui_app/src/main.rs`** — thin `[[bin]]` compositor
- [ ] **PENDING: Drive-by fix `config_adapter/src/lib.rs`** — `fn From` → `fn from` (lines 23, 32)
- [ ] **PENDING: `cargo build` verification**
- [ ] **PENDING: Commit**
</work_done>

<technical_details>
**Locked design decisions (Q1–Q5):**
1. `GitApp.adapter` becomes `Option<Box<dyn GitRepository + Send>>` — `None` = welcome screen, `Some` = repo view.
2. `gui_app` lib's `src/lib.rs` has zero `git_adapter` import at the source level. `[dependencies]` still lists it (Cargo can't do per-target lib/bin deps without feature flags in one crate) — isolation is at the source level.
3. `gui_app/src/main.rs` (`[[bin]]` compositor): reads CLI arg for repo path, constructs `GitoxideAdapter`, calls `gui_app::run(Box::new(adapter))`. Falls back to `gui_app::run_welcome()` if no path given.
4. Async deferred: all `update()` arms still return `Task::none()` — add `// TODO: [DEBT] Wire spawn_blocking` comments.
5. `test_support` crate: `FakeGitRepository` with pre-configured clone-based `Result` fields. All adapter crates use it via `[dev-dependencies]`.

**iced 0.13 API notes:**
- `.run()` requires `State: Default` — cannot inject initial state.
- To inject initial state: `.run_with(move || (state, Task::none()))` — does NOT require `Default`.
- `run_with` closure must be `Send + 'static` (iced uses tokio internally).
- Functional API: `iced::application("title", GitApp::update, GitApp::view)` — state type inferred from fn signatures.
- `GitApp: Send` confirmed: all fields are `Send` (`AppState`, `Option<Box<dyn GitRepository + Send>>`, `DesignTokens` which contains only `f32`-based `Color`).

**`GitRepository` trait (from `core_domain/src/lib.rs` lines 74–81) — IS object-safe:**
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

**`FakeGitRepository` field type safety:**
- `DomainError: Clone`, `RepoStatus: Clone`, `Commit: Clone`, `Branch: Clone` — all confirmed.
- `Result<T, DomainError>: Clone` when `T: Clone, E: Clone` ✓.
- `fetch`/`fetch_ssh` take `&dyn CredentialStore`/`&dyn SshManager` — fake ignores these args, returns pre-configured result.
- `FakeGitRepository: Send` because all `Result<T, DomainError>` fields are `Send`.

**`config_adapter/src/lib.rs` compile error:**
- Lines 23 and 32: `fn From(...)` (capital F) — should be `fn from(...)`. Breaks workspace build. Must fix as drive-by before `cargo build`.

**Other codebase issues (NOT in this PR):**
- `auth_adapter/src/lib.rs`: all logic commented out.
- Hardcoded branch buttons `"main"` and `"feature/iced-ui"` at `gui_app/src/main.rs` lines 162–163 — must replace with `self.state.branches` iteration in `lib.rs` as part of this PR.
- `core_domain` God module (Candidate C — future work): ALL port traits + domain models + `AppState` + `build_dag` + `ViewEvent` + `UserCommand` in one 246-line file.

**`gui_app/src/main.rs` current state:**
- Line 5: `use git_adapter::GitoxideAdapter;` — remove from lib target
- Lines 53–57: `GitApp` struct — `adapter: GitoxideAdapter` to change
- Lines 67–74: `Default` impl hardwires `GitoxideAdapter::new(".")` — replace with `run_with` injection
- Lines 78–96: ALL `update()` arms return `Task::none()` — adapter never called (safe, zero behaviour regression)
- Lines 162–163: hardcoded branch buttons — replace with dynamic `self.state.branches` iteration

**PowerShell ENAMETOOLONG workaround:** Passing multi-KB strings as inline args to the `powershell` tool fails. Use the `create` tool to write directly to target files.

**SQL todos:** IDs `workspace-cargo` and `gui-app-cargo` returned 0 rows on update — actual IDs in the session DB may differ. Use `SELECT * FROM todos` to verify before updating status.
</technical_details>

<important_files>
- `test_support/Cargo.toml`
  - New workspace crate (just created)
  - `[dependencies]` only: `core_domain = { path = "../core_domain" }`

- `test_support/src/lib.rs`
  - New file (just created)
  - `FakeGitRepository` struct with 6 `Result` fields implementing all `GitRepository` methods
  - `FakeGitRepository::ok()` constructor for all-success baseline

- `gui_app/src/main.rs` (226 lines, NOT YET CHANGED)
  - Central subject of Candidate B
  - Line 5: `use git_adapter::GitoxideAdapter;` — must NOT appear in `lib.rs`
  - Lines 53–57: `GitApp` struct with `adapter: GitoxideAdapter` — to become `Option<Box<dyn GitRepository + Send>>`
  - Lines 67–74: `Default` impl — to be removed; replaced by `GitApp::with_repo()` / `GitApp::welcome()`
  - Lines 78–96: all `update()` arms return `Task::none()` — add `// TODO: [DEBT]` comments
  - Lines 162–163: hardcoded branch names — replace with dynamic `self.state.branches` iteration
  - This file becomes the thin `[[bin]]` compositor after the split

- `gui_app/Cargo.toml` (NOT YET CHANGED)
  - Current: no `[lib]`/`[[bin]]` sections
  - Needs: `[lib]` (`name = "gui_app"`, `path = "src/lib.rs"`), `[[bin]]` (`name = "git_cthulhu"`, `path = "src/main.rs"`), `test_support` in `[dev-dependencies]`
  - `git_adapter` stays in `[dependencies]` (Cargo limitation; isolation enforced at source level)

- `Cargo.toml` (workspace root, NOT YET CHANGED)
  - Needs `"test_support"` added to `[workspace] members` (currently 8 members, needs 9th)

- `config_adapter/src/lib.rs` (NOT YET CHANGED)
  - Lines 23, 32: `fn From` (capital F) → must be `fn from` — compile error blocking `cargo build`
  - Drive-by fix required before build verification

- `core_domain/src/lib.rs` (246 lines, NO CHANGES NEEDED)
  - Defines `GitRepository` trait (lines 74–81) — the port `gui_app` lib depends on
  - `TransportState` (lines 106–109), `ViewEvent` (lines 162–173), `AppState` (lines 175–245)
  - `AppState.branches: Vec<Branch>` — used for dynamic sidebar rendering in `lib.rs`
</important_files>

<next_steps>
Remaining implementation work (all Candidate B):

**Immediate — complete the file writes:**
1. Update root `Cargo.toml`: add `"test_support"` to `[workspace] members`
2. Update `gui_app/Cargo.toml`: add `[lib]` section, `[[bin]]` section, `test_support` dev-dep
3. Create `gui_app/src/lib.rs`: move all content from `main.rs` except `fn main()` and `use git_adapter`; change `GitApp.adapter` to `Option<Box<dyn GitRepository + Send>>`; add `GitApp::with_repo()` / `GitApp::welcome()`; add `pub fn run()` / `pub fn run_welcome()` using `.run_with()`; add welcome screen early return in `view()`; replace hardcoded branch buttons with `self.state.branches` iteration; add `// TODO: [DEBT]` comments on all `Task::none()` arms
4. Rewrite `gui_app/src/main.rs` as thin compositor: import `git_adapter::GitoxideAdapter`, match on `std::env::args().nth(1)`, call `gui_app::run()` or `gui_app::run_welcome()`
5. Fix `config_adapter/src/lib.rs` lines 23 and 32: `fn From` → `fn from`

**Then verify and commit:**
6. `cargo build` from workspace root
7. Commit: `feat: invert gui_app seam — lib target has zero git_adapter dependency (Candidate B)`

**Post-implementation (after commit):**
- Run `domain-modeling` skill to name new terms
- Create `CONTEXT.md`
- Write ADR for Candidate B decisions
- Grill Candidate C (AppState God state machine in `core_domain/src/lib.rs`)

**Potential blockers:**
- `config_adapter` `fn From` error will block `cargo build` — fix is required.
- SQL todo IDs may not match `'workspace-cargo'`/`'gui-app-cargo'` — run `SELECT * FROM todos` first.
</next_steps>