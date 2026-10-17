<overview>
Gregory Mifsud (principal technical architect) is evolving GitCthulhu from a Rust/iced proof-of-concept Git GUI into a production-grade application. The `improve-codebase-architecture` skill identified 7 architectural friction candidates (A–G); Candidate B (gui_app seam bypass) was selected, grilled to lock 5 design decisions, and implementation is now ~80% complete. The approach is a hexagonal inversion: split `gui_app/src/main.rs` into a pure library (`lib.rs`) with zero `git_adapter` import, and a thin binary compositor (`main.rs`) that wires the concrete `GitoxideAdapter`. A new `test_support` crate provides `FakeGitRepository` for all adapter dev-dependencies.
</overview>

<history>
1. User invoked `improve-codebase-architecture` skill to evolve GitCthulhu from PoC to production-ready.
   - Full codebase exploration across all 8 workspace crates.
   - Critical discovery: `src/App.tsx` is an AI Studio web preview (NOT Tauri) — real app is the `gui_app/` iced GUI.
   - 7 architectural friction candidates (A–G) identified and documented.
   - HTML report written to `docs/architecture-review-20260630.html`, committed at `714bcff`.

2. User selected Candidate B (gui_app seam bypass) for grilling.
   - `grilling` skill loaded; all 5 design questions answered and locked.
   - User approved implementation.
   - Branch renamed from generated name to `gmifsud-candidate-b-seam-fix`.

3. Implementation Phase 1 — test_support crate (prior session).
   - `test_support/Cargo.toml` created.
   - `test_support/src/lib.rs` created with `FakeGitRepository` implementing all 6 `GitRepository` methods.
   - Work stopped mid-step before remaining Cargo/source changes.

4. Implementation Phase 2 — resuming in current session.
   - Root `Cargo.toml` updated: `"test_support"` added to workspace members.
   - `gui_app/Cargo.toml` updated: added `[lib]`, `[[bin]]`, `test_support` dev-dep.
   - `config_adapter/src/lib.rs` drive-by fix: `fn From` → `fn from` on lines 23 and 32.
   - Read `core_domain/src/lib.rs` in full to confirm `Branch.name`, `AppState.branches`, `TransportState` variants.
   - Read `gui_app/src/main.rs` in full (226 lines).
   - **`gui_app/src/lib.rs` created** — pure library with all GUI logic, zero `git_adapter` import, `Option<Box<dyn GitRepository + Send>>` adapter field, welcome screen, dynamic branch list, `run()`/`run_welcome()` public entry points.
   - `gui_app/src/main.rs` has NOT yet been rewritten as the thin compositor (still the original 226-line file).
</history>

<work_done>
Files created:
- `test_support/Cargo.toml` — new workspace crate; depends on `core_domain`
- `test_support/src/lib.rs` — `FakeGitRepository` with 6 pre-configured `Result` fields; `FakeGitRepository::ok()` all-success constructor
- `gui_app/src/lib.rs` — **NEW**: pure library target; all GUI logic moved here; zero `git_adapter` source import; `GitApp.adapter: Option<Box<dyn GitRepository + Send>>`; `GitApp::with_repo()` / `GitApp::welcome()` constructors; `pub fn run()` / `pub fn run_welcome()` using `run_with()`; welcome screen; dynamic branch list from `self.state.branches`; all `Task::none()` arms annotated with `// TODO: [DEBT]`

Files modified:
- `Cargo.toml` (root) — added `"test_support"` to `[workspace] members` (now 9 members)
- `gui_app/Cargo.toml` — added `[lib]` (`name = "gui_app"`, `path = "src/lib.rs"`), `[[bin]]` (`name = "git_cthulhu"`, `path = "src/main.rs"`), `test_support` in `[dev-dependencies]`
- `config_adapter/src/lib.rs` — fixed `fn From` → `fn from` at lines 23 and 32

