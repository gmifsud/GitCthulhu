# GitDesktop → GitCthulhu transfer brief

Status: Living reference. Captures how [gmifsud/GitDesktop](https://github.com/gmifsud/GitDesktop) implements UI and features so we carry **behaviour and contracts** into this repo — not the Tauri/React stack.

**Shipped on the binary (2026-10-25):** inspector section/folder collapse; commit files-changed → Diff focus; graph relative age + WIP row; unified Diff line numbers; status-bar expandable/dismissible errors + ahead/behind; keyboard Esc / F5 / Ctrl+R / Ctrl+S / Ctrl+Shift+L.

Local clone used for the original analysis: `D:\Repos\_ref_GitDesktop`.  
Layout target for this window: [`three-column-window.md`](./three-column-window.md) and `docs/design/three-column-wireframe.html`.

## Transfer rule

Treat GitDesktop as a **UX and feature-design reference** for a keyboard-first local Git client (changes → commit → sync → history, with safe confirms and rich feedback). Implement those behaviours inside GitCthulhu’s iced four-pane shell and port/reducer architecture (ADR 0001, ADR 0002). Do **not** transplant Tauri, React, TanStack Query, Zustand, or the forge/AI/MCP product surface until the local git loops match.

---

## Stack contrast

| Layer | GitDesktop | GitCthulhu |
|---|---|---|
| Shell | Tauri 2 (`src-tauri/`) | iced 0.13 (`gui_app`) |
| UI | React 19 + Vite + Tailwind/shadcn | Native iced views |
| Server/git state | TanStack Query | `ViewEvent` → `AppState` reducer |
| Session/UI state | Zustand | fields on `GitApp` |
| Git | System `git` via `invoke("git_…")` | `GitRepository` port → `git_adapter` |
| Concurrency | Lock domains in `git/runner.rs` | Background threads in `gui_app/src/jobs.rs` |
| Scope | Local git + multi-forge + agents | Local git (+ optional suggest-commit) |

**GitDesktop entry:** `src/main.tsx` → `App.tsx`; Rust `src-tauri/src/lib.rs` registers commands; git lives under `src-tauri/src/git/`.

**GitCthulhu entry:** `app` binary → `gui_app::run` → `GitApp::{update,view}`; adapter wired only by the composition root.

---

## GitDesktop UI composition

`App.tsx` switches on Zustand `view`:

| View | Role |
|---|---|
| `welcome` | Open / clone / create + recents (`features/welcome/`) |
| `repo` | Main workspace (`features/repository/RepositoryView.tsx`) |
| `settings` / `help` / `explore` / `mywork` | Overlay screens |

Global hosts (always mounted): confirm, clone/create dialogs, command palette, shortcuts sheet, error dialog, toasts.

Repo chrome is **tabbed** (changes, history, compare, pulls, issues, …). The local loop that matters for us is **Changes**: left file list + CommitBox beside a lazy DiffSurface — not a permanent graph column. History and compare are separate tabs.

GitCthulhu’s shell is already closer to GitLens: top bar → `pane_grid` (`Graph | Inspector | Diff | Files`) → terminal → status bar (`gui_app/src/view/`, `layout.rs`). Keep that shell; borrow GitDesktop’s **behaviour** for changes/sync/safety.

---

## Feature → implementation map (GitDesktop)

| Feature | UI | Frontend API | Rust |
|---|---|---|---|
| Open / clone / create | `features/welcome/*` | `lib/git/api/repo.ts` | `git/repo.rs` |
| Status / stage | `repository/ChangesPanel.tsx` | `api/status.ts`, `workingtree.ts` | `status.rs`, `stage.rs` |
| Commit / amend | `commit/CommitBox`, `useCommitSubmit` | `gitCommit` | `commit.rs` |
| Diff | `features/diff/*` | `git_diff_file`, … | `diff.rs` |
| Branches | Branch switcher + dialogs | `api/branches.ts` | `branches.rs` |
| Sync | `SyncControls.tsx` | `api/sync.ts` (ff / rebase / merge, autostash) | ops + `pull_guard`, `autostash` |
| History | `history/HistoryPanel` | `api/history.ts` | `history.rs` |
| Stash / worktrees / compare / conflicts | matching `features/*` | matching APIs | matching `git/*.rs` |
| Shortcuts / palette | `hotkeys/registry.ts`, command palette | — | — |

### Typical commit flow (GitDesktop)

1. Draft in Zustand.  
2. Mutation → `invoke("git_commit", { repoPath, title, body, amend, expectedHead, … })`.  
3. Rust acquires working-tree lock, runs git, returns structured result / `AppError`.  
4. Invalidate Query keys; toast on success; short toast + Details dialog on failure.

### Same intent in GitCthulhu

1. `Message::EditWorktree(WorktreeEdit::Commit(msg))`.  
2. `jobs::edit_worktree_in_background` → port `commit`.  
3. Refresh via `load_repository` → `ViewEvent`s into the reducer.  
4. Errors today are a string on the app / status bar — not a typed toast + details stack.

---

## Patterns to carry (contracts, not widgets)

1. **Keyboard-first registry** — one catalogue drives palette and shortcuts sheet (`hotkeys/registry.ts`).  
2. **Never surprise on destroy** — shared confirm; cancel on repo switch; centralised copy.  
3. **Disabled with reason** — never silent grey-out.  
4. **Calm errors** — one-line summary + expandable stderr; typed kinds (`conflict`, `busy` with holder, pull-would-drop).  
5. **Rich sync** — pull modes, ahead/behind, last-fetched, publish when no remote, named autostash outcomes.  
6. **Conflict as first-class UI** — banner + file list + continue/abort.  
7. **Changes UX** — virtualised list, tree/flat, **collapse directories**, filter, multi-select, context menu, empty/error retry.  
8. **Diff quality** — hunk/line stage, budgets, resting placeholder while heavy work loads.  
9. **List keyboard nav** — same arrow behaviour on every selectable list.  
10. **Chrome restraint** — repo work is the focus; keep orange `deep_focus` tokens (do not copy GitDesktop’s slate/shadcn look).  
11. **AI as assistant** — stream into the commit field; hideable; never block the manual path.

PRODUCT.md in GitDesktop states the safety and chrome principles; honour the intent, not the CSS.

---

## Gaps vs GitCthulhu today

| Axis | GitDesktop | GitCthulhu today |
|---|---|---|
| Layout | Tabbed sidebar + detail | Four-pane graph / inspector / diff / files (shell landed; depth incomplete) |
| Graph | History tab with rich commit story | Selectable rows; thin metadata; weak “what happened” loop |
| Changes list | Collapsible tree, filters, context menus | List/Tree toggle; directories are labels only — **no collapse** |
| Sync | Multi-mode pull, guards, autostash | Fetch / push / pull `--ff-only` |
| Errors | Toast + Details | Status / header string; full-window confirms |
| Shortcuts | Large registry + palette | No central registry |
| Forge / PR | First-class tabs | Out of scope until local parity |

---

## Immediate product gaps (from live use of `git_cthulhu`)

These are the reasons the binary still feels thin even after the shell improved. They map onto three-column Phases 2–3 and should lead the queue.

### 1. Graph column — interact to learn what happened

**Pain:** Selecting a commit does not tell a clear story. The graph is a list of hash + message + author; the inspector for a commit shows little more than the subject and a “Working tree” escape hatch; the Diff column may load a patch, but there is no “files changed” summary, no relative age, no ref pills, no working-changes row, and no keyboard move through the graph.

**Where it lives today**

- Rows: `gui_app/src/view/history.rs` (`history_pane` → `Message::Select(Commit)`)
- Lanes: `gui_app/src/commit_graph.rs`
- Commit inspector body: `gui_app/src/view/workspace.rs::detail_body` (message only; no file list)
- Diff for selection: `app.rs` patch load + `view/patch.rs`

**Target (carry from three-column Phase 2 + GitDesktop History/Changes)**

- Row layout: lanes · ref pills · message · author · relative age · short hash (`three-column-window.md` Phase 2).
- Working-changes WIP row → `ShowWorkingTree`.
- Selecting a commit switches the inspector to **Commit mode**: meta + **Files changed (n)** with `+n −m`; click a file to focus Diff.
- Keyboard ↑/↓ moves graph selection (Phase 6 in the layout proposal; pull forward if cheap).

**Port notes:** ref pills need `Branch.tip` (or equivalent). File stats can start from loaded patches (`patch_stats`) before `commit_files()` exists.

### 2. Inspector — collapse changes by folder / section

**Pain:** Tree mode still expands every directory as a non-interactive label. Large worktrees force long scrolls through every file. There is no per-section or per-directory collapse.

**Where it lives today**

- Pane: `gui_app/src/view/changes.rs` (`changes_pane`, `change_line`)
- Rows: `gui_app/src/changes.rs` (`change_rows`, `ChangeRow::{Directory, File}`)
- Tree toggle: `Message::SetFileTree` / `files_as_tree`

**Target (carry from GitDesktop ChangesPanel + three-column Phase 3)**

- Directory rows are **buttons** that toggle expand/collapse; collapsed dirs show a count badge and hide descendants.
- Optional: collapse whole sections (Staged / Unstaged / Untracked) the same way.
- Persist collapse state for the session (path → open); reset on repo switch.
- Keep List/Tree; collapse only applies in Tree (and to section headers in both modes if section collapse ships).
- Cap drawn rows remains (`FILE_ROWS_SHOWN`); collapse reduces scroll before that cap matters.

**Smallest first cut:** `Message::ToggleChangeDir { path_prefix, place }` + a `HashSet` (or map) of collapsed prefixes on `GitApp`; filter `change_rows` output (or skip rendering children of collapsed dirs). No port change.

---

## Ordered priorities (revised)

1. ~~Inspector collapse~~ — done.  
2. ~~Graph → story loop~~ — files-changed, age, WIP, **column headers, author, Branch.tip ref pills** done.  
3. Diff affordances — **line numbers done**; side-by-side + word highlights still open (Phase 4).  
4. Sync richness beyond ff-only — port + top bar (ahead/behind chip in status bar done; pull modes still ff-only by design).  
5. Confirm / error UX — **status expand/dismiss done**; modal overlay confirms still open.  
6. Keyboard command map — **starter set done**; grow toward a registry + palette.  
7. Conflict workflow.  
8. Multi-repo / recents.  
9. Forge / PR — explicitly later.

Do not let forge or AI work jump the queue.

---

## What this document does not do

- It does not authorise copying GitDesktop’s UI chrome, colours, or tab model.  
- It does not change ADR 0001 or 0002.  
- It does not replace [`three-column-window.md`](./three-column-window.md); that proposal remains the layout phase plan. This brief is the **behavioural** reference and the **priority override** for graph story + collapse.

## How to use it in a session

1. For layout mechanics → three-column proposal + wireframe.  
2. For “how should this feel / what did GitDesktop do?” → this brief + `_ref_GitDesktop` paths above.  
3. For the next code change → start with priority 1 (collapse) or 2 (graph story), surgically, with `cargo test -p gui_app` and a manual pass on `target/debug/git_cthulhu.exe`.
