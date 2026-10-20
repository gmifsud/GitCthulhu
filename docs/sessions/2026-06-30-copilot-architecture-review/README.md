# Copilot session — Architecture review & Candidate B (30 June 2026)

Recovered from the old laptop's backup (`F:\Users\grego\.copilot\session-state\d79b1a9e-…`) on 17 October 2026.
This is the only GitHub Copilot session that did GitCthulhu work — none were found in VS Code, VS Code Insiders or WebStorm.

**In one line:** Copilot reviewed the whole PoC, wrote an architecture report with seven refactor candidates (A–G), grilled you through Candidate B (stop `gui_app` depending on the concrete git adapter), then implemented and committed it.

| | |
|---|---|
| Session ID | `d79b1a9e-8a31-4536-8f65-59b30a1d7942` ("Architecture review", later "Candidate B seam fix") |
| When | 2026-06-30, 08:23 → 13:36 CEST (≈24 min of model time across 5 hours) |
| Tool | GitHub Copilot desktop app, agent 1.0.66, model `claude-sonnet-4.6`, reasoning high |
| Skills used | `improve-codebase-architecture`, `grilling` (restored to `C:\Users\grego\.agents\skills\`) |
| Branch | `gmifsud-architecture-review` (worktree folder still named `gmifsud-refactored-waddle`) — now pushed to GitHub |
| Commits | `714bcff` docs: architecture review report · `e90f110` feat: invert gui_app seam (Candidate B) |
| Base | `83b4c09` on `main` |
| Size of change | +930 / −224 lines, 9 files |

## Files in this folder

- **README.md** — this summary.
- **[TRANSCRIPT.md](TRANSCRIPT.md)** — the full conversation: your messages, Copilot's replies and reasoning, every question it asked you with your answer, and every tool call (code edits shown in full, tool output truncated).
- **[checkpoints/](checkpoints/index.md)** — Copilot's own 16 progress summaries, written each time it compacted context. [016](checkpoints/016-candidate-b-seam-fix-committed.md) is the final state.
- The report itself is committed at [`docs/architecture-review-20260630.html`](../../architecture-review-20260630.html) — open it in a browser (it uses Tailwind + Mermaid from CDNs).

## Timeline

| Time | What happened |
|---|---|
| 08:23 | You asked Copilot to run `improve-codebase-architecture`: *"this application was done more as a proof of concept… take it to an actual application able to handle real life scenarios."* |
| 08:24–08:30 | Explored all 8 crates and `src/`. No `CONTEXT.md` or ADRs existed. |
| 08:32 | HTML report written (first to `%TEMP%`). Asked which candidate to explore. You replied: put generated docs in the repo, create a `docs` folder. |
| 08:34 | Report moved to `docs/` and committed (`714bcff`). Asked which candidate to grill. |
| ~09:58 | You picked **Candidate B**. The `grilling` skill was loaded. |
| 09:59–12:23 | Five design questions, answered one by one (see below). |
| 13:08 | Copilot summarised the decisions; you answered **"Yes"** to implement. |
| 13:12–13:24 | Built `test_support`, split `gui_app` into lib + bin, fixed the `config_adapter` compile error, cleaned warnings. `cargo build` succeeded. |
| 13:25–13:27 | Commit blocked (missing `target/` in `.gitignore`, stale `index.lock`); fixed; committed `e90f110`. |
| 13:36 | Session closed normally. |

## What the review found

Seven candidates, from the report:

| | Candidate | Strength | The friction |
|---|---|---|---|
| **A** | `graph_layout` module | Strong | `build_dag` in `core_domain` is a 3-line stub (`index % 3` lanes). The commit graph is the core product value and has no real home. |
| **B** | `gui_app` seam bypass | Strong | `GitApp` held a concrete `GitoxideAdapter` built with `Default` on `"."`, so the `GitRepository` port was bypassed and the GUI couldn't be tested. **← done in this session** |
| **C** | `AppState` decomposition | Strong | `core_domain/src/lib.rs` (246 lines) holds every port, model, the state machine, events and the DAG stub. `handle_token_received` performs I/O inside the state machine. Proposal: a pure state machine plus a `SideEffect` enum. |
| **D** | `DomainError` taxonomy | Worth exploring | `DomainError::Unknown` everywhere, so callers can't react meaningfully. |
| **E** | Fetch seam | Worth exploring | `fetch(&dyn CredentialStore)` vs `fetch_ssh(&dyn SshManager)` leaks the auth protocol into the port. Proposal: a single `GitCredential` enum. |
| **F** | `SshManager` port | Worth exploring | Port is wider than its callers need. |
| **G** | React/iced question | Speculative | `src/App.tsx` is the Google AI Studio web preview, **not** a Tauri front end. The real app is the iced GUI in `gui_app/`. Decide which UI you're keeping before investing in `src/`. |

Recommended order: **B → C → A → D/E/F**, with G settled whenever you're ready.

Other facts it established:
- `config_adapter` didn't compile (`fn From` instead of `fn from`, lines 23 and 32). **Fixed in `e90f110`.**
- `cache_adapter` (redb + bincode) works but is never wired in.
- `auth_adapter` is a stub (logic commented out). `git_adapter` methods are stubs apart from `open_repo()`.
- `gui_app` had hard-coded branch names (`"main"`, `"feature/iced-ui"`) and no subscriptions.
- `README.md` is still the AI Studio template.

## Candidate B — your decisions

| # | Question | Your answer | What was built |
|---|---|---|---|
| 1 | How should the app open a repo at startup? | Remember last repo, fall back to welcome screen | ⚠️ **Partly.** `main.rs` takes a path from the command line, otherwise shows the welcome screen. "Remember last repo" is a `TODO: [DEBT]` waiting for `config_adapter::load_settings().last_repo_path`. |
| 2 | Should `gui_app` depend on `git_adapter`? | Full inversion — `gui_app` drops the dependency | ⚠️ **Source-level only.** `lib.rs` never imports `git_adapter`, but `gui_app/Cargo.toml` still lists it, because the binary in the same crate needs it. A Cargo feature or a separate binary crate would make it real. |
| 3 | Where does the compositor live? | Split `gui_app` into `lib.rs` + `main.rs` | ✅ `main.rs` is now 13 lines and the only file importing `git_adapter`. |
| 4 | Sync or async port? | Sync trait, wrapped at `Task::perform` | ✅ Trait stays sync. The async wiring is not done: every `update()` arm returns `Task::none()` with `TODO: [DEBT] Wire spawn_blocking`. |
| 5 | Where does the fake live? | Shared `test_support` crate | ✅ `test_support::FakeGitRepository` exists. ⚠️ **No test uses it yet.** |

Shape of the result: `GitApp.adapter: Option<Box<dyn GitRepository + Send>>` (`None` = welcome screen). Public entry points: `gui_app::run(repo)` and `gui_app::run_welcome()`. Startup state is injected with iced 0.13's `.run_with(...)`, because `.run()` requires `Default`.

## Things worth reviewing before you build on it

1. **Decision 2 wasn't fully honoured.** You chose "drops the dependency", but the crate still depends on `git_adapter`. Decide whether source-level isolation is enough, or move the binary into its own crate (the alternative Copilot offered for Q3).
2. **The app doesn't do anything yet.** With the async wiring deferred and `git_adapter` mostly stubs, opening a repo shows no data. Real `status`/`log`/`branches` are the next functional step.
3. **The safety net isn't in place.** `FakeGitRepository` was the reason for decision 5, but there are no `gui_app` tests using it.
4. **`Cargo.lock` is now committed** (6,600 lines). That's the right call for a binary, but worth knowing.
5. **Not re-verified on this machine.** The build was green on the old laptop; Rust (`cargo`) isn't installed here yet.

### Follow-up — 20 October 2026

Items 1, 3 and 5 above are closed:

- **Isolation is now compiler-enforced.** The binary moved to a new `app` crate (the composition root), which builds `git_cthulhu` and depends on `gui_app` + `git_adapter`. `gui_app` is library-only and its `Cargo.toml` lists no adapter crates; `cargo tree -p gui_app` shows no `git_adapter` or `gix`. This replaces decision 3 (lib + bin in one crate), which couldn't coexist with decision 2.
- **`FakeGitRepository` is in use.** `gui_app` has a synchronous `load_repository(&dyn GitRepository) -> Vec<ViewEvent>` that reports status, log and branches independently. Eight tests cover it on success, single-failure and repo-missing paths, the fake → events → `AppState` chain, and `update()` state transitions.
- **Verified on this machine** with Rust 1.99.1 (MSVC): workspace builds, all tests pass.

Item 2 is half closed: `RefreshRepository` now runs `load_repository` on a worker thread via `Task::perform`, and the app loads the repository as soon as the window opens. Overlapping refreshes are ignored, and a crashed adapter is reported as an error rather than leaving the view stuck on "Loading…". The adapter is now shared as `Arc<dyn GitRepository + Send + Sync>`. **The view will still be empty against a real repo**, because `git_adapter`'s `status`, `log` and `branches` are stubs that return empty lists.

## Next steps (Copilot's list, in its order)

1. Open a PR for `gmifsud-architecture-review` → `main`.
2. Run `domain-modeling` to name the new terms (compositor, adapter port, welcome screen) and start `CONTEXT.md`.
3. Write an ADR recording the five Candidate B decisions.
4. Grill **Candidate C** (`AppState` decomposition) next.
5. ~~Replace the `Task::none()` stubs with real `Task::perform` / `spawn_blocking` calls~~ — done for `RefreshRepository`; `CheckoutBranch` and `OpenRepository` remain.
6. ~~Implement `status`, `log` and `branches` in `git_adapter` with gix~~ — done; the app now shows real branches and history. Still placeholders in the view: commit lanes (`build_dag` stub, Candidate A) and the "Staged Changes" panel, which doesn't read `RepoStatus` yet.

## Picking the session back up

The session folder has been copied back into the Copilot app's store at
`C:\Users\grego\.copilot\session-state\d79b1a9e-8a31-4536-8f65-59b30a1d7942`. It's identical to the backup, except that `workspace.yaml` now points at the D: worktree and the real branch name. The worktree's git link was repaired, so it works on this machine.

To resume it with its full history, run this from the worktree folder:

```powershell
& "$env:LOCALAPPDATA\github-copilot-sdk\cli\1.0.99-2\copilot.exe" --resume=d79b1a9e
```

The session was made with agent 1.0.66 and this machine runs 1.0.99. If it won't resume, or doesn't show in the desktop app's list, start a fresh session and give it this README plus [checkpoint 016](checkpoints/016-candidate-b-seam-fix-committed.md). Together they hold everything the old session knew.

The original on `F:\` is untouched.