Files NOT yet changed (still pending):
- `gui_app/src/main.rs` — still the original 226-line file; needs to become the thin compositor

Work completed:
- [x] Architecture review + HTML report committed (`714bcff`)
- [x] Grilling: 5 decisions locked
- [x] Branch renamed to `gmifsud-candidate-b-seam-fix`
- [x] `test_support/Cargo.toml` created
- [x] `test_support/src/lib.rs` created
- [x] Root `Cargo.toml` updated (test_support member)
- [x] `gui_app/Cargo.toml` updated (`[lib]`, `[[bin]]`, dev-dep)
- [x] `config_adapter/src/lib.rs` fixed (`fn From` → `fn from`)
- [x] `gui_app/src/lib.rs` created (pure library)
- [ ] **IN PROGRESS: Rewrite `gui_app/src/main.rs`** as thin compositor
- [ ] **PENDING: `cargo build` verification**
- [ ] **PENDING: Commit**
</work_done>

<technical_details>
**Locked design decisions (Q1–Q5):**
1. `GitApp.adapter` → `Option<Box<dyn GitRepository + Send>>` — `None` = welcome screen, `Some` = repo view.
2. `gui_app` lib's `src/lib.rs` has zero `git_adapter` import at the source level. `[dependencies]` still lists it (Cargo can't do per-target lib/bin deps without feature flags) — isolation is enforced at source level only.
3. `gui_app/src/main.rs` (`[[bin]]` compositor): reads `std::env::args().nth(1)` for repo path, constructs `GitoxideAdapter`, calls `gui_app::run(Box::new(adapter))`. Falls back to `gui_app::run_welcome()` if no path given.
4. Async deferred: all `update()` arms still return `Task::none()` — annotated with `// TODO: [DEBT] Wire spawn_blocking` comments.
5. `test_support` crate: `FakeGitRepository` with pre-configured clone-based `Result` fields. All adapter crates use it via `[dev-dependencies]`.

**iced 0.13 API notes:**
- `.run()` requires `State: Default` — cannot inject initial state.
- Injection pattern: `.run_with(move || (state, Task::none()))` — does NOT require `Default`.
- `run_with` closure must be `Send + 'static`.
- Functional API: `iced::application("title", GitApp::update, GitApp::view)` — state type inferred.
- `GitApp: Send` confirmed: all fields are `Send` (`AppState`, `Option<Box<dyn GitRepository + Send>>`, `DesignTokens` which is `Copy`).

**`GitRepository` trait (core_domain lines 74–81) — IS object-safe:**
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

**`Branch` struct (core_domain line 49–53):**
```rust
pub struct Branch { pub name: String, pub is_remote: bool, pub is_head: bool }
```

**`AppState` fields relevant to view (core_domain lines 176–186):**
- `transport_state: TransportState` — `Healthy` | `Degraded(String)`
- `commits: Vec<Commit>`, `dag_nodes: Vec<DagNode>`, `branches: Vec<Branch>`, `loading: bool`

**`config_adapter` bug fixed:** Lines 23, 32 had `fn From` (capital F) — invalid Rust; now `fn from`. This was blocking `cargo build`.

**`view()` lifetime pattern:** `branch.name.as_str()` borrows from `self.state.branches` (lifetime `'_` tied to `&self`) — safe. For `TransportState::Degraded(ref reason)`, use `reason.as_str()` — also safe without cloning.

**PowerShell ENAMETOOLONG workaround:** Multi-KB strings as inline powershell args fail — use the `create` tool to write files directly.

**`UserCommand` in `core_domain`:** Defined but unused in the existing `update()` — correctly omitted from `lib.rs` imports.

**`text_input` in original `main.rs`:** Imported but never used — correctly dropped in `lib.rs`.
</technical_details>

<important_files>
- `gui_app/src/lib.rs`
  - **Just created** — the entire Candidate B payload; pure GUI library with zero `git_adapter` source dependency
  - `GitApp.adapter: Option<Box<dyn GitRepository + Send>>`, `GitApp::with_repo()`, `GitApp::welcome()`
  - `pub fn run(repo: Box<dyn GitRepository + Send>) -> iced::Result` — calls `run_with`
  - `pub fn run_welcome() -> iced::Result` — welcome screen entry point
  - Dynamic branch list from `self.state.branches` (replaces hardcoded `"main"` / `"feature/iced-ui"`)
  - Welcome screen when `adapter.is_none()`
  - All `Task::none()` arms annotated `// TODO: [DEBT]`

- `gui_app/src/main.rs`
  - **NOT YET CHANGED** — still the original 226-line file
  - **Must be rewritten** as the thin compositor: import `git_adapter::GitoxideAdapter`, match `std::env::args().nth(1)`, call `gui_app::run()` or `gui_app::run_welcome()`
  - After rewrite: should be ~15 lines total

- `gui_app/Cargo.toml`
  - Modified: added `[lib]` (`name = "gui_app"`, `path = "src/lib.rs"`), `[[bin]]` (`name = "git_cthulhu"`, `path = "src/main.rs"`), `test_support` in `[dev-dependencies]`
  - `git_adapter` stays in `[dependencies]` (accessible to both lib and bin targets)

- `test_support/src/lib.rs`
  - New crate; `FakeGitRepository` with 6 `Result<T, DomainError>` fields implementing all `GitRepository` methods
  - `FakeGitRepository::ok()` all-success constructor
  - Used by adapter crates via `[dev-dependencies]`

- `core_domain/src/lib.rs` (246 lines, unchanged)
  - Defines `GitRepository` port (lines 74–81), `Branch` (lines 49–53), `AppState` (lines 176–186), `TransportState` (lines 106–109), `ViewEvent` (lines 162–173)
  - No changes needed; it IS the port that `lib.rs` depends on

- `config_adapter/src/lib.rs`
  - Drive-by fix: lines 23, 32 — `fn From` → `fn from` (was a compile error blocking build)

- `Cargo.toml` (root)
  - Modified: `"test_support"` added as 9th workspace member
</important_files>

<next_steps>
Remaining implementation work:

**Immediate — one file left to write:**
1. **Rewrite `gui_app/src/main.rs`** as the thin `[[bin]]` compositor:
```rust
//! git_cthulhu binary entry point.
//! Constructs adapters and injects them into the pure gui_app library.
//! This is the only place in the application that touches git_adapter directly.

use git_adapter::GitoxideAdapter;

fn main() -> iced::Result {
    // TODO: [DEBT] Read config_adapter to restore last_repo_path from persisted settings
    match std::env::args().nth(1) {
        Some(path) => gui_app::run(Box::new(GitoxideAdapter::new(&path))),
        None => gui_app::run_welcome(),
    }
}
```
   Note: `iced` is in `gui_app/[dependencies]` so `iced::Result` is accessible to the bin target.

**Then verify and commit:**
2. `cargo build` from workspace root — expect clean compile; fix any issues found
3. Commit: `feat: invert gui_app seam — lib target has zero git_adapter dependency (Candidate B)`
   - Co-authored-by trailer required

**Potential blockers:**
- `text(reason.as_str())` in `lib.rs` — if iced 0.13 has lifetime issues with `&str` from matched enum, may need `.clone()`
- `button(branch.name.as_str())` in the branch loop — same concern; fallback is `button(branch.name.clone())`
- `iced::Result` visible to `main.rs` bin target — comes through `gui_app`'s dependency on `iced`; if not re-exported, may need `iced` added directly to `[[bin]]` or use `std::process::ExitCode`

**Post-implementation (after commit):**
- Run `domain-modeling` skill to name new terms
- Create `CONTEXT.md`
- Write ADR for Candidate B decisions
- Grill Candidate C (AppState God state machine in `core_domain/src/lib.rs` — ALL port traits + domain models in one 246-line file)
</next_steps>