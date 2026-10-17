<overview>
Gregory Mifsud (principal technical architect) is evolving GitCthulhu from a Rust/iced proof-of-concept Git GUI into a production-grade application. The `improve-codebase-architecture` skill was invoked to surface architectural friction and propose deepening opportunities. The approach: explore all 8 crates organically, identify shallow modules and leaky seams, produce an HTML architecture review report, then run a grilling loop on the user's chosen candidate.
</overview>

<history>
1. User invoked `improve-codebase-architecture` skill with context that GitCthulhu was a PoC needing to become production-ready.
   - Skill loaded and instructions read.
   - Session renamed to "Architecture review"; branch renamed to `gmifsud-architecture-review`.
   - Confirmed no `CONTEXT.md` or `docs/adr/` directory exists — nothing to pre-read.

2. Full codebase exploration across all 8 crates + `src/App.tsx`.
   - Read `Cargo.toml` workspace, all crate `lib.rs`/`main.rs` files, per-crate `Cargo.toml` for `core_domain`, `git_adapter`, `gui_app`, `cache_adapter`.
   - Critical discovery: `src/App.tsx` is a React/Vite "sandboxed web preview" from Google AI Studio — NOT a real Tauri frontend. The actual app is the iced GUI in `gui_app/`. The `README.md` is also the AI Studio template.
   - 7 architectural friction candidates identified (A–G).

3. HTML architecture review report authoring — blocked, then unblocked.
   - First attempt via PowerShell inline here-string failed with `ENAMETOOLONG` — multi-KB HTML too large for a single command argument.
   - Solved by using the `create` tool to write the file directly to `%TEMP%`.
   - Report written to `C:\Users\grego\AppData\Local\Temp\architecture-review-20260630-082536.html` (35,501 chars).
   - Opened in browser via `Start-Process`.
</history>

<work_done>
Files created:
- `C:\Users\grego\AppData\Local\Temp\architecture-review-20260630-082536.html` — full self-contained HTML architecture review report (Tailwind CDN + Mermaid CDN), 35,501 chars. NOT in the repo.

No source files changed. All work is exploration and report generation.

Work completed:
- [x] Loaded `improve-codebase-architecture` skill
- [x] Confirmed no `CONTEXT.md` or `docs/adr/` exists
- [x] Read all 8 crate source files
- [x] Identified 7 architectural candidates (A–G)
- [x] Renamed session to "Architecture review" and branch to `gmifsud-architecture-review`
- [x] Written HTML report to `%TEMP%` and opened in browser
- [ ] Ask user which candidate to explore (grilling loop not yet started)
- [ ] Run `/grilling` skill on chosen candidate
- [ ] Create `CONTEXT.md` as decisions crystallise
</work_done>

<technical_details>
**Workspace structure:**
- Rust Cargo workspace, hexagonal/ports-and-adapters pattern — 8 crates: `core_domain`, `git_adapter`, `gui_app`, `auth_adapter`, `config_adapter`, `cache_adapter`, `keyring_adapter`, `ssh_adapter`

**`core_domain/src/lib.rs` — God module:**
- Claims "zero-dependency" but has `rayon` + `serde`
- Defines ALL port traits + domain models + `AppState` state machine + `ViewEvent` + `UserCommand` + `build_dag` algorithm stub (3 lines, `index % 3` lane assignment)
- `AppState` spans 10+ fields covering lifecycle, repo data, auth flow, settings, DAG geometry
- `handle_token_received` calls `store.save_token()` — side effect inside state machine = not pure

**`gui_app/src/main.rs` — seam bypassed:**
- `GitApp { adapter: GitoxideAdapter }` — concrete type, not `Box<dyn GitRepository>`
- `Default::default()` hardwires `GitoxideAdapter::new(".")`
- `gui_app/Cargo.toml` depends directly on `git_adapter` — bypasses the port
- Lines 160–163: hardcoded branch names `"main"`, `"feature/iced-ui"`
- `subscription()` returns `Subscription::none()` — config/file watching not wired

**`config_adapter/src/lib.rs` — compile error:**
- `fn From` (capital F) on lines 23 and 32 — should be `fn from` (lowercase)

**`cache_adapter/src/lib.rs` — orphaned:**
- redb + bincode, functional, but **never wired up** in `gui_app`

**`auth_adapter/src/lib.rs` — pure stub:**
- All logic commented out

