<overview>
Gregory Mifsud (principal technical architect) wants to evolve GitCthulhu from a proof-of-concept into a production-grade application. The `improve-codebase-architecture` skill was invoked, which requires: reading `CONTEXT.md` and ADRs first, organically exploring the codebase for architectural friction (shallow modules, leaky seams, testability gaps), then generating a self-contained HTML report of deepening candidates, and finally running a grilling loop on the user's chosen candidate.
</overview>

<history>
1. User requested invocation of the `improve-codebase-architecture` skill, with context that GitCthulhu was a PoC needing to become production-ready.
   - Skill loaded successfully; instructions prescribe `/codebase-design` vocabulary, organic exploration, HTML report in `%TEMP%`, then grilling loop.
   - Session named "Architecture review"; branch left as `gmifsud-refactored-waddle`.

2. Began codebase exploration after context compaction.
   - Confirmed no `CONTEXT.md` exists and no `docs/adr/` directory exists — no domain glossary or ADRs to read.
   - Discovered project structure: Rust/Tauri workspace (`Cargo.toml`) with 8 crates + TypeScript frontend in `src/`.
   - The README is actually a placeholder from Google AI Studio — not the real project README.
   - Read all 8 crate `lib.rs` / `main.rs` files: `core_domain`, `git_adapter`, `gui_app`, `auth_adapter`. Still need: `ssh_adapter`, `config_adapter`, `cache_adapter`, `keyring_adapter`.
   - `src/` (frontend) only has `App.tsx`, `index.css`, `main.tsx` — skeleton Tauri frontend.
</history>

<work_done>
Files updated: None — no changes made yet.

Work completed:
- [x] Loaded `improve-codebase-architecture` skill
- [x] Confirmed no `CONTEXT.md` or `docs/adr/` exists
- [x] Read `Cargo.toml` workspace manifest
- [x] Read `core_domain/src/lib.rs` (full domain model)
- [x] Read `git_adapter/src/lib.rs` (GitoxideAdapter implementation)
- [x] Read `gui_app/src/main.rs` (iced app, full UI + state wiring)
- [x] Read `auth_adapter/src/lib.rs` (LocalOAuthServer)

Work not yet completed:
- [ ] Read `ssh_adapter/src/lib.rs`
- [ ] Read `config_adapter/src/lib.rs`
- [ ] Read `cache_adapter/src/lib.rs`
- [ ] Read `keyring_adapter/src/lib.rs`
- [ ] Read `src/App.tsx`, `src/main.tsx` (Tauri frontend)
- [ ] Read all `Cargo.toml` files per crate (to understand dependencies)
- [ ] Generate HTML architecture review report in `%TEMP%`
- [ ] Open report and ask user which candidate to explore
- [ ] Run grilling loop on chosen candidate
- [ ] Create/update `CONTEXT.md` via `/domain-modeling`
</work_done>

<technical_details>
**Architecture overview (from files read so far):**
- Rust Cargo workspace with 8 crates using a hexagonal/ports-and-adapters pattern
- `core_domain` is the pure domain — zero external framework dependencies, defines ports as Rust `trait`s
- Adapters implement those traits: `git_adapter` (gitoxide/gix), `auth_adapter` (OAuth loopback), `keyring_adapter` (credential storage), `ssh_adapter`, `config_adapter`, `cache_adapter`
- `gui_app` is the iced desktop UI — it hardwires `GitoxideAdapter::new(".")` and uses `AppState` from `core_domain`
- Frontend in `src/` is a skeleton Tauri setup (React/TSX) — seems to coexist with the iced backend, which is unusual

**Key friction already visible:**

1. **`core_domain` is shallow and overloaded** — it conflates: domain models (`Commit`, `Branch`, `RepoStatus`), port traits (`GitRepository`, `CredentialStore`, `SshManager`, `OAuthServer`, `ConfigStore`, `GraphCache`), application state machine (`AppState`), view events (`ViewEvent`), user commands (`UserCommand`), and even a parallel DAG builder (`build_dag`). This is a God module masquerading as a domain.

2. **`build_dag` is a stub in `core_domain`** — uses `index % 3` for lane assignment, pulls in `rayon` for parallelism that isn't implemented. DAG layout logic is a complex, testable algorithm that belongs in its own deep module.

