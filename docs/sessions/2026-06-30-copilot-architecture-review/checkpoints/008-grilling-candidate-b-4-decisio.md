<overview>
Gregory Mifsud (principal technical architect) is evolving GitCthulhu from a Rust/iced proof-of-concept Git GUI into a production-grade application. The `improve-codebase-architecture` skill surfaced 7 architectural friction candidates across all 8 crates; a full HTML report was committed to `docs/`. Gregory selected **Candidate B — gui_app seam bypass** for grilling first, and the `grilling` skill interview has completed 4 of ~5 key questions with all answers locked.
</overview>

<history>
1. User invoked `improve-codebase-architecture` skill to take GitCthulhu from PoC to production-ready.
   - Session renamed to "Architecture review"; branch renamed to `gmifsud-architecture-review`.
   - Confirmed no `CONTEXT.md` or `docs/adr/` exists.

2. Full codebase exploration across all 8 crates.
   - Read `Cargo.toml` workspace, all crate `lib.rs`/`main.rs` files, per-crate `Cargo.toml`.
   - Critical discovery: `src/App.tsx` is a React/Vite AI Studio web preview — NOT a Tauri frontend. Actual app is the iced GUI in `gui_app/`.
   - 7 architectural friction candidates identified (A–G).

3. HTML architecture review report generated and committed.
   - PowerShell inline here-string failed with `ENAMETOOLONG`; workaround: used `create` tool to write directly to `docs/`.
   - Report written to `docs/architecture-review-20260630.html` and committed at `714bcff`.

4. User selected **Candidate B — gui_app seam bypass** for grilling.
   - `grilling` skill loaded; re-read `gui_app/src/main.rs`, `gui_app/Cargo.toml`, `core_domain/src/lib.rs` lines 74–154.
   - Critical observation: `adapter: GitoxideAdapter` is stored in `GitApp` but **never called** in any `update()` branch — violation is latent, not yet painful.

5. Grilling interview — all 4 answered questions locked:
   - **Q1 (Startup model):** "Remember last repo, fall back to welcome screen." → adapter construction must be deferred; `Default::default()` cannot build an adapter.
   - **Q2 (Adapter ownership):** "Full inversion — `gui_app` drops `git_adapter` dep entirely, compositor wires concrete adapter." → `gui_app/Cargo.toml` removes `git_adapter`; thin compositor/binary passes `Box<dyn GitRepository>` to `GitApp`.
   - **Q3 (Compositor location):** "Split `gui_app` into `lib.rs` + `main.rs` bin target — wiring stays in same crate." → `gui_app` becomes a library; `src/main.rs` is the `[[bin]]` target that constructs `GitoxideAdapter` and calls `gui_app::run(repo)`.
   - **Q4 (Async/blocking bridge):** "Sync trait — blocking calls, wrap at `Task::perform` boundary in `gui_app`." → `GitRepository` trait in `core_domain` uses sync `fn`, no `async_trait` dep; `gui_app/src/main.rs` wraps calls via `tokio::task::spawn_blocking` inside `iced::Task::perform`.
</history>

<work_done>
Files created/committed:
- `docs/architecture-review-20260630.html` — committed at `714bcff`. No changes since.

Work completed:
- [x] Loaded `improve-codebase-architecture` skill
- [x] Confirmed no `CONTEXT.md` or `docs/adr/` exists
- [x] Read all 8 crate source files
- [x] Identified 7 architectural candidates (A–G)
- [x] Renamed session ("Architecture review") and branch (`gmifsud-architecture-review`)
- [x] Written and committed HTML report to `docs/`
- [x] User selected Candidate B for grilling
- [x] Loaded `grilling` skill
- [x] Re-read `gui_app/src/main.rs`, `gui_app/Cargo.toml`, `core_domain/src/lib.rs` 74–154
- [x] Grilling Q1: startup model → "remember last repo, fall back to welcome screen"
- [x] Grilling Q2: adapter ownership → full inversion, `gui_app` drops `git_adapter` dep
- [x] Grilling Q3: compositor location → `[[bin]]` target in `gui_app` (`lib.rs` + `main.rs` split)
- [x] Grilling Q4: trait async model → sync trait, `spawn_blocking` at `Task::perform` boundary
- [ ] **Grilling Q5 (test strategy): NOT YET ASKED**
- [ ] Implement Candidate B fix (after grilling completes)
- [ ] Run `domain-modeling` skill to name new terms
- [ ] Create `CONTEXT.md`
- [ ] ADR for Candidate B design
- [ ] Grill Candidates C, A, D/E/F
</work_done>

<technical_details>
**Workspace structure:**
- Rust Cargo workspace, hexagonal/ports-and-adapters pattern — 8 crates: `core_domain`, `git_adapter`, `gui_app`, `auth_adapter`, `config_adapter`, `cache_adapter`, `keyring_adapter`, `ssh_adapter`.

**Candidate B — gui_app seam bypass (central subject):**
- `gui_app/src/main.rs` line 5: `use git_adapter::GitoxideAdapter;` — import to remove from lib
- `gui_app/src/main.rs` line 55: `adapter: GitoxideAdapter` — field to change to `Box<dyn GitRepository>`
- `gui_app/src/main.rs` lines 68–74: `Default::default()` hardwires `GitoxideAdapter::new(".")` — to be replaced by compositor injection
- `gui_app/src/main.rs` lines 78–96: ALL `update()` arms return `Task::none()` — adapter never actually called (violation is latent)
- `gui_app/src/main.rs` lines 162–163: hardcoded branch names `"main"`, `"feature/iced-ui"` — to remove
- `gui_app/src/main.rs` lines 99–103: `subscription()` stub — config watching not wired
- `gui_app/Cargo.toml` line 9: `git_adapter = { path = "../git_adapter" }` — dependency to remove

