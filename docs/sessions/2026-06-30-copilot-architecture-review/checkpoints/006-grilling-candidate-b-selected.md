<overview>
Gregory Mifsud (principal technical architect) is evolving GitCthulhu from a Rust/iced proof-of-concept Git GUI into a production-grade application. The `improve-codebase-architecture` skill was invoked to surface architectural friction. After a full codebase exploration across all 8 crates, a self-contained HTML architecture review report was generated with 7 candidates, committed to `docs/`. Gregory selected **Candidate B — gui_app seam bypass** to grill first. The `grilling` skill has been loaded and is ready to begin the interview.
</overview>

<history>
1. User invoked `improve-codebase-architecture` skill to take GitCthulhu from PoC to production-ready.
   - Skill loaded; session renamed to "Architecture review"; branch renamed to `gmifsud-architecture-review`.
   - Confirmed no `CONTEXT.md` or `docs/adr/` directory exists.

2. Full codebase exploration across all 8 crates.
   - Read `Cargo.toml` workspace, all crate `lib.rs`/`main.rs` files, per-crate `Cargo.toml`.
   - Critical discovery: `src/App.tsx` is a React/Vite AI Studio web preview — NOT a Tauri frontend. Actual app is the iced GUI in `gui_app/`.
   - 7 architectural friction candidates identified (A–G).

3. HTML architecture review report generated.
   - PowerShell inline here-string failed with `ENAMETOOLONG`; workaround: used `create` tool to write directly to `%TEMP%`, then copied to `docs/`.
   - Report written to `docs/architecture-review-20260630.html`.
   - Committed to branch `gmifsud-architecture-review` (commit `714bcff`).

4. User selected **Candidate B — gui_app seam bypass** to grill first.
   - `grilling` skill loaded and ready.
   - Grilling interview has NOT yet started — no questions asked yet.
</history>

<work_done>
Files created:
- `docs/architecture-review-20260630.html` — committed at `714bcff`

Work completed:
- [x] Loaded `improve-codebase-architecture` skill
- [x] Confirmed no `CONTEXT.md` or `docs/adr/` exists
- [x] Read all 8 crate source files
- [x] Identified 7 architectural candidates (A–G)
- [x] Renamed session ("Architecture review") and branch (`gmifsud-architecture-review`)
- [x] Written HTML report and committed to `docs/`
- [x] User selected Candidate B for grilling
- [x] Loaded `grilling` skill
- [ ] **Grilling interview for Candidate B — NOT YET STARTED**
- [ ] Run `/domain-modeling` skill as decisions crystallise
- [ ] Create `CONTEXT.md` lazily
- [ ] ADR for any rejected candidates with load-bearing reasons
</work_done>

<technical_details>
**Workspace structure:**
- Rust Cargo workspace, hexagonal/ports-and-adapters pattern — 8 crates: `core_domain`, `git_adapter`, `gui_app`, `auth_adapter`, `config_adapter`, `cache_adapter`, `keyring_adapter`, `ssh_adapter`

**Candidate B — gui_app seam bypass (the selected candidate):**
- `gui_app/src/main.rs`: `GitApp { adapter: GitoxideAdapter }` — concrete type, not `Box<dyn GitRepository>`
- `gui_app/Cargo.toml` depends directly on `git_adapter = { path = "../git_adapter" }` — this bypasses the port defined in `core_domain`
- `gui_app/src/main.rs` lines 53–57: `GitApp` struct with hardwired `GitoxideAdapter`
- `gui_app/src/main.rs` lines 160–163: hardcoded branch names `"main"`, `"feature/iced-ui"`
- `gui_app/src/main.rs` line 99–103: `subscription()` returns `Subscription::none()` — config/file watching not wired
- Fix: remove `git_adapter` dep from `gui_app/Cargo.toml`, inject `Box<dyn GitRepository>` at startup in `main()`
- This turns the hypothetical seam into a real one and enables GUI testing with a fake adapter

**Other key findings:**
- `core_domain/src/lib.rs` — God module: defines ALL port traits + domain models + `AppState` state machine + `ViewEvent` + `UserCommand` + `build_dag` algorithm stub. `handle_token_received` calls `store.save_token()` — side effect inside state machine (not pure).
- `config_adapter/src/lib.rs` — compile error: `fn From` (capital F) on lines 23 and 32, should be `fn from`
- `cache_adapter/src/lib.rs` — functional but never wired in `gui_app`
- `auth_adapter/src/lib.rs` — all logic commented out
- `src/App.tsx` — AI Studio web preview, dead weight, NOT a Tauri frontend

**PowerShell ENAMETOOLONG limitation:**
- Passing multi-KB strings as inline arguments to the `powershell` tool fails with ENAMETOOLONG.
- Workaround: use the `create` tool to write directly to the target path.

**Branch state:** Branch is `gmifsud-architecture-review` (renamed from `gmifsud-refactored-waddle`).

**7 Candidates ranked:**
- **Strong:** A (graph_layout stub), B (gui_app seam bypass), C (AppState God state machine)
- **Worth exploring:** D (DomainError::Unknown everywhere), E (fetch seam leaks auth protocol), F (SshManager wide port)
- **Speculative:** G (React src/ web preview — framework decision needed first)
- **Recommended order:** B → C → A → D/E/F
</technical_details>

<important_files>
- `docs/architecture-review-20260630.html`
  - Primary deliverable; 7 candidates with Mermaid diagrams, before/after visuals, recommendation badges
  - Created and committed this session at commit `714bcff`

- `core_domain/src/lib.rs`
  - ALL port traits, domain models, AppState, ViewEvent, UserCommand, build_dag — God module
  - Lines 74–154: port traits; Lines 175–245: AppState state machine; Lines 26–45: build_dag stub
  - No changes made

- `gui_app/src/main.rs`
  - iced desktop UI; holds `GitoxideAdapter` directly (seam bypass — subject of Candidate B grilling)
  - Lines 53–57: `GitApp` struct with hardwired `adapter: GitoxideAdapter`
  - Lines 160–163: hardcoded branch names
  - Lines 99–103: `subscription()` returns `Subscription::none()`
  - No changes made

- `gui_app/Cargo.toml`
  - Depends on `git_adapter = { path = "../git_adapter" }` — the seam violation for Candidate B
  - No changes made

- `config_adapter/src/lib.rs`
  - Compile error: `fn From` (capital F) lines 23 and 32
  - No changes made
</important_files>

<next_steps>
Immediate next step:
- **Begin the grilling interview for Candidate B (gui_app seam bypass).** The skill is loaded; start asking the first grilling question now.

Key grilling angles for Candidate B:
1. **Startup compositor**: where does `main()` compose the concrete adapter — inline in `gui_app/main.rs` or a separate `compositor` module? Who owns the `Box<dyn GitRepository>` lifetime?
2. **Multiple repos**: does `GitApp` ever need to switch repos at runtime (workspace with multiple roots), or is one repo per process?
3. **Test strategy**: fake adapter in a `tests/` integration module within `gui_app`, or a separate `test_support` crate shared across crates?
4. **Async**: `GitoxideAdapter` operations are blocking — does the seam expose `async fn` or use `iced::Command` + `tokio::spawn` bridge?
5. **Adapter knowledge in gui_app**: after the fix, does `gui_app/Cargo.toml` retain any reference to `git_adapter` at all, or does it vanish entirely?

As decisions crystallise:
- Run `/domain-modeling` skill to name and record new domain terms in `CONTEXT.md`
- Offer ADR for any rejected design with a load-bearing reason
</next_steps>