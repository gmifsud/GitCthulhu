<overview>
Gregory Mifsud (principal technical architect) wants to evolve GitCthulhu from a proof-of-concept into a production-grade Rust/iced desktop Git GUI application. The `improve-codebase-architecture` skill was invoked to surface architectural friction and propose deepening opportunities. The approach: read all crates organically, identify shallow modules and leaky seams, then generate an HTML architecture review report with candidates ranked by recommendation strength, followed by a grilling loop on the user's chosen candidate.
</overview>

<history>
1. User invoked `improve-codebase-architecture` skill with context that GitCthulhu was a PoC needing to become production-ready.
   - Skill loaded; requires reading CONTEXT.md/ADRs first, then organic exploration, then HTML report to `%TEMP%`, then grilling loop.
   - Session renamed to "Architecture review"; branch renamed to `gmifsud-architecture-review`.
   - Confirmed no `CONTEXT.md` exists and no `docs/adr/` directory exists.

2. Full codebase exploration across all 8 crates + frontend.
   - Read `Cargo.toml` workspace, `core_domain/src/lib.rs`, `git_adapter/src/lib.rs`, `gui_app/src/main.rs`, `auth_adapter/src/lib.rs`, `ssh_adapter/src/lib.rs`, `config_adapter/src/lib.rs`, `cache_adapter/src/lib.rs`, `keyring_adapter/src/lib.rs`, `src/App.tsx`.
   - Read per-crate `Cargo.toml` files for `core_domain`, `git_adapter`, `gui_app`, `cache_adapter`.
   - Key finding: `src/App.tsx` is a React/Vite "web preview" from Google AI Studio — not a real Tauri frontend. The actual app is the iced GUI in `gui_app/`.

3. Attempted to write HTML architecture review report.
   - First PowerShell attempt failed with `ENAMETOOLONG` because the HTML was passed as an inline here-string — too large for a single command argument.
   - Need to write via a different method (temp file, chunked writes, or a script file).
</history>

<work_done>
Files updated:
- None — no source files changed yet. All work has been exploration and analysis.

