<overview>
Gregory Mifsud (principal technical architect) is evolving GitCthulhu from a Rust/iced proof-of-concept Git GUI into a production-grade application using a hexagonal/ports-and-adapters architecture across 8 Cargo crates. The `improve-codebase-architecture` skill identified 7 architectural friction candidates; the `grilling` skill was used to interrogate all design decisions for **Candidate B (gui_app seam bypass)** before implementation. All 5 grilling questions are now answered and locked — implementation is approved and about to begin.
</overview>

<history>
1. User invoked `improve-codebase-architecture` to take GitCthulhu from PoC to production-ready.
   - Session renamed "Architecture review"; branch renamed `gmifsud-architecture-review`.
   - Full codebase exploration across all 8 crates.
   - Critical discovery: `src/App.tsx` is an AI Studio web preview (NOT Tauri) — real app is `gui_app/` iced GUI.
   - 7 architectural friction candidates identified (A–G).
   - HTML report written to `docs/architecture-review-20260630.html`, committed at `714bcff`.

2. User selected **Candidate B — gui_app seam bypass** for grilling.
   - `gui_app/src/main.rs` imports and hardwires `GitoxideAdapter` directly — violates hexagonal boundaries.
   - Violation is latent: ALL `update()` arms return `Task::none()`, adapter is stored but never called.
   - `grilling` skill loaded; all 5 questions asked and answered.

3. Grilling Q1–Q5 completed and locked (user approved each answer):
   - **Q1 (Startup):** Remember last repo from config; fall back to welcome screen. → `GitApp` holds `Option<Box<dyn GitRepository>>`.
   - **Q2 (Ownership):** Full inversion — `gui_app` lib drops `git_adapter` dep entirely; only the `[[bin]]` target imports it.
   - **Q3 (Compositor):** Split `gui_app` into `src/lib.rs` (pure library) + `src/main.rs` (`[[bin]]` target that constructs `GitoxideAdapter` and calls `lib::run()`).
   - **Q4 (Async model):** Sync `GitRepository` trait (no `async_trait`); `gui_app` wraps blocking calls with `tokio::task::spawn_blocking` inside `iced::Task::perform`.
   - **Q5 (Test strategy):** `test_support` crate with shared `FakeGitRepository` — reusable by all adapter crates as a `[dev-dependency]`.

4. User approved implementation ("Yes") — work is about to begin.
</history>

<work_done>
Files created/committed:
- `docs/architecture-review-20260630.html` — committed at `714bcff`. HTML report with 7 candidates, Mermaid diagrams, before/after visuals, recommendation badges. No changes since.

Work completed:
- [x] Loaded `improve-codebase-architecture` skill
- [x] Confirmed no `CONTEXT.md` or `docs/adr/` exists
- [x] Read all 8 crate source files
- [x] Identified 7 architectural candidates (A–G)
- [x] Renamed session and branch (`gmifsud-architecture-review`)
- [x] Written and committed HTML report to `docs/`
- [x] User selected Candidate B for grilling
- [x] Loaded `grilling` skill
- [x] Grilling Q1–Q5 all answered, locked, and user-approved
- [ ] **NEXT: Implement Candidate B** — no code changes made yet

Current state: All planning complete, zero implementation files touched. Codebase is identical to `714bcff` commit.
</work_done>

<technical_details>
**Workspace:** Rust Cargo workspace, hexagonal/ports-and-adapters, 8 crates: `core_domain`, `git_adapter`, `gui_app`, `auth_adapter`, `config_adapter`, `cache_adapter`, `keyring_adapter`, `ssh_adapter`.

**Candidate B — locked design decisions:**
1. `GitApp` holds `Option<Box<dyn GitRepository>>` (not `GitoxideAdapter`). `None` → welcome screen. `Some(repo)` → repo view.
2. `gui_app` crate is split: `src/lib.rs` (library target, zero `git_adapter` dep) + `src/main.rs` (`[[bin]]` compositor target that imports `git_adapter`, constructs `GitoxideAdapter`, reads config, calls `gui_app::run(Box<dyn GitRepository>)`).
3. `GitRepository` trait in `core_domain` uses sync `fn` signatures. Blocking calls wrapped with `tokio::task::spawn_blocking` inside `iced::Task::perform` in `gui_app`.
4. `test_support` crate created as new workspace member — contains `FakeGitRepository` with controllable response queues. All adapter crates use it in `[dev-dependencies]`.
5. Hardcoded branch names `"main"` and `"feature/iced-ui"` (lines 162–163 of `gui_app/src/main.rs`) must be removed.