3. **`gui_app` directly holds `GitoxideAdapter`** — the port exists (`GitRepository` trait) but the adapter is hardwired into the UI struct (`adapter: GitoxideAdapter`). The seam exists on paper but is bypassed in practice.

4. **`AppState::handle_token_received` calls `store.save_token`** — authentication side-effects are inside the state machine. The state machine should be pure (event-in → state-out); side effects belong in a command handler.

5. **`GitRepository::fetch` takes `&dyn CredentialStore` and `fetch_ssh` takes `&dyn SshManager`** — two separate fetch methods with different auth protocols leak the transport abstraction into the port. The port knows too much about how credentials are sourced.

6. **All adapter implementations are stubs** — `status()`, `log()`, `branches()`, `checkout_branch()` return empty vecs. The entire git_adapter is scaffolding. Only `open_repo()` and the single test actually exercise gix.

7. **`gui_app` has hardcoded branch names** in the view (`"main"`, `"feature/iced-ui"`) — not connected to `self.state.branches` at all.

8. **Possible framework conflict**: `Cargo.toml` mentions iced for `gui_app`, but `src/` is a React/Tauri frontend. Need to verify if these are parallel experiments or if one is the real target.

**Skill vocabulary in use:** module, interface, depth, seam, adapter, leverage, locality.
**No `CONTEXT.md` exists** — will need to create it as part of the grilling/domain-modeling phase.
**No ADRs exist** — no prior decisions to respect or avoid re-litigating.
</technical_details>

<important_files>
- `Cargo.toml` (workspace root)
  - Defines 8-crate workspace; establishes the module decomposition strategy
  - No changes made

- `core_domain/src/lib.rs`
  - Central to everything — defines all ports (traits) and domain models; also contains `AppState` (state machine), `ViewEvent`, `UserCommand`, `build_dag`
  - **Primary friction candidate**: too wide, conflates multiple concerns
  - Lines 74–154: all port traits; Lines 175–245: `AppState` state machine; Lines 26–45: `build_dag` stub

- `git_adapter/src/lib.rs`
  - Implements `GitRepository` for gitoxide; all methods are stubs except `open_repo()` and the test
  - Lines 82–111: `fetch`/`fetch_ssh` with split auth protocol seam
  - Lines 114–133: only real test in the codebase

- `gui_app/src/main.rs`
  - Iced desktop UI; holds `GitoxideAdapter` directly (bypasses the `GitRepository` seam)
  - Lines 53–57: `GitApp` struct with hardwired adapter; Lines 160–163: hardcoded branch names
  - Lines 99–103: `subscription()` returns `Subscription::none()` — config watching not wired

- `auth_adapter/src/lib.rs`
  - OAuth loopback server stub; all logic commented out; no real HTTP server
  - Demonstrates the adapter skeleton pattern clearly

- `src/App.tsx`, `src/main.tsx`
  - Tauri/React frontend skeleton — need to read to understand framework conflict with iced
</important_files>

<next_steps>
Remaining reads before generating the report:
1. Read `ssh_adapter/src/lib.rs`, `config_adapter/src/lib.rs`, `cache_adapter/src/lib.rs`, `keyring_adapter/src/lib.rs` (all in parallel)
2. Read `src/App.tsx` and `src/main.tsx` to resolve the iced-vs-Tauri framework question
3. Read per-crate `Cargo.toml` files for `gui_app` and `core_domain` at minimum — need to confirm rayon, gix, iced versions and whether tokio is in the dep tree

Then:
4. Write HTML architecture review report to `%TEMP%\architecture-review-<timestamp>.html` using Tailwind CDN + Mermaid CDN
5. Open with `start <path>` on Windows; provide absolute path
6. Ask user: "Which of these would you like to explore?"
7. Run `/grilling` skill on chosen candidate
8. Create `CONTEXT.md` via `/domain-modeling` as decisions crystallize

**Top friction candidates identified so far:**
- A. `core_domain` God module → split into domain models, ports, state machine, and DAG module
- B. DAG layout algorithm as a deep, testable standalone module
- C. `gui_app` seam bypass (hardwired `GitoxideAdapter`)
- D. `AppState` impurity (side-effect in state machine)
- E. Split fetch protocol seam (`fetch` vs `fetch_ssh`)
- F. Framework question (iced vs Tauri/React) — may be the most consequential architectural decision of all
</next_steps>