# Worktree sessions (WP1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship multi-session git worktree tabs (list / open / close / create / remove) with per-session `AppState`, path-routed job finishes, and a `WorktreeOpener` that does not overwrite `last_repo_path`.

**Architecture:** Each open tab is a `WorktreeSession` owning adapter + `AppState` + UI bag. Catalogue comes from `git worktree list --porcelain`. Tab switch is a pointer move. Composition root supplies `RepositoryOpener` (remembers path) and `WorktreeOpener` (does not). Finish handlers resolve by path. Aligns with ADR 0003 WP1-a…d and `docs/superpowers/specs/2026-10-25-worktree-tabs-design.md`.

**Tech Stack:** Rust, `core_domain` ports, `git_adapter` + system `git`, `gui_app` (iced 0.13), `test_support::FakeGitRepository`, `config_adapter` (confy), `app` composition root.

## Global Constraints

- ADR 0001: `gui_app` never depends on / names `git_adapter`.
- ADR 0002: repository data enters via `AppState::apply_event` only; no scheduling in the reducer.
- ADR 0003: sessions own `AppState`; one tab strip (sessions only); path-routed finishes; `WorktreeOpener` without `remember`.
- Default `WorktreeTabRetention::Partial`; close last tab refused; never remove `is_main` from UI.
- British English in user-facing copy.
- Every task ends green: `cargo test -p <crates touched>`.
- Out of scope: WP2 compare, WP3 split diff, WP4 graph search/avatars (except WP1-d multi WIP rows).

## File map

| File | Responsibility |
|---|---|
| `core_domain/src/model.rs` | `Worktree`, `WorktreeTabRetention` on `AppSettings` |
| `core_domain/src/ports.rs` | `worktrees` / `add_worktree` / `remove_worktree` |
| `git_adapter/src/worktree.rs` (new) | Porcelain parse + add/remove via `run_git` |
| `git_adapter/src/lib.rs` | Wire trait methods |
| `git_adapter/src/tests.rs` | Temp-repo worktree tests |
| `test_support/src/lib.rs` | Fake fields + trait impl |
| `config_adapter/src/lib.rs` | Persist `worktree_tab_retention` |
| `gui_app/src/session.rs` (new) | `WorktreeSession`, `apply_retention`, accessors helpers |
| `gui_app/src/app.rs` | `sessions` / `active` / catalogue; path on finishes; messages |
| `gui_app/src/jobs.rs` | Background list/add/remove; pass path into finishes |
| `gui_app/src/lib.rs` | Export `WorktreeOpener` type alias |
| `gui_app/src/view/tabs.rs` (new) | Tab strip |
| `gui_app/src/view/worktrees.rs` (new) | Explorer section + create/remove prompts |
| `gui_app/src/view/settings.rs` | Retention control |
| `gui_app/src/view/history.rs` | WIP row per open session |
| `gui_app/src/view/mod.rs` / `workspace.rs` / `refs.rs` | Mount strip + explorer |
| `app/src/main.rs` | `worktree_opener` without `remember` |
| `app/src/startup.rs` | Tests: `last_repo_path` unchanged after worktree open (if tested at app layer) |
| `CONTEXT.md` | Replace “Later: worktrees” paragraph |

---

### Task 1: Domain model + port + porcelain adapter (WP1-a)

**Files:**
- Modify: `core_domain/src/model.rs`, `core_domain/src/ports.rs`
- Create: `git_adapter/src/worktree.rs`
- Modify: `git_adapter/src/lib.rs` (mod + trait impl)
- Modify: `test_support/src/lib.rs`
- Modify: `git_adapter/src/tests.rs`
- Test: `cargo test -p core_domain -p git_adapter -p test_support`

**Interfaces:**
- Produces:
  - `pub struct Worktree { path: String, branch: String, is_main: bool, locked: bool }`
  - `GitRepository::worktrees(&self) -> Result<Vec<Worktree>, DomainError>`
  - `GitRepository::add_worktree(&self, path: &str, branch: &str, create_branch: bool) -> Result<(), DomainError>`
  - `GitRepository::remove_worktree(&self, path: &str, force: bool) -> Result<(), DomainError>`
  - `pub(crate) fn parse_worktree_porcelain(stdout: &str) -> Vec<Worktree>` (testable without git)

