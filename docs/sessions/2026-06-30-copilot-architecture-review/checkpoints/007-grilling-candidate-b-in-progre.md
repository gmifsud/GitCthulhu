<overview>
Gregory Mifsud (principal technical architect) is evolving GitCthulhu from a Rust/iced proof-of-concept Git GUI into a production-grade application. The `improve-codebase-architecture` skill surfaced 7 architectural friction candidates across all 8 crates; a full HTML report was committed to `docs/`. Gregory selected **Candidate B — gui_app seam bypass** for grilling first, and the `grilling` skill interview is now in progress with 2 of ~5 key questions answered.
</overview>

<history>
1. User invoked `improve-codebase-architecture` skill to take GitCthulhu from PoC to production-ready.
   - Session renamed to "Architecture review"; branch renamed to `gmifsud-architecture-review`.
   - Confirmed no `CONTEXT.md` or `docs/adr/` exists.

2. Full codebase exploration across all 8 crates.
   - Read `Cargo.toml` workspace, all crate `lib.rs`/`main.rs` files, per-crate `Cargo.toml`.
   - Critical discovery: `src/App.tsx` is a React/Vite AI Studio web preview — NOT a Tauri frontend. Actual app is the iced GUI in `gui_app/`.
   - 7 architectural friction candidates identified (A–G).

3. HTML architecture review report generated.
   - PowerShell inline here-string failed with `ENAMETOOLONG`; workaround: used `create` tool to write directly to `docs/`.
   - Report written to `docs/architecture-review-20260630.html` and committed at `714bcff`.

4. User selected **Candidate B — gui_app seam bypass** for grilling.
   - `grilling` skill loaded.
   - Re-read `gui_app/src/main.rs`, `gui_app/Cargo.toml`, `core_domain/src/lib.rs` (lines 74–154) to ground grilling in actual code.
   - Critical observation: `adapter: GitoxideAdapter` is stored in `GitApp` but **never called** in any `update()` branch — violation is latent.

5. Grilling interview started (Candidate B — in progress).
   - **Q1 (Startup model):** User chose "Remember last repo, fall back to welcome screen." Implication: `GitApp` cannot be initialized with a concrete adapter at `Default::default()` time — repo path must come from config first.
   - **Q2 (Adapter ownership):** User chose "Full inversion — `gui_app` drops `git_adapter` dep entirely, compositor wires concrete adapter." Implication: `gui_app/Cargo.toml` removes `git_adapter`; a thin compositor/binary layer holds `GitoxideAdapter`; `GitApp` only sees `Box<dyn GitRepository>` from `core_domain`.
   - **Grilling continues** — remaining questions not yet asked.
</history>

<work_done>
Files created/committed:
- `docs/architecture-review-20260630.html` — committed at `714bcff` (no changes since)

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
- [x] Grilling Q1: startup model decided → "remember last repo, fall back to welcome screen"
- [x] Grilling Q2: adapter ownership decided → full inversion, `gui_app` drops `git_adapter` dep
- [ ] **Grilling Q3–Q5: NOT YET ASKED** (see Next Steps)
- [ ] Run `domain-modeling` skill as decisions crystallise
- [ ] Create `CONTEXT.md` lazily
- [ ] ADRs for rejected candidates with load-bearing reasons
- [ ] Implement Candidate B fix (after grilling completes and design is locked)
</work_done>

<technical_details>
**Workspace structure:**
- Rust Cargo workspace, hexagonal/ports-and-adapters pattern — 8 crates: `core_domain`, `git_adapter`, `gui_app`, `auth_adapter`, `config_adapter`, `cache_adapter`, `keyring_adapter`, `ssh_adapter`

**Candidate B — gui_app seam bypass:**
- `gui_app/src/main.rs` line 55: `adapter: GitoxideAdapter` — concrete type, not `Box<dyn GitRepository>`
- `gui_app/src/main.rs` line 71: `GitoxideAdapter::new(".")` — hardcodes CWD, in `Default::default()`
- `gui_app/src/main.rs` lines 78–96: ALL `update()` branches return `Task::none()` — adapter is **never actually called** (violation is latent, not yet painful)
- `gui_app/src/main.rs` lines 162–163: hardcoded branch names `"main"`, `"feature/iced-ui"`
- `gui_app/src/main.rs` lines 99–103: `subscription()` returns `Subscription::none()` — config/file watching not wired
- `gui_app/Cargo.toml` line 9: `git_adapter = { path = "../git_adapter" }` — the seam violation