**Other key codebase findings (not yet addressed):**
- `core_domain/src/lib.rs` — God module: ALL port traits + domain models + `AppState` state machine + `ViewEvent` + `UserCommand` + `build_dag` stub. `handle_token_received` has side effects inside state machine. → Candidate C (future grilling).
- `config_adapter/src/lib.rs` — compile error: `fn From` (capital F) on lines 23 and 32, should be `fn from`.
- `auth_adapter/src/lib.rs` — all logic commented out.
- `cache_adapter/src/lib.rs` — functional but never wired in `gui_app`.
- `src/App.tsx` — AI Studio web preview, dead weight, NOT a Tauri frontend (Candidate G).

**PowerShell ENAMETOOLONG workaround:**
- Passing multi-KB strings as inline arguments to the `powershell` tool fails. Use the `create` tool to write directly to the target file path instead.

**`gui_app/src/main.rs` violation is latent:**
- `adapter: GitoxideAdapter` is stored in `GitApp` but ALL `update()` arms return `Task::none()` — adapter is never called. Fix is safe to land without behaviour regression.

**Recommended grilling order for remaining candidates:** C → A → D/E/F.
</technical_details>

<important_files>
- `gui_app/src/main.rs`
  - Central subject of Candidate B — the file being split and refactored
  - Line 5: `use git_adapter::GitoxideAdapter;` — import to remove from lib target
  - Line 55: `adapter: GitoxideAdapter` — field to change to `Option<Box<dyn GitRepository>>`
  - Lines 68–74: `Default::default()` hardwires `GitoxideAdapter::new(".")` — replace with compositor injection
  - Lines 78–96: ALL `update()` arms return `Task::none()` — adapter never called (latent violation, safe to change)
  - Lines 162–163: hardcoded branch names `"main"`, `"feature/iced-ui"` — to remove
  - Lines 99–103: `subscription()` stub — config watching not wired
  - **No changes made yet**

- `gui_app/Cargo.toml`
  - Line 9: `git_adapter = { path = "../git_adapter" }` — dependency to remove from lib, keep only in `[[bin]]`
  - Needs `[lib]` section + `[[bin]]` target added when splitting
  - **No changes made yet**

- `core_domain/src/lib.rs`
  - Lines 74–81: `GitRepository` trait — the port `gui_app` library depends on (sync `fn` signatures confirmed)
  - Lines 175–245: `AppState` state machine (God module — Candidate C, future)
  - Lines 26–45: `build_dag` stub
  - **No changes made**

- `docs/architecture-review-20260630.html`
  - Primary deliverable, committed at `714bcff`
  - 7 candidates with Mermaid diagrams, before/after, recommendation badges
  - No changes needed

- `Cargo.toml` (workspace root)
  - Will need `test_support` added to `[workspace] members` when creating that crate
  - **No changes made yet**
</important_files>

<next_steps>
Immediate implementation plan (approved by user):

1. **Create `test_support/` crate**
   - `test_support/Cargo.toml` — `[dev-dependency]` crate, depends on `core_domain`
   - `test_support/src/lib.rs` — `FakeGitRepository` struct with controllable response queues and call recording
   - Add `test_support` to workspace `members` in root `Cargo.toml`

2. **Split `gui_app` into lib + bin**
   - Create `gui_app/src/lib.rs` — move `GitApp` struct here; import `core_domain::GitRepository` trait only (zero `git_adapter` dep); `adapter` field becomes `Option<Box<dyn GitRepository>>`; expose `pub fn run(repo: Box<dyn GitRepository>)` entry point
   - Rewrite `gui_app/src/main.rs` as `[[bin]]` compositor — reads config, constructs `GitoxideAdapter`, calls `gui_app::run()`
   - Remove hardcoded branch names `"main"` and `"feature/iced-ui"`

3. **Update `gui_app/Cargo.toml`**
   - Add `[lib]` section (path = `src/lib.rs`)
   - Add `[[bin]]` target (name = `git_cthulhu`, path = `src/main.rs`)
   - Move `git_adapter` dep to `[[bin]]` target only (or use a `[target.'cfg(...)'.dependencies]` section if Cargo requires it — may need `build-dependencies` approach)

4. **Wire `spawn_blocking` in `update()` arms** — replace all `Task::none()` stubs with `iced::Task::perform(tokio::task::spawn_blocking(...))` pattern for actual git calls

5. **Verify build** — `cargo build` from workspace root; fix `config_adapter` compile error (`fn From` → `fn from`) as a drive-by fix if it blocks compilation

6. **Post-implementation:**
   - Run `domain-modeling` skill to name new terms (compositor, app phase, welcome state, repo state)
   - Create `CONTEXT.md`
   - Write ADR for Candidate B design decisions
   - Proceed to grill Candidate C (AppState God state machine)
</next_steps>