**`src/App.tsx` — dead weight:**
- React/Vite AI Studio web preview. Footer: "ENGINE: VITE/REACT", "OS: WEB PREVIEW"
- NOT a Tauri frontend. Framework decision (iced vs Tauri) is unresolved — Candidate G in report.

**PowerShell ENAMETOOLONG limitation:**
- Passing multi-KB strings as inline here-string arguments to `powershell` tool fails with ENAMETOOLONG.
- Workaround: use the `create` tool to write directly to the target path.

**7 Candidates ranked:**
- **Strong:** A (graph_layout stub), B (gui_app seam bypass), C (AppState God state machine)
- **Worth exploring:** D (DomainError::Unknown everywhere), E (fetch seam leaks auth protocol), F (SshManager wide port)
- **Speculative:** G (React src/ web preview — framework decision needed first)

**Recommended order:** B → C → A → D/E/F
- B first because it converts the hypothetical seam to a real one and enables GUI testing
- A is the core business value (graph layout) but needs B+C in place to test properly

**Unanswered questions for grilling:**
- B: How should compositor handle repository-not-found at startup? Dialog or error state? Multiple repos in v1?
- A: New `graph_layout` crate or module inside `core_domain`? Multiple layout algorithms planned?
- G: iced vs Tauri — the framework question must be settled before architectural energy is spent on `src/`
</technical_details>

<important_files>
- `core_domain/src/lib.rs`
  - Central to everything — defines ALL port traits, domain models, AppState state machine, ViewEvent, UserCommand, build_dag
  - Primary friction: too wide; conflates domain models + ports + state machine + algorithm + events
  - Lines 74–154: port traits; Lines 175–245: AppState state machine; Lines 26–45: build_dag stub
  - No changes made

- `gui_app/src/main.rs`
  - iced desktop UI; holds `GitoxideAdapter` directly (bypasses the `GitRepository` seam)
  - Lines 53–57: `GitApp` struct with hardwired `adapter: GitoxideAdapter`
  - Lines 160–163: hardcoded branch names
  - Line 99–103: `subscription()` returns `Subscription::none()`
  - No changes made

- `gui_app/Cargo.toml`
  - Critical: depends on `git_adapter = { path = "../git_adapter" }` — this is the seam violation (Candidate B)
  - No changes made

- `git_adapter/src/lib.rs`
  - Implements `GitRepository`; all methods are stubs except `open_repo()` and one test
  - Lines 82–111: split fetch seam — `fetch(&dyn CredentialStore)` vs `fetch_ssh(&dyn SshManager)`
  - No changes made

- `config_adapter/src/lib.rs`
  - Has compile error: `fn From` (capital F) lines 23 and 32 — should be `fn from` (lowercase)
  - No changes made

- `src/App.tsx`
  - AI Studio web preview — NOT a Tauri frontend, confirmed dead weight
  - Footer: "ENGINE: VITE/REACT", "OS: WEB PREVIEW"
  - No changes made

- `C:\Users\grego\AppData\Local\Temp\architecture-review-20260630-082536.html`
  - The architecture review report — 7 candidates with Mermaid diagrams, before/after visuals, recommendation badges, top-recommendation section
  - Created this session; opened in browser
</important_files>

<next_steps>
Immediate next step:
- Ask Gregory: **"Which of the 7 candidates would you like to explore?"** — the report is open in his browser.

After he picks a candidate, run the `/grilling` skill loop:
- For **B (likely top pick):** challenge around compositor startup behaviour (repo not found, multiple repos), test strategy, where the composer module lives, whether `gui_app` retains any adapter knowledge at all.
- For **A:** challenge around whether `graph_layout` warrants a new crate or a module, algorithm requirements (lane-colouring, merge commits, octopus merges), property test strategy.
- For **C:** challenge around the `SideEffect` enum shape, which effects are synchronous vs queued, whether `AppPhase` / `RepoSession` / `AuthFlow` are all in `core_domain` or split.

As decisions crystallise:
- Create `CONTEXT.md` lazily (via `/domain-modeling` skill) when domain terms get named.
- If a candidate is rejected with a load-bearing reason, offer to record as an ADR.

Blocker to note: All adapters except `keyring_adapter` are stubs (and `config_adapter` has a compile error). Architecture changes B/C/A will not produce a running app until real git operations (`status`, `log`, `branches`) are implemented in `git_adapter`. Architecture refactoring and adapter implementation should proceed in parallel.
</next_steps>