**Locked design decisions for Candidate B:**
1. **Startup:** Read config on launch; restore last repo if saved, else show welcome screen. `GitApp` holds `Option<Box<dyn GitRepository>>` or type-state (Q5 pending determines which).
2. **Inversion:** `gui_app` library has zero `git_adapter` dep. Only the `[[bin]]` target imports `git_adapter`.
3. **Compositor:** `gui_app` is split: `src/lib.rs` (pure library, `GitApp` struct) + `src/main.rs` (`[[bin]]` target — reads config, constructs `GitoxideAdapter`, calls `lib::run(Box<dyn GitRepository>)`).
4. **Trait signature:** `GitRepository` in `core_domain` is sync (`fn`, not `async fn`). `gui_app` wraps blocking calls with `tokio::task::spawn_blocking` inside `iced::Task::perform`.

**Other key codebase findings:**
- `core_domain/src/lib.rs` — God module: ALL port traits + domain models + `AppState` state machine + `ViewEvent` + `UserCommand` + `build_dag` stub. `handle_token_received` calls `store.save_token()` — side effect inside state machine (not pure). This is Candidate C.
- `config_adapter/src/lib.rs` — compile error: `fn From` (capital F) on lines 23 and 32, should be `fn from`.
- `cache_adapter/src/lib.rs` — functional but never wired in `gui_app`.
- `auth_adapter/src/lib.rs` — all logic commented out.
- `src/App.tsx` — AI Studio web preview, dead weight, NOT a Tauri frontend (Candidate G).

**PowerShell ENAMETOOLONG workaround:**
- Passing multi-KB strings as inline arguments to the `powershell` tool fails. Use the `create` tool to write directly to the target file path instead.

**7 Candidates ranked:**
- **Strong:** A (graph_layout stub), B (gui_app seam bypass), C (AppState God state machine)
- **Worth exploring:** D (DomainError::Unknown everywhere), E (fetch seam leaks auth protocol), F (SshManager wide port)
- **Speculative:** G (React src/ web preview)
- **Recommended order:** B → C → A → D/E/F
</technical_details>

<important_files>
- `docs/architecture-review-20260630.html`
  - Primary deliverable; 7 candidates with Mermaid diagrams, before/after visuals, recommendation badges
  - Created and committed at `714bcff`; no changes since

- `gui_app/src/main.rs`
  - Central subject of Candidate B grilling
  - Line 5: `use git_adapter::GitoxideAdapter;` — the import to remove from lib target
  - Line 55: `adapter: GitoxideAdapter` — field to change to `Box<dyn GitRepository>`
  - Lines 68–74: `Default::default()` hardwires `GitoxideAdapter::new(".")` — replace with compositor injection
  - Lines 78–96: ALL `update()` arms return `Task::none()` — adapter never actually called
  - Lines 162–163: hardcoded branch names to remove
  - Lines 99–103: `subscription()` stub — config watching not wired
  - **No changes made yet**

- `gui_app/Cargo.toml`
  - Line 9: `git_adapter = { path = "../git_adapter" }` — dependency to remove (full inversion locked)
  - Will need `[lib]` section added and `[[bin]]` target added when splitting
  - **No changes made yet**

- `core_domain/src/lib.rs`
  - ALL port traits (lines 74–154), domain models, AppState state machine (lines 175–245), build_dag stub (lines 26–45)
  - `GitRepository` trait at lines 74–81: the port `gui_app` library should depend on, not the concrete adapter
  - God module — subject of Candidate C grilling (future)
  - **No changes made**
</important_files>

<next_steps>
Grilling Candidate B is in progress — 4 of ~5 questions answered. One question remains before implementation begins.

**Remaining grilling question:**

5. **Test strategy:** Where does `FakeGitRepository` live for testing `GitApp` without a real git adapter?
   - Option A: `gui_app/tests/` integration module with a local fake
   - Option B: Shared `test_support` crate so any future crate can use the fake
   - Recommendation: `test_support` crate — shared fakes avoid duplication when Candidate C work also needs a fake `GitRepository`

**After grilling completes, implement Candidate B:**
1. Split `gui_app/src/main.rs` into `src/lib.rs` (GitApp struct, no git_adapter import) and `src/main.rs` (`[[bin]]` wiring target)
2. Update `gui_app/Cargo.toml`: remove `git_adapter` dep from `[lib]`, add it only to `[[bin]]` target; add `[lib]` section
3. Change `adapter: GitoxideAdapter` → `adapter: Option<Box<dyn GitRepository>>` (or type-state per Q5 answer)
4. Replace `Default::default()` adapter construction with compositor injection via `pub fn run(repo: Box<dyn GitRepository>)` entry point
5. Remove hardcoded branch names (lines 162–163)
6. Wire `spawn_blocking` in `update()` arms via `iced::Task::perform`

**Post-implementation:**
- Run `domain-modeling` skill to name new terms (compositor, app phase, welcome state, repo state)
- Create `CONTEXT.md`
- Write ADR for Candidate B design decisions
- Move to grilling Candidate C (AppState God state machine)
</next_steps>