**Grilling decisions locked so far:**
1. **Startup model:** Remember last repo from config, fall back to welcome screen. Means adapter construction is deferred; `GitApp` cannot use `Default::default()` to build an adapter.
2. **Full inversion:** `gui_app/Cargo.toml` removes `git_adapter` dependency entirely. A thin compositor (likely `main.rs` in a new top-level binary crate, or just the existing `gui_app/src/main.rs` restructured) builds `GitoxideAdapter` and passes `Box<dyn GitRepository>` to `GitApp`. `gui_app` only imports `core_domain`.

**Other key codebase findings:**
- `core_domain/src/lib.rs` — God module: ALL port traits + domain models + `AppState` state machine + `ViewEvent` + `UserCommand` + `build_dag` stub. `handle_token_received` calls `store.save_token()` — side effect inside state machine (not pure).
- `config_adapter/src/lib.rs` — compile error: `fn From` (capital F) on lines 23 and 32, should be `fn from`
- `cache_adapter/src/lib.rs` — functional but never wired in `gui_app`
- `auth_adapter/src/lib.rs` — all logic commented out
- `src/App.tsx` — AI Studio web preview, dead weight, NOT a Tauri frontend

**PowerShell ENAMETOOLONG limitation:**
- Passing multi-KB strings as inline arguments to the `powershell` tool fails. Workaround: use the `create` tool to write directly to the target file path.

**Branch:** `gmifsud-architecture-review` (renamed from `gmifsud-refactored-waddle`)

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
  - Line 5: `use git_adapter::GitoxideAdapter;` — the import to remove
  - Line 55: `adapter: GitoxideAdapter` — field to change to `Box<dyn GitRepository>`
  - Line 68–74: `Default::default()` hardwires `GitoxideAdapter::new(".")` — to be replaced by compositor injection
  - Lines 78–96: ALL `update()` arms return `Task::none()` — adapter never actually called
  - Lines 162–163: hardcoded branch names to remove
  - Lines 99–103: `subscription()` stub — config watching not wired
  - No changes made yet

- `gui_app/Cargo.toml`
  - Line 9: `git_adapter = { path = "../git_adapter" }` — dependency to remove (full inversion decision locked)
  - No changes made yet

- `core_domain/src/lib.rs`
  - ALL port traits (lines 74–154), domain models, AppState state machine (lines 175–245), build_dag stub (lines 26–45)
  - God module — subject of Candidate C grilling (future)
  - `GitRepository` trait at lines 74–81: the port `gui_app` should depend on, not the concrete adapter
  - No changes made
</important_files>

<next_steps>
Grilling Candidate B is in progress — 2 of ~5 questions answered. Continue the interview before writing any code.

**Remaining grilling questions (in order):**

3. **`GitApp` field type when no repo is open:** Should it hold `Option<Box<dyn GitRepository>>` (nullable), or should there be a proper type-state enum (`AppPhase::NoRepo(WelcomeState)` vs `AppPhase::RepoOpen(RepoState { adapter: Box<dyn GitRepository> })`)? Recommendation: type-state enum — `Option<Box<dyn>>` leaks "no repo" guard logic across all `update()` arms.

4. **Async/blocking bridge:** `GitoxideAdapter` operations are blocking. Does the seam expose `async fn`, or does the bridge use `iced::Task` + `tokio::spawn`? This affects the `GitRepository` trait signature in `core_domain`. Recommendation: keep the trait sync (no `async fn`), wrap blocking calls in `tokio::task::spawn_blocking` in the `iced::Task` bridge inside `gui_app`.

5. **Test strategy:** Fake adapter in `gui_app/tests/` integration module, or a shared `test_support` crate? Recommendation: `test_support` crate with `FakeGitRepository` so any future crate can use it.

After grilling concludes:
- Implement the Candidate B fix (remove `git_adapter` dep, inject `Box<dyn GitRepository>`, compositor in `main.rs`)
- Run `domain-modeling` skill to name new domain terms (compositor, app phase, welcome state)
- Create `CONTEXT.md`
- ADR for Candidate B design
- Move to grilling Candidate C (AppState God state machine)
</next_steps>