Work completed:
- [x] Loaded `improve-codebase-architecture` skill
- [x] Confirmed no `CONTEXT.md` or `docs/adr/` exists
- [x] Read all 8 crate source files (`lib.rs` / `main.rs`)
- [x] Read per-crate `Cargo.toml` for `core_domain`, `git_adapter`, `gui_app`, `cache_adapter`
- [x] Read `src/App.tsx` (confirmed it's AI Studio web preview, not real Tauri frontend)
- [x] Identified 7 architectural candidates (A through G)
- [x] Renamed branch to `gmifsud-architecture-review`
- [ ] Write HTML architecture review report to `%TEMP%\architecture-review-20260630-082536.html` — **FAILED** (ENAMETOOLONG — inline PowerShell string too long)
- [ ] Open report in browser
- [ ] Ask user which candidate to explore
- [ ] Run grilling loop on chosen candidate
- [ ] Create `CONTEXT.md` via `/domain-modeling`

Current blocker: The HTML report (large, multi-KB string) cannot be passed as an inline PowerShell here-string argument. Need to write it using a different approach — e.g., write the HTML to the artifacts folder first as a file, then copy it to `%TEMP%`, or use chunked `Add-Content` calls.
</work_done>

<technical_details>
**Architecture overview (all 8 crates read):**
- Rust Cargo workspace, hexagonal/ports-and-adapters pattern
- `core_domain`: zero-dep claim but has `rayon` + `serde`; defines ALL port traits + domain models + `AppState` state machine + `ViewEvent` + `UserCommand` + `build_dag` stub
- `git_adapter`: gitoxide (`gix 0.66`), tokio, all methods are stubs except `open_repo()` and one test
- `gui_app`: iced 0.13, depends on `git_adapter` directly (bypasses port), `GitApp { adapter: GitoxideAdapter }`, `Default::default()` hardwires `GitoxideAdapter::new(".")`
- `auth_adapter`: OAuth loopback server — all logic commented out, pure stub
- `config_adapter`: confy + notify for file watching — has **compile error**: `fn From` (capital F) instead of `fn from` (lowercase)
- `cache_adapter`: redb + bincode — works but is **never wired up** in `gui_app`
- `keyring_adapter`: OS keyring via `keyring` crate — most complete adapter, actually functional
- `ssh_adapter`: NativeSshManager — functional for key discovery and path translation; `preflight_check` spawns `ssh -V`

**`src/App.tsx` is NOT a Tauri frontend** — it's a React/Vite "sandboxed web preview" from Google AI Studio, showing a file tree of the Rust workspace. The footer says "ENGINE: VITE/REACT". The README.md is also the AI Studio template. The real application is the iced GUI in `gui_app/`.

**7 Friction Candidates identified:**

**A — graph_layout: build_dag stub** (Strong)
- `build_dag` in `core_domain/src/lib.rs` uses `index % 3` for lane assignment — a 3-line stub
- Called inline in `AppState::apply_event` — not independently testable
- Should be extracted to a standalone `graph_layout` crate with pure fn `layout_graph(&[Commit]) → Vec<DagNode>`
- Removes `rayon` from `core_domain`, enables property testing

**B — gui_app direct coupling** (Strong)
- `GitApp { adapter: GitoxideAdapter }` — concrete type, not `Box<dyn GitRepository>`
- `gui_app/Cargo.toml` depends on `git_adapter` directly
- `Default::default()` constructs `GitoxideAdapter::new(".")` — no repository selection
- Fix: compositor pattern in `main.rs`, inject through ports, drop `git_adapter` dep from `gui_app`

**C — AppState God state machine** (Strong)
- 10+ fields covering: lifecycle phase, repo data, auth flow, settings, DAG geometry
- `handle_token_received` calls `store.save_token()` — side effect inside state machine = not pure
- Fix: split into `AppPhase`, `RepoSession`, `AuthFlow`; state machine returns `(State, Vec<SideEffect>)` — pure

**D — DomainError::Unknown(String)** (Worth exploring)
- Every adapter maps every error to `Unknown(String)` — 6 adapters, same pattern
- `NotFound` and `AuthenticationFailed` variants exist but are never reached
- Fix: add `AuthExpired`, `NetworkTimeout`, `RepositoryCorrupt`, `SshKeyNotFound`, `PermissionDenied` variants

**E — GitRepository fetch seam** (Worth exploring)
- Two methods: `fetch(&dyn CredentialStore)` and `fetch_ssh(&dyn SshManager)` — port knows transport
- Fix: `enum GitCredential { Token(String), SshKey { path }, Anonymous }`, single `fetch(cred: GitCredential)`

**F — SshManager wide port** (Worth exploring)
- 4 methods: `get_default_key_path`, `resolve_subsystem_path`, `prepare_ssh_env`, `preflight_check`
- Caller only needs `prepare_ssh_env` and `preflight_check`; the other two are internal impl steps
- Fix: narrow port to `resolve_credential()` + `preflight_check()`; others become private on `NativeSshManager`

**G — src/ React web preview** (Speculative)
- Dead weight if the target is iced; significant reversal if pivoting to Tauri
- Not worth architectural effort until framework decision is settled

**Recommended order:** B → C → A → D/E/F

**PowerShell limitation discovered:** Passing multi-KB HTML as an inline here-string via the `powershell` tool fails with `ENAMETOOLONG`. Must write HTML via chunked `Add-Content`, a script file in the artifacts directory, or `Set-Content` with a variable built from smaller pieces.

**Temp path:** `C:\Users\grego\AppData\Local\Temp`
**Target HTML file:** `C:\Users\grego\AppData\Local\Temp\architecture-review-20260630-082536.html`
**Artifacts dir:** `C:\Users\grego\.copilot\session-state\d79b1a9e-8a31-4536-8f65-59b30a1d7942\files`
</technical_details>

<important_files>
- `core_domain/src/lib.rs`
  - Central to everything — defines ALL port traits, domain models, AppState state machine, ViewEvent, UserCommand, build_dag
  - **Primary friction**: too wide; conflates domain models + ports + state machine + algorithm + events
  - Lines 74–154: port traits; Lines 175–245: AppState state machine; Lines 26–45: build_dag stub
  - No changes made

- `gui_app/src/main.rs`
  - iced desktop UI; holds `GitoxideAdapter` directly (bypasses the `GitRepository` seam)
  - Line 53–57: `GitApp` struct with hardwired `adapter: GitoxideAdapter`
  - Lines 160–163: hardcoded branch names `"main"`, `"feature/iced-ui"` — not connected to state
  - Lines 99–103: `subscription()` returns `Subscription::none()` — config/file watching not wired
  - No changes made

- `git_adapter/src/lib.rs`
  - Implements `GitRepository` for gitoxide; all methods are stubs except `open_repo()` and one test
  - Lines 82–111: split fetch with auth protocol seam (`fetch` vs `fetch_ssh`)
  - Lines 114–133: only real test in the codebase
  - No changes made

- `config_adapter/src/lib.rs`
  - Has **compile error**: `fn From` (capital F) on lines 23 and 32 — should be `fn from` (lowercase)
  - `watch_settings` implementation is the most sophisticated adapter logic (notify file watcher)
  - No changes made

- `cache_adapter/src/lib.rs`
  - redb + bincode — functional implementation but **never wired up** in `gui_app`
  - Orphaned — the `GraphCache` port is unused
  - No changes made

- `gui_app/Cargo.toml`
  - Critical: depends on `git_adapter = { path = "../git_adapter" }` — this is the seam violation
  - Removing this dep is the goal of Candidate B
  - No changes made

- `src/App.tsx`
  - Confirmed to be an AI Studio web preview (React/Vite file tree demo), NOT a Tauri frontend
  - Footer text: "ENGINE: VITE/REACT", "OS: WEB PREVIEW"
  - Explains the iced+React coexistence — they're not in conflict; React is dead weight
</important_files>

<next_steps>
Remaining work:

**Immediate — unblock the HTML report:**
Write the HTML report to `C:\Users\grego\AppData\Local\Temp\architecture-review-20260630-082536.html` using a method that avoids ENAMETOOLONG. Best approach: write a PowerShell script file to the artifacts directory (`C:\Users\grego\.copilot\session-state\d79b1a9e-8a31-4536-8f65-59b30a1d7942\files\write-report.ps1`) that contains the full HTML as a here-string, then execute it with `& <path>`. Alternatively, use multiple `Add-Content` calls to build the file incrementally.

**After report is open:**
1. Ask user: "Which of these candidates would you like to explore?"
2. Run `/grilling` skill loop on chosen candidate
3. As decisions crystallize, create `CONTEXT.md` via `/domain-modeling` skill
4. If user rejects a candidate with a load-bearing reason, offer to record as an ADR

**HTML report content is fully designed** — all 7 candidates with before/after Mermaid diagrams, recommendation badges, and top recommendation section are ready. Just the write mechanism needs to change.

**Key question for grilling (whichever candidate is chosen):**
- For B (most likely top pick): How should the compositor handle repository-not-found at startup? Dialog to browse, or error state? Should multiple repositories be supported in the initial version?
- For A: Should `graph_layout` be a new crate in the workspace, or a module inside `core_domain`? Is the intent to eventually support multiple layout algorithms?
</next_steps>