- [ ] **Step 1: Add `Worktree` and failing pure parse test in adapter**

In `git_adapter/src/worktree.rs` (create file with parse + tests module first):

```rust
use core_domain::Worktree;

/// Parse `git worktree list --porcelain` stdout into domain rows.
pub(crate) fn parse_worktree_porcelain(stdout: &str) -> Vec<Worktree> {
    Vec::new() // intentional stub until Step 3
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn porcelain_lists_main_and_linked() {
        let raw = "\
worktree /repo
HEAD abcdef0123456789abcdef0123456789abcdef01
branch refs/heads/main

worktree /repo-feature
HEAD fedcba9876543210fedcba9876543210fedcba98
branch refs/heads/feature
locked
";
        let list = parse_worktree_porcelain(raw);
        assert_eq!(list.len(), 2);
        assert!(list[0].is_main);
        assert_eq!(list[0].branch, "main");
        assert_eq!(list[0].path.replace('\\', "/"), "/repo");
        assert!(!list[1].is_main);
        assert_eq!(list[1].branch, "feature");
        assert!(list[1].locked);
    }
}
```

Add to `core_domain/src/model.rs` (before or after `Branch`):

```rust
/// One entry from `git worktree list`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worktree {
    pub path: String,
    pub branch: String,
    pub is_main: bool,
    pub locked: bool,
}
```

Wire `mod worktree;` in `git_adapter/src/lib.rs`.

- [ ] **Step 2: Run parse test — expect FAIL**

Run: `cargo test -p git_adapter porcelain_lists_main_and_linked -- --exact`  
Expected: FAIL (empty vec / assert on len)

- [ ] **Step 3: Implement porcelain parser**

Rules:
- Split on blank lines into records.
- First record is main (`is_main: true`); later records `is_main: false`.
- `worktree <path>` → `path` (trim).
- `branch refs/heads/<name>` → `branch = name`; if `detached` line present or no branch → `branch = "(detached)"`.
- Presence of `locked` or `locked <reason>` → `locked = true`.
- Skip `prunable` entries (omit from list) so the UI never offers open/remove on dead paths.

- [ ] **Step 4: Run parse test — expect PASS**

- [ ] **Step 5: Extend port + fake**

In `core_domain/src/ports.rs` on `GitRepository`:

```rust
fn worktrees(&self) -> Result<Vec<Worktree>, DomainError>;
fn add_worktree(&self, path: &str, branch: &str, create_branch: bool) -> Result<(), DomainError>;
fn remove_worktree(&self, path: &str, force: bool) -> Result<(), DomainError>;
```

In `test_support`: add `worktrees_result`, `add_worktree_result`, `remove_worktree_result` fields (default `Ok(vec![])` / `Ok(())`) and implement the three methods. Fix every in-crate fake in `gui_app/src/jobs.rs` tests the same way.

- [ ] **Step 6: Implement adapter methods**

```rust
fn worktrees(&self) -> Result<Vec<Worktree>, DomainError> {
    self.open_repo()?;
    let out = run_git_stdout(&self.repo_path, &["worktree", "list", "--porcelain"])?;
    Ok(parse_worktree_porcelain(&out))
}

fn add_worktree(&self, path: &str, branch: &str, create_branch: bool) -> Result<(), DomainError> {
    self.open_repo()?;
    require_safe_path(path)?; // or path validation that rejects empty / `-` prefixed
    require_branch_name(branch)?;
    if create_branch {
        run_git(&self.repo_path, &["worktree", "add", "-b", branch, "--", path])
    } else {
        run_git(&self.repo_path, &["worktree", "add", "--", path, branch])
    }
}

fn remove_worktree(&self, path: &str, force: bool) -> Result<(), DomainError> {
    self.open_repo()?;
    // validate path; never special-case main here — UI blocks; git will refuse main
    if force {
        run_git(&self.repo_path, &["worktree", "remove", "--force", "--", path])
    } else {
        run_git(&self.repo_path, &["worktree", "remove", "--", path])
    }
}
```

Confirm exact `git worktree add` argument order against `git help worktree` on the machine (`add <path> <commit-ish>` vs `-b <new-branch> <path>`). Adjust to match.

- [ ] **Step 7: Temp-repo integration tests**

In `git_adapter/src/tests.rs`, following existing fixture patterns:

```rust
#[test]
fn worktrees_lists_main_after_init() { /* init fixture → worktrees() len 1, is_main */ }

#[test]
fn add_worktree_checks_out_branch_and_lists() { /* create branch, add path, assert list + HEAD */ }

#[test]
fn remove_worktree_drops_linked() { /* add then remove; list back to 1 */ }

#[test]
fn force_remove_dirty_linked_worktree() { /* dirty file in linked; remove without force fails; with force ok */ }
```

- [ ] **Step 8: Run adapter tests — expect PASS**

Run: `cargo test -p git_adapter worktree`  
Expected: PASS

- [ ] **Step 9: Commit**

```bash
git add core_domain git_adapter test_support
git commit -m "$(cat <<'EOF'
feat: add Worktree port and git worktree porcelain adapter

EOF
)"
```

---

### Task 2: Sessions, retention, path-routed finishes (WP1-b)

**Files:**
- Modify: `core_domain/src/model.rs` (`WorktreeTabRetention` + field on `AppSettings`)
- Modify: `config_adapter/src/lib.rs` (DTO default + round-trip)
- Create: `gui_app/src/session.rs`
- Modify: `gui_app/src/lib.rs` (`mod session`)
- Modify: `gui_app/src/app.rs` (largest change)
- Modify: `gui_app/src/jobs.rs` (pass `path` into finish constructors)
- Modify: `gui_app/src/view/settings.rs`
- Test: `cargo test -p gui_app -p config_adapter -p core_domain`

**Interfaces:**
- Produces:
  - `enum WorktreeTabRetention { Full, Partial, PathOnly }` default `Partial`
  - `AppSettings.worktree_tab_retention: WorktreeTabRetention`
  - `struct WorktreeSession { path, adapter, state, selection, commit_message, marked_files, … }`
  - `fn apply_retention(session: &mut WorktreeSession, mode: WorktreeTabRetention)`
  - `GitApp::session(&self) -> &WorktreeSession` / `session_mut(&mut self) -> &mut WorktreeSession`
  - Each git `*Finished` carries `path: String` (or consolidated `Finished { path, outcome }` — either OK if fitness holds)

- [ ] **Step 1: Failing unit test for `apply_retention`**

In `gui_app/src/session.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use core_domain::WorktreeTabRetention;
    use crate::selection::Selection;

    #[test]
    fn partial_clears_selection_keeps_message_and_marks() {
        let mut session = WorktreeSession::test_stub();
        session.commit_message = "wip".into();
        session.marked_files = vec![/* one MarkedFile */];
        session.selection = Some(Selection::Commit { id: "abc".into() });
        apply_retention(&mut session, WorktreeTabRetention::Partial);
        assert_eq!(session.commit_message, "wip");
        assert_eq!(session.marked_files.len(), 1);
        assert!(session.selection.is_none());
    }
}
```

- [ ] **Step 2: Run — expect FAIL** (type missing)

- [ ] **Step 3: Implement `WorktreeSession` + `apply_retention`**

Move per-repo fields from `GitApp` into `WorktreeSession` as listed in the spec. Keep layout/panes/prompts/terminal/settings/openers on `GitApp`.

```rust
pub(crate) fn apply_retention(session: &mut WorktreeSession, mode: WorktreeTabRetention) {
    match mode {
        WorktreeTabRetention::Full => {}
        WorktreeTabRetention::Partial => {
            session.selection = None;
            session.focused_diff_path = None;
        }
        WorktreeTabRetention::PathOnly => {
            session.selection = None;
            session.focused_diff_path = None;
            session.commit_message.clear();
            session.marked_files.clear();
        }
    }
}
```

Add Full / PathOnly tests in the same module.

- [ ] **Step 4: Settings persistence**

Add to `AppSettings` + DTO with `#[serde(default)]` → `WorktreeTabRetention::Partial`. Round-trip test in `config_adapter` mirroring `last_repo_path_round_trips…`. Persist from Settings UI when the user changes the control (reuse `store_appearance` / settings save pattern; extend `save_appearance` in `app/src/main.rs` to also copy `worktree_tab_retention` — or fold into the existing settings saver that already writes agent fields).

- [ ] **Step 5: Refactor `GitApp` to `sessions` + `active`**

- `sessions: Vec<WorktreeSession>`, `active: usize`, `worktree_catalogue: Vec<Worktree>`.
- Welcome = `sessions.is_empty()`.
- `with_repo` / `RepositoryOpened` build one session.
- Replace `self.state` / `self.adapter` / `self.commit_message` / `self.marked_files` / `self.selection` / patch fields with `self.session()` / `session_mut()` accessors.
- Mechanical test updates: `app.state` → `app.session().state` (or keep a temporary `state()` helper that forwards to active session during migration — delete helper before task ends).

- [ ] **Step 6: Path-routed finishes**

For every background git completion (`CheckoutFinished`, `FetchFinished`, … `RepositoryLoaded`, patch results):

1. Capture `path` when spawning (`session.path.clone()`).
2. On finish, `if let Some(session) = self.session_mut_by_path(&path) { … } else { return Task::none(); }`.
3. Never toggle `loading` on `sessions[active]` using only `active`.

Add test:

```rust
#[test]
fn finish_for_unknown_path_is_dropped() {
    let mut app = GitApp::with_repo(Arc::new(FakeGitRepository::ok()));
    let foreign = r"D:\other".to_string();
    let _ = app.update(Message::CheckoutFinished {
        path: foreign,
        result: Ok(()),
    });
    assert!(!app.session().state.loading);
    // and no panic
}
```

(Adapt variant shape to whatever you chose.)

- [ ] **Step 7: Activate / deactivate**

```rust
fn activate(&mut self, index: usize) {
    if index == self.active || index >= self.sessions.len() { return; }
    let mode = self.settings.worktree_tab_retention; // or session.state.settings
    apply_retention(&mut self.sessions[self.active], mode);
    self.active = index;
    // clear global prompts
}
```

Test: two sessions with different `commits`; activate second; first `commits` unchanged; no load job started for first.

- [ ] **Step 8: Settings UI control**

Three-option control (Full / Message and marks / Path only) in `view/settings.rs`; message updates settings + persists.

- [ ] **Step 9: `cargo test -p gui_app -p config_adapter` — PASS**

- [ ] **Step 10: Commit**

```bash
git commit -m "$(cat <<'EOF'
feat: multi-session AppState with path-routed git finishes

EOF
)"
```

---

### Task 3: Chrome — tabs, explorer, create/remove, WorktreeOpener (WP1-c)

**Files:**
- Create: `gui_app/src/view/tabs.rs`, `gui_app/src/view/worktrees.rs`
- Modify: `gui_app/src/view/mod.rs`, `workspace.rs`, `settings.rs` (if needed)
- Modify: `gui_app/src/lib.rs` (`WorktreeOpener` type)
- Modify: `gui_app/src/app.rs`, `jobs.rs`
- Modify: `app/src/main.rs`, optionally `app/src/startup.rs` tests
- Test: `cargo test -p gui_app -p app`

**Interfaces:**
- Produces:
  - `pub type WorktreeOpener = Arc<dyn Fn(&str) -> Result<SharedRepository, DomainError> + Send + Sync>`
  - Messages: `SelectWorktreeTab(usize)`, `CloseWorktreeTab(usize)`, `OpenWorktree(String)`, `AskCreateWorktree`, `AskRemoveWorktree { path }`, `ConfirmCreateWorktree`, `ConfirmRemoveWorktree { force }`, `WorktreesLoaded(Result<Vec<Worktree>>)`, …
  - `fn worktree_opener() -> WorktreeOpener` in `app` that does **not** call `remember`

- [ ] **Step 1: `WorktreeOpener` in composition root**

```rust
// app/src/main.rs
fn worktree_opener() -> gui_app::WorktreeOpener {
    Arc::new(|path| Ok(Arc::new(GitoxideAdapter::new(path))))
}
```

Pass into `GitApp::welcome_with` / `with_repo_opener` alongside `open`. Thread through constructors.

Test (unit in `app` or `gui_app` with spy opener): after `OpenWorktree`, the remember-opener's call count stays 0 / `last_repo_path` unchanged when using a fake store.

- [ ] **Step 2: Catalogue load job**

On session open and after create/remove: `worktrees_in_background(adapter)` → `WorktreesLoaded` → set `worktree_catalogue` (or `catalogue_error`).

- [ ] **Step 3: Tab strip view**

`view/tabs.rs`: row of buttons (label = basename), active style, `×` → `CloseWorktreeTab`, trailing `+` → picker / open list of catalogue paths not in `sessions`.

Mount under top bar in `view/mod.rs` when `!sessions.is_empty()`.

- [ ] **Step 4: Close / open handlers**

- `CloseWorktreeTab(i)`: refuse if `sessions.len()==1` or `sessions[i].state.loading`; else remove; fix `active`.
- `OpenWorktree(path)`: if already open, `activate`; else `open_worktree(path)` → push session → load → activate.
- Tests for refuse-last-close and open-second-session.

- [ ] **Step 5: Explorer Worktrees section**

In left Files/Changes column (`workspace.rs` / `view/worktrees.rs`): section header with count; rows; New… / Remove… actions. Remove disabled when `is_main` or path is main.

- [ ] **Step 6: Create modal**

Fields: path, branch, create_branch checkbox (or dual mode). Confirm → job `add_worktree` → on Ok refresh catalogue + `OpenWorktree(new_path)`.

- [ ] **Step 7: Remove confirm**

Path + branch + Force checkbox. Confirm → `remove_worktree` → drop matching session → refresh catalogue.

- [ ] **Step 8: `cargo test -p gui_app -p app` — PASS**

- [ ] **Step 9: Commit**

```bash
git commit -m "$(cat <<'EOF'
feat: worktree tab strip, explorer, and create/remove UI

EOF
)"
```

---

### Task 4: Multi WIP graph rows + CONTEXT (WP1-d)

**Files:**
- Modify: `gui_app/src/view/history.rs`
- Modify: `CONTEXT.md`
- Test: `cargo test -p gui_app`

**Interfaces:**
- Consumes: `app.sessions`, each session’s `state.current_status` + `state.branches` head tip
- Produces: one WIP row per **open** session that is dirty, labelled with that session’s branch (or basename), clicking activates that session / `ShowWorkingTree` for that path

- [ ] **Step 1: Extend history WIP rendering**

Replace single `working_tree_dirty(app)` gate with iteration:

```rust
for (index, session) in app.sessions.iter().enumerate() {
    if !session_dirty(session) { continue; }
    // push WIP row; on_press → Message::SelectWorktreeTab(index) then ShowWorkingTree
}
```

Keep lane column empty for WIP rows as today. Ensure only sessions whose HEAD tip is relevant still show when filtering later (for now show all dirty open sessions at top).

- [ ] **Step 2: Manual sanity** (if GUI available): two dirty worktrees → two WIP rows.

- [ ] **Step 3: Rewrite `CONTEXT.md`**

Replace “Later: several repositories and worktrees” with shipped behaviour: session tabs, catalogue, create/remove, retention setting, `WorktreeOpener` vs remember opener. Keep multi-**repo** (unrelated clones) as later if desired.

- [ ] **Step 4: Full test sweep**

Run: `cargo test -p core_domain -p git_adapter -p gui_app -p app -p config_adapter`  
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git commit -m "$(cat <<'EOF'
feat: WIP row per open worktree session

EOF
)"
```

- [ ] **Step 6: Check ADR 0003 action items 2–5**

Mark WP1-a…d checkboxes `[x]` in `docs/adr/0003-gitlens-target-sessions-compare-split-diff.md` and commit docs if not already in the feature commits.

---

## Spec coverage (self-review)

| Spec requirement | Task |
|---|---|
| `Worktree` + port methods + porcelain | Task 1 |
| Session owns `AppState`; switch = pointer | Task 2 |
| Retention Full/Partial/PathOnly + Settings persist | Task 2 |
| Path-routed finishes; drop unknown path | Task 2 |
| `WorktreeOpener` without remember | Task 3 |
| Tab strip sessions only; last close refused | Task 3 |
| Explorer list / open / create / remove+force | Task 3 |
| Close ≠ delete; remove ≠ tab × | Task 3 |
| Multi WIP rows | Task 4 |
| CONTEXT update | Task 4 |
| Compare / split diff | Deferred (WP2/WP3) |

## Placeholder scan

No TBD steps; git `worktree add` argv must be verified against local `git help worktree` in Task 1 Step 6.

## Type consistency

- `Worktree` / `WorktreeTabRetention` live in `core_domain`.
- `WorktreeSession` / `apply_retention` live in `gui_app::session`.
- Openers: `RepositoryOpener` remembers; `WorktreeOpener` does not.
