<overview>
Gregory Mifsud (principal technical architect) is evolving GitCthulhu from a Rust/iced proof-of-concept Git GUI into a production-grade application. The chosen approach (Candidate B) is a hexagonal seam inversion: split `gui_app/src/main.rs` into a pure library (`lib.rs`) with zero `git_adapter` source import, and a thin binary compositor (`main.rs`) that wires the concrete `GitoxideAdapter`. A new `test_support` crate provides `FakeGitRepository` for testability. **The Candidate B implementation is now complete and committed at `e90f110`.**
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
   - Branch renamed to `gmifsud-architecture-review` (was `gmifsud-refactored-waddle`, but the commit shows `gmifsud-architecture-review`).

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

6. Commit unblocked and completed.
   - Root `.gitignore` was missing `target/` entry — appended `target/` block.
   - Discovered `target/` was never actually tracked (git pathspec fatal confirmed it).
   - Cleared stale `index.lock` at `C:\Repos\GitCthulhu\.git\worktrees\gmifsud-refactored-waddle\index.lock` (worktree lock lives in main repo's `.git`, not worktree `.git` file).
   - Staged 9 files cleanly (no `target/` artifacts); committed at `e90f110` on branch `gmifsud-architecture-review`.
</history>

<work_done>
Files created:
- `test_support/Cargo.toml` — new workspace crate; depends on `core_domain`
- `test_support/src/lib.rs` — `FakeGitRepository` with 6 pre-configured `Result<T, DomainError>` fields; `FakeGitRepository::ok()` all-success constructor
- `gui_app/src/lib.rs` — pure library target; all GUI logic; zero `git_adapter` source import; `GitApp.adapter: Option<Box<dyn GitRepository + Send>>`; constructors `with_repo()`/`welcome()`; `pub fn run()` / `pub fn run_welcome()`

Files modified:
- `Cargo.toml` (root) — added `"test_support"` to `[workspace] members` (now 9 members)
- `gui_app/Cargo.toml` — added `[lib]`, `[[bin]]`, `test_support` dev-dep
- `gui_app/src/main.rs` — rewritten from 226-line monolith to ~14-line thin compositor
- `config_adapter/src/lib.rs` — `fn From` → `fn from` at lines 23 and 32
- `.gitignore` — appended `# Rust build artifacts` + `target/`
- `Cargo.lock` — newly committed (was previously untracked)

Work completed:
- [x] Architecture review + HTML report committed (`714bcff`)
- [x] Grilling: 5 decisions locked
- [x] `test_support` crate created
- [x] Root `Cargo.toml` updated (test_support member)
- [x] `gui_app/Cargo.toml` updated (`[lib]`, `[[bin]]`, dev-dep)
- [x] `config_adapter/src/lib.rs` fixed
- [x] `gui_app/src/lib.rs` created (pure library, warnings clean)
- [x] `gui_app/src/main.rs` rewritten (thin compositor)
- [x] `cargo build` — exit code 0, zero gui_app warnings ✅
- [x] `.gitignore` updated with `target/`
- [x] **Committed at `e90f110`** on branch `gmifsud-architecture-review` ✅
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

**Worktree git lock file location:** In a worktree, `.git` is a file pointing to `<main_repo>/.git/worktrees/<name>/`. Lock files live at `<main_repo>/.git/worktrees/<name>/index.lock` — NOT in the worktree directory itself. This is non-obvious and caused the commit to be temporarily blocked.

**`target/` tracking status:** Despite `.gitignore` missing `target/`, the directory was never actually tracked (no build artifacts staged). The earlier apparent "staging" was git CRLF warnings on untracked files, not actual staging. `git rm -r --cached target/` returned a fatal pathspec error confirming this.

**Pre-existing warnings (not our changes):**
- `git_adapter` (lib): 2 warnings
- `auth_adapter` (lib): 2 warnings (`unused import: std::thread`, `unused variable: port`)
- These are pre-existing — do not fix as part of Candidate B.
</technical_details>

<important_files>
- `gui_app/src/lib.rs`
  - Core payload of Candidate B — pure GUI library with zero `git_adapter` source dependency
  - Created from scratch; all GUI logic previously in `main.rs`
  - `GitApp.adapter: Option<Box<dyn GitRepository + Send>>`, constructors `with_repo()`/`welcome()`
  - `pub fn run(repo: Box<dyn GitRepository + Send>) -> iced::Result`; `pub fn run_welcome() -> iced::Result`
  - Line 75: `#[allow(dead_code)]` on `Message` enum; Line 142: `fn view(&self) -> Element<'_, Message>` (lifetime fixed)

- `gui_app/src/main.rs`
  - Thin compositor — the only file in the codebase that imports `git_adapter` directly
  - Rewritten from 226-line monolith to ~14 lines

- `gui_app/Cargo.toml`
  - Defines both `[lib]` and `[[bin]]` targets pointing to `lib.rs` and `main.rs` respectively
  - `test_support` added as `[dev-dependencies]`

- `test_support/src/lib.rs`
  - New crate; `FakeGitRepository` implementing all 6 `GitRepository` methods; enables adapter unit tests without real git

- `core_domain/src/lib.rs` (246 lines, unchanged)
  - Defines `GitRepository` port (lines 74–81), `Branch` (lines 49–53), `AppState` (lines 176–186)

- `.gitignore`
  - Updated: appended `target/` under `# Rust build artifacts` comment
</important_files>

<next_steps>
Remaining work (post-commit, in priority order):

1. **Rename branch** — the worktree branch shows as `gmifsud-architecture-review` but was originally `gmifsud-refactored-waddle`. Confirm which name is canonical and whether a PR needs to be opened.

2. **Create PR** for Candidate B commit (`e90f110`) targeting `main`.

3. **Domain modeling** — run `domain-modeling` skill to name new terms introduced by the seam split (`compositor`, `adapter port`, `welcome screen`, etc.) and update `CONTEXT.md`.

4. **ADR** — write Architecture Decision Record documenting the 5 locked Candidate B decisions.

5. **Grill Candidate C** — `AppState` God state machine — ALL port traits + domain models in one 246-line `core_domain/src/lib.rs` file. This is the next highest-friction architectural issue.

6. **Async wiring** — all `// TODO: [DEBT] Wire spawn_blocking` annotations in `gui_app/src/lib.rs` need real `Task::perform` implementations once the seam is proven stable.
</next_steps>