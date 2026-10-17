<overview>
Gregory Mifsud (principal technical architect) is evolving GitCthulhu from a Rust/iced proof-of-concept Git GUI into a production-grade application. The chosen approach (Candidate B) is a hexagonal seam inversion: split `gui_app/src/main.rs` into a pure library (`lib.rs`) with zero `git_adapter` source import, and a thin binary compositor (`main.rs`) that wires the concrete `GitoxideAdapter`. A new `test_support` crate provides `FakeGitRepository` for testability. The implementation is now complete and `cargo build` exited with code 0.
</overview>

<history>
1. User invoked `improve-codebase-architecture` skill to evolve GitCthulhu from PoC to production-ready.
   - Full codebase exploration across all 8 workspace crates.
   - Critical discovery: `src/App.tsx` is an AI Studio web preview (NOT Tauri) — the real app is the `gui_app/` iced GUI.
   - 7 architectural friction candidates (A–G) identified and documented.
   - HTML report written to `docs/architecture-review-20260630.html`, committed at `714bcff`.

2. User selected Candidate B (gui_app seam bypass) for grilling.
   - `grilling` skill loaded; all 5 design decisions answered and locked.
   - User approved implementation.
   - Branch renamed to `gmifsud-candidate-b-seam-fix`.

3. Implementation Phase 1 — `test_support` crate created.
   - `test_support/Cargo.toml` created (depends on `core_domain`).
   - `test_support/src/lib.rs` created with `FakeGitRepository` implementing all 6 `GitRepository` methods.

4. Implementation Phase 2 — library split and compositor.
   - Root `Cargo.toml` updated: `"test_support"` added as 9th workspace member.
   - `gui_app/Cargo.toml` updated: added `[lib]`, `[[bin]]`, `test_support` dev-dep.
   - `config_adapter/src/lib.rs` drive-by fix: `fn From` → `fn from` on lines 23 and 32.
   - `gui_app/src/lib.rs` created — pure GUI library, zero `git_adapter` import, `Option<Box<dyn GitRepository + Send>>` adapter field, `run()`/`run_welcome()` public entry points.
   - `gui_app/src/main.rs` rewritten as ~14-line thin compositor: imports `GitoxideAdapter`, matches `std::env::args().nth(1)`, calls `gui_app::run()` or `gui_app::run_welcome()`.
   - `cargo build` completed successfully with exit code 0.
</history>

<work_done>
Files created:
- `test_support/Cargo.toml` — new workspace crate; depends on `core_domain`
- `test_support/src/lib.rs` — `FakeGitRepository` with 6 pre-configured `Result<T, DomainError>` fields; `FakeGitRepository::ok()` all-success constructor
- `gui_app/src/lib.rs` — pure library target; all GUI logic; zero `git_adapter` source import; `GitApp.adapter: Option<Box<dyn GitRepository + Send>>`; `GitApp::with_repo()` / `GitApp::welcome()` constructors; `pub fn run()` / `pub fn run_welcome()`; welcome screen; dynamic branch list; all `Task::none()` arms annotated `// TODO: [DEBT]`

Files modified:
- `Cargo.toml` (root) — added `"test_support"` to `[workspace] members` (now 9 members)
- `gui_app/Cargo.toml` — added `[lib]` (`name = "gui_app"`, `path = "src/lib.rs"`), `[[bin]]` (`name = "git_cthulhu"`, `path = "src/main.rs"`), `test_support` dev-dep
- `config_adapter/src/lib.rs` — fixed `fn From` → `fn from` at lines 23 and 32
- `gui_app/src/main.rs` — **rewritten** from 226-line monolith to ~14-line thin compositor

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
- [x] `gui_app/src/main.rs` rewritten (thin compositor)
- [x] `cargo build` — exit code 0 ✅
- [ ] **PENDING: Commit**
</work_done>

<technical_details>
**Locked design decisions (Q1–Q5):**
1. `GitApp.adapter` → `Option<Box<dyn GitRepository + Send>>` — `None` = welcome screen, `Some` = repo view.
2. `gui_app/src/lib.rs` has zero `git_adapter` source import. `[dependencies]` still lists it (Cargo can't do per-target lib/bin deps without feature flags) — isolation is enforced at source level only.
3. `gui_app/src/main.rs` compositor: reads `std::env::args().nth(1)` for repo path, constructs `GitoxideAdapter`, calls `gui_app::run(Box::new(adapter))`. Falls back to `gui_app::run_welcome()` if no path.
4. Async deferred: all `update()` arms return `Task::none()`, annotated `// TODO: [DEBT] Wire spawn_blocking`.
5. `test_support` crate: `FakeGitRepository` with pre-configured clone-based `Result` fields. Adapter crates use it via `[dev-dependencies]`.

**iced 0.13 API notes:**
- `.run()` requires `State: Default` — cannot inject initial state.
- Injection pattern: `.run_with(move || (state, Task::none()))` — does NOT require `Default`.
- `run_with` closure must be `Send + 'static`.
- Functional API: `iced::application("title", GitApp::update, GitApp::view)` — state type inferred.
- `GitApp: Send` confirmed: all fields are `Send`.

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

**`Branch` struct:** `pub struct Branch { pub name: String, pub is_remote: bool, pub is_head: bool }`

**`AppState` relevant fields:** `transport_state: TransportState`, `commits: Vec<Commit>`, `dag_nodes: Vec<DagNode>`, `branches: Vec<Branch>`, `loading: bool`

**`config_adapter` bug fixed:** Lines 23, 32 had `fn From` (capital F) — invalid Rust; now `fn from`.

**PowerShell ENAMETOOLONG workaround:** Multi-KB strings as inline powershell args fail — use the `create` tool to write files directly.
</technical_details>

<important_files>
- `gui_app/src/lib.rs`
  - **Core payload of Candidate B** — pure GUI library with zero `git_adapter` source dependency
  - Created from scratch; replaces the monolithic `main.rs` logic
  - `GitApp.adapter: Option<Box<dyn GitRepository + Send>>`, `GitApp::with_repo()`, `GitApp::welcome()`
  - `pub fn run(repo: Box<dyn GitRepository + Send>) -> iced::Result`
  - `pub fn run_welcome() -> iced::Result`
  - Dynamic branch list from `self.state.branches`; welcome screen when adapter is `None`

- `gui_app/src/main.rs`
  - **Thin compositor** — the only file in the codebase that imports `git_adapter` directly
  - Rewritten from 226-line monolith to ~14 lines
  - Matches `std::env::args().nth(1)`, constructs `GitoxideAdapter`, delegates to `gui_app::run()` or `gui_app::run_welcome()`

- `gui_app/Cargo.toml`
  - Defines both `[lib]` and `[[bin]]` targets pointing to `lib.rs` and `main.rs` respectively
  - `test_support` added as `[dev-dependencies]`

- `test_support/src/lib.rs`
  - New crate; `FakeGitRepository` implementing all 6 `GitRepository` methods
  - `FakeGitRepository::ok()` all-success constructor for tests

- `core_domain/src/lib.rs` (246 lines, unchanged)
  - Defines `GitRepository` port (lines 74–81), `Branch` (lines 49–53), `AppState` (lines 176–186)
  - The port that `lib.rs` depends on — no changes needed

- `config_adapter/src/lib.rs`
  - Drive-by fix: lines 23, 32 — `fn From` → `fn from` (was a compile error blocking build)

- `Cargo.toml` (root)
  - Now has 9 workspace members including new `test_support`
</important_files>

<next_steps>
Remaining work:
- **Commit** the implementation with message:
  `feat: invert gui_app seam — lib target has zero git_adapter dependency (Candidate B)`
  (include Co-authored-by trailer)

Post-commit work:
- Run `domain-modeling` skill to name new terms introduced by the seam split
- Create `CONTEXT.md` for the project
- Write ADR documenting the 5 locked Candidate B decisions
- Grill **Candidate C**: `AppState` God state machine — ALL port traits + domain models in one 246-line `core_domain/src/lib.rs` file

Immediate next action:
```
git add -A
git commit -m "feat: invert gui_app seam — lib target has zero git_adapter dependency (Candidate B)

Co-authored-by: Copilot App <223556219+Copilot@users.noreply.github.com>"
```
</next_steps>