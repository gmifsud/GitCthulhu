<overview>
Gregory Mifsud (principal technical architect) is evolving GitCthulhu from a Rust/iced proof-of-concept Git GUI into a production-grade application. The chosen approach (Candidate B) is a hexagonal seam inversion: split `gui_app/src/main.rs` into a pure library (`lib.rs`) with zero `git_adapter` source import, and a thin binary compositor (`main.rs`) that wires the concrete `GitoxideAdapter`. A new `test_support` crate provides `FakeGitRepository` for testability. The implementation is complete and `cargo build` exits with code 0 (warnings clean). The commit is blocked because `target/` is missing from `.gitignore`, causing git to stage thousands of build artifacts.
</overview>

<history>
1. User invoked `improve-codebase-architecture` skill to evolve GitCthulhu from PoC to production-ready.
   - Full codebase exploration across all 8 workspace crates.
   - Critical discovery: `src/App.tsx` is an AI Studio web preview (NOT Tauri) — the real app is `gui_app/` iced GUI.
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
   - `gui_app/src/lib.rs` created — pure GUI library, zero `git_adapter` import.
   - `gui_app/src/main.rs` rewritten as ~14-line thin compositor.
   - `cargo build` completed successfully (exit code 0).

5. Pre-commit warning cleanup.
   - Fixed `Element<'_, Message>` lifetime annotation in `gui_app/src/lib.rs` line 142.
   - Added `#[allow(dead_code)]` to `Message` enum for forward-scaffolded variants.
   - Second `cargo build` confirmed: exit code 0, zero `gui_app` warnings.

6. Attempted commit — blocked by missing `target/` in `.gitignore`.
   - `git add -A` began staging thousands of build artifacts from `target/`.
   - Command killed after discovering `.gitignore` only excludes `node_modules/`, `build/`, `dist/`, `coverage/`, `.DS_Store`, `*.log`, `.env*`.
   - **`target/` is not excluded** — must be added before committing.
</history>

<work_done>
Files created:
- `test_support/Cargo.toml` — new workspace crate; depends on `core_domain`
- `test_support/src/lib.rs` — `FakeGitRepository` with 6 pre-configured `Result<T, DomainError>` fields; `FakeGitRepository::ok()` all-success constructor
- `gui_app/src/lib.rs` — pure library target; all GUI logic; zero `git_adapter` source import; `GitApp.adapter: Option<Box<dyn GitRepository + Send>>`; `GitApp::with_repo()` / `GitApp::welcome()` constructors; `pub fn run()` / `pub fn run_welcome()`; welcome screen; dynamic branch list

Files modified:
- `Cargo.toml` (root) — added `"test_support"` to `[workspace] members` (now 9 members)
- `gui_app/Cargo.toml` — added `[lib]`, `[[bin]]`, `test_support` dev-dep
- `gui_app/src/main.rs` — rewritten from 226-line monolith to ~14-line thin compositor
- `gui_app/src/lib.rs` — fixed `Element<'_, Message>` lifetime (line 142); added `#[allow(dead_code)]` on `Message` enum
- `config_adapter/src/lib.rs` — fixed `fn From` → `fn from` at lines 23 and 32
- `.gitignore` — **NOT YET UPDATED** — missing `target/` entry (this is the blocker)

Work completed:
- [x] Architecture review + HTML report committed (`714bcff`)
- [x] Grilling: 5 decisions locked
- [x] Branch renamed to `gmifsud-candidate-b-seam-fix`
- [x] `test_support` crate created
- [x] Root `Cargo.toml` updated (test_support member)
- [x] `gui_app/Cargo.toml` updated (`[lib]`, `[[bin]]`, dev-dep)
- [x] `config_adapter/src/lib.rs` fixed
- [x] `gui_app/src/lib.rs` created (pure library, warnings clean)
- [x] `gui_app/src/main.rs` rewritten (thin compositor)
- [x] `cargo build` — exit code 0, zero gui_app warnings ✅
- [ ] **BLOCKED: Add `target/` to `.gitignore` before committing**
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

**`.gitignore` gap:** Only excludes `node_modules/`, `build/`, `dist/`, `coverage/`, `.DS_Store`, `*.log`, `.env*`. Missing `target/` (Rust build artifacts), `Cargo.lock` (workspace policy TBD). Must add `target/` before any commit.

**PowerShell ENAMETOOLONG workaround:** Multi-KB strings as inline powershell args fail — use the `create` tool to write files directly.

**Pre-existing warnings (not our changes):**
- `git_adapter` (lib): 2 warnings
- `auth_adapter` (lib): 2 warnings (`unused import: std::thread`, `unused variable: port`)
- These are pre-existing — do not fix as part of Candidate B.
</technical_details>

<important_files>
- `gui_app/src/lib.rs`
  - Core payload of Candidate B — pure GUI library with zero `git_adapter` source dependency
  - Created from scratch; contains all GUI logic previously in `main.rs`
  - `GitApp.adapter: Option<Box<dyn GitRepository + Send>>`, `GitApp::with_repo()`, `GitApp::welcome()`
  - `pub fn run(repo: Box<dyn GitRepository + Send>) -> iced::Result`
  - `pub fn run_welcome() -> iced::Result`
  - Line 75: `#[allow(dead_code)]` on `Message` enum
  - Line 142: `fn view(&self) -> Element<'_, Message>` (lifetime fixed)

- `gui_app/src/main.rs`
  - Thin compositor — the only file in the codebase that imports `git_adapter` directly
  - Rewritten from 226-line monolith to ~14 lines

- `gui_app/Cargo.toml`
  - Defines both `[lib]` and `[[bin]]` targets pointing to `lib.rs` and `main.rs` respectively
  - `test_support` added as `[dev-dependencies]`

- `test_support/src/lib.rs`
  - New crate; `FakeGitRepository` implementing all 6 `GitRepository` methods

- `.gitignore` (root)
  - **CRITICAL: Missing `target/`** — must be updated before `git add -A`
  - Currently only has: `node_modules/`, `build/`, `dist/`, `coverage/`, `.DS_Store`, `*.log`, `.env*`

- `core_domain/src/lib.rs` (246 lines, unchanged)
  - Defines `GitRepository` port (lines 74–81), `Branch` (lines 49–53), `AppState` (lines 176–186)

- `config_adapter/src/lib.rs`
  - Drive-by fix: lines 23, 32 — `fn From` → `fn from`
</important_files>

<next_steps>
Remaining work (immediate blockers first):

1. **Add `target/` to `.gitignore`** (and optionally `Cargo.lock` per workspace policy)
   ```
   /target
   ```
   Append to `.gitignore` before any git operation.

2. **Commit** with message:
   ```
   feat: invert gui_app seam — lib target has zero git_adapter dependency (Candidate B)

   - Split gui_app into lib (src/lib.rs) and bin (src/main.rs) targets
   - lib.rs: pure GUI logic, zero git_adapter source import, adapter injected as
     Option<Box<dyn GitRepository + Send>> via with_repo()/welcome() constructors
   - main.rs: thin ~15-line compositor; reads argv[1] for repo path, constructs
     GitoxideAdapter, calls gui_app::run() or gui_app::run_welcome()
   - Add test_support crate: FakeGitRepository for adapter unit tests
   - Fix config_adapter fn From -> fn from (was a compile error)
   - Annotate all Task::none() arms with TODO: [DEBT] for async wiring

   Co-authored-by: Copilot App <223556219+Copilot@users.noreply.github.com>
   ```

Post-commit work:
- Run `domain-modeling` skill to name new terms introduced by the seam split
- Create `CONTEXT.md` for the project
- Write ADR documenting the 5 locked Candidate B decisions
- Grill **Candidate C**: `AppState` God state machine — ALL port traits + domain models in one 246-line `core_domain/src/lib.rs` file
</next_steps>