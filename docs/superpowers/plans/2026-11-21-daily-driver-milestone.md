# Daily-Driver Milestone Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the commit-loop, sync and safety gaps that make GitCthulhu feel thinner than GitDesktop in daily use.

**Architecture:** Every git capability is a new method on `core_domain::GitRepository` (with a default body so fakes keep compiling), implemented in `git_adapter` by running the git CLI and tested against scratch repositories. The GUI reaches it through a `jobs.rs` background function and a `Message` handled in `GitApp::update`. New read results enter `AppState` only through new `ViewEvent`s (ADR 0002). Views stay O(visible) (ADR 0007).

**Tech Stack:** Rust 2021, iced 0.13 (`gui_app`), git CLI via `git_adapter::command::{run_git, run_git_stdout, git_output}`, `test_support::FakeGitRepository`.

**Spec:** `docs/analysis/2026-11-21-gitdesktop-competitive-analysis.md` §7, items A1–A4, B1–B5, B7, C1, C4, D1, D5, E2. The GitDesktop reference paths there are behaviour references only. A5 (persist layout) is **not** in this milestone.

## Global Constraints

- ADR 0001: the window never names `GitoxideAdapter`; only the composition root does.
- ADR 0002: `AppState::apply_event` is the only writer of `AppState`.
- ADR 0007: no message per pointer move; no per-row scan in `view`; new history data joins `IndexRow` when the index is built, not when rows are drawn.
- New `GitRepository` methods have a default body returning `Err(DomainError::Unknown("this repository cannot <verb>".to_string()))`, as `merge_preview` does. `PanickingRepository` (`gui_app/src/jobs.rs:1109`) needs no change.
- `FakeGitRepository` (`test_support/src/lib.rs`) gets one `pub <name>_result` field per new method the GUI tests drive, defaulted to `Ok(..)` in `FakeGitRepository::ok()`.
- Adapter tests use the fixtures in `git_adapter/src/tests.rs` (`TestRepo`, `git_in`, `cloned_from`, `commit_file`, `head_at`). Nothing may depend on the developer's global git config.
- British English in copy and identifiers. Copy says what happened, not what failed technically.
- Commit straight to `main`, one commit per task, ending with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Checks per task: `cargo test -p git_adapter` and/or `cargo test -p gui_app` (whichever the task touches) and `cargo clippy -p gui_app -p git_adapter -p core_domain -- -D warnings`.

## Review Focus

1. **HEAD moved under an open prompt** (the user commits from the terminal pane while Undo or Amend is waiting): the operation refuses with `DomainError::HeadMoved` and nothing is reset. Pinned in Task 5.
2. **Unborn repository** (no commits yet): undo, amend, revert, unpushed markers and publish do not panic; undo and amend are unavailable with the reason "No commits yet". Pinned in Tasks 2, 3 and 5.
3. **`numstat` edge rows**: binary files print `-\t-`, and renames in `-z` mode spread over three NUL fields. Both parse without error (binary → no counts, rename → counted under the new path). Pinned in Task 6.
4. **Filter hides marked files**: Ctrl+S and Stage act only on marked files the filter still shows; a hidden mark is never staged silently. Pinned in Task 7.
5. **Remote moved since the last fetch**: force push with lease refuses, the error presents as "Push rejected", and the remote branch is unchanged. Pinned in Task 9.

---

### Task 1: Error presentation (A4)

**Files:**
- Create: `gui_app/src/git_error.rs`
- Modify: `gui_app/src/lib.rs` (add `mod git_error;`), `gui_app/src/view/status_bar.rs` (`error_cluster`, delete `error_summary` and move its test)

**Interfaces:**
- Produces: `pub(crate) enum ErrorKind { Conflict, Rejected, Auth, Network, DirtyTree, PathTooLong, Locked, HeadMoved, Git }` with `fn label(self) -> &'static str`. `pub(crate) struct ErrorReport { pub kind: ErrorKind, pub summary: String }`. `pub(crate) fn present(error: &str) -> ErrorReport`. Pure; the full text stays in `GitApp::error`.

- [ ] **Step 1: Write the failing tests in `git_error.rs`**

```rust
#[test] fn fatal_and_hints_are_stripped_from_the_summary() {
    let r = present("hint: see git help\nfatal: refusing to merge unrelated histories\n");
    assert_eq!(r.kind, ErrorKind::Git);
    assert_eq!(r.summary, "refusing to merge unrelated histories");
}
#[test] fn a_non_fast_forward_push_is_rejected() {
    assert_eq!(present("! [rejected]        main -> main (fetch first)\nerror: failed to push some refs").kind, ErrorKind::Rejected);
    assert_eq!(present("! [rejected] main -> main (stale info)").kind, ErrorKind::Rejected);
}
#[test] fn auth_network_dirty_lock_and_long_paths_are_named() {
    assert_eq!(present("fatal: Authentication failed for 'https://x'").kind, ErrorKind::Auth);
    assert_eq!(present("git@github.com: Permission denied (publickey).").kind, ErrorKind::Auth);
    assert_eq!(present("fatal: unable to access 'https://x/': Could not resolve host: x").kind, ErrorKind::Network);
    assert_eq!(present("error: Your local changes to the following files would be overwritten by checkout:").kind, ErrorKind::DirtyTree);
    assert_eq!(present("fatal: Unable to create '/r/.git/index.lock': File exists.").kind, ErrorKind::Locked);
    assert_eq!(present("error: unable to create file a/b: Filename too long").kind, ErrorKind::PathTooLong);
    assert_eq!(present("CONFLICT (content): Merge conflict in a.txt").kind, ErrorKind::Conflict);
}
#[test] fn the_first_non_blank_line_is_the_summary_when_nothing_matches() {
    assert_eq!(present("\n  something odd  \nmore").summary, "something odd");
}
```

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p gui_app git_error` · Expected: FAIL (`present` not found).

- [ ] **Step 3: Implement `present`**

Kind = the first rule that matches the whole text, in the order the test lists them (`HeadMoved` matches the `HEAD_MOVED` copy from Task 5; write the constant here as `pub(crate) const HEAD_MOVED: &str = "HEAD moved while this was open, so nothing was changed.";`). Summary = the first line that is not blank, does not start with `hint:` and is not a progress line (`Enumerating`, `Counting`, `Compressing`, `Writing objects`, `remote: `), with a leading `error: ` / `fatal: ` / `warning: ` removed. Labels: "Merge conflict", "Push rejected", "Sign-in failed", "Network", "Local changes in the way", "Path too long", "Repository busy", "HEAD moved", "Git".

- [ ] **Step 4: Make `error_cluster` show `"{label} · {summary}"` collapsed and the full text expanded; delete `error_summary` and point its callers at `present(..).summary`**

- [ ] **Step 5: Run `cargo test -p gui_app` · Expected: PASS**

- [ ] **Step 6: Commit** `feat(gui): name the kind of a git error and summarise it in one line`

---

### Task 2: Action catalogue, command palette and shortcuts sheet (A1, A2)

**Files:**
- Create: `gui_app/src/actions.rs`, `gui_app/src/view/shortcuts.rs`
- Modify: `gui_app/src/app.rs` (`shortcut_message` 5718, `typing_shortcut` 5865, `palette_message` 5900, `palette_results`, `Message`), `gui_app/src/palette.rs`, `gui_app/src/view/palette.rs`, `gui_app/src/view/mod.rs` (overlay stack)

**Interfaces:**
- Produces:
  - `pub(crate) enum ActionId { Fetch, Pull, Push, Refresh, StageMarked, UnstageMarked, Stash, Commit, ToggleTerminal, CycleLayout, CycleHunkMode, QuickOpen, CommandPalette, FocusHistorySearch, ShowShortcuts, ToggleDiffSplit, ToggleFileTree, OpenSettings }`. Later tasks add variants: `AmendLastCommit`, `UndoLastCommit` (Task 5), `Publish`, `ForcePush` (Task 9), `FindLostStashes` (Task 12).
  - `pub(crate) struct Chord { pub ctrl: bool, pub shift: bool, pub key: ChordKey }`, `pub(crate) enum ChordKey { Char(char), F5, Enter, Backquote, Slash }`, `fn Chord::label(&self) -> String` ("Ctrl+Shift+P").
  - `pub(crate) struct ActionSpec { pub id: ActionId, pub label: &'static str, pub category: &'static str, pub chord: Option<Chord> }` and `pub(crate) const ACTIONS: &[ActionSpec]`.
  - `pub(crate) fn action_for(key: &iced::keyboard::Key, modifiers: iced::keyboard::Modifiers) -> Option<ActionId>`.
  - `#[derive(Debug, Clone, Copy, Default)] pub(crate) struct ActionContext { pub has_session: bool, pub loading: bool, pub busy: bool, pub head_born: bool, pub has_upstream: bool, pub has_staged: bool, pub has_marked: bool, pub has_changes: bool }` built by `GitApp::action_context(&self) -> ActionContext`.
  - `pub(crate) fn availability(id: ActionId, cx: &ActionContext) -> Result<(), &'static str>`.
  - `Message::RunAction(ActionId)`, `Message::ToggleShortcuts`. `GitApp::message_for(id: ActionId) -> Message` maps each id to the existing message (`Fetch` → `Message::Fetch`, `QuickOpen` → `TogglePalette(String::new())`, `CommandPalette` → `TogglePalette(">".into())`, …).
  - `PaletteKind::Command`, `PaletteTarget::Command(ActionId)`, scope prefix `">"`.

Default chords copy today's keys plus these new ones: Ctrl+Shift+P command palette, Ctrl+/ shortcuts sheet, Ctrl+Enter commit, Ctrl+Shift+S stash. Undo has **no** default chord.

- [ ] **Step 1: Write the failing tests in `actions.rs`**

```rust
#[test] fn every_action_is_listed_once_and_no_two_share_a_chord() { /* ids unique; chords unique */ }
#[test] fn todays_keys_still_resolve() {
    assert_eq!(action_for(&Key::Named(Named::F5), Modifiers::empty()), Some(ActionId::Fetch));
    assert_eq!(action_for(&Key::Character("r".into()), Modifiers::CTRL), Some(ActionId::Refresh));
    assert_eq!(action_for(&Key::Character("S".into()), Modifiers::CTRL | Modifiers::SHIFT), Some(ActionId::Stash));
    assert_eq!(action_for(&Key::Character("p".into()), Modifiers::CTRL | Modifiers::SHIFT), Some(ActionId::CommandPalette));
}
#[test] fn an_unavailable_action_says_why() {
    let idle = ActionContext { has_session: true, ..Default::default() };
    assert_eq!(availability(ActionId::Commit, &idle), Err("Nothing is staged"));
    assert_eq!(availability(ActionId::Pull, &idle), Err("No upstream: this branch does not track a remote branch"));
    let busy = ActionContext { busy: true, ..idle };
    assert_eq!(availability(ActionId::Fetch, &busy), Err("Another git action is running"));
}
```

In `app.rs` tests:

```rust
#[test] fn the_command_scope_lists_actions_and_refuses_an_unavailable_one() {
    let mut app = GitApp::with_repo(Arc::new(FakeGitRepository::ok()));
    let _ = app.update(Message::TogglePalette(">commit".into()));
    let (items, ranked) = app.palette_results();
    let first = &items[ranked[0]];
    assert_eq!(first.target, PaletteTarget::Command(ActionId::Commit));
    assert_eq!(first.detail, "Ctrl+Enter · Nothing is staged");
    let _ = app.update(Message::PaletteActivate(0));
    assert!(app.palette.is_some(), "an unavailable command keeps the palette open");
}
```

- [ ] **Step 2: Run them to see them fail** · `cargo test -p gui_app actions` · Expected: FAIL.

- [ ] **Step 3: Implement `actions.rs`, `availability` and `GitApp::action_context`**

Busy and loading are checked first and reuse `tracking::sync_buttons` reasons so the top bar and palette say the same thing. Pull and Push read `status.tracking`.

- [ ] **Step 4: `shortcut_message` and `typing_shortcut` look chords up with `action_for` and send `RunAction`**

Escape, ArrowUp/ArrowDown and Ctrl+C keep their special handling. `RunAction(id)` sends `message_for(id)` only when `availability` is `Ok`; otherwise it sets `self.error` to the reason.

- [ ] **Step 5: Palette command scope**

`palette_items` adds one `PaletteItem` per `ACTIONS` row only when the query starts with `>` (`label` = action label; `detail` = chord label, then `· reason` when unavailable). Activating an unavailable command does nothing. Unavailable rows draw in `tokens.text_muted`.

- [ ] **Step 6: Shortcuts sheet** (`view/shortcuts.rs`): a modal overlay listing `ACTIONS` grouped by `category`, each row "label … chord". Esc closes it (add a branch to `Message::Escape`).

- [ ] **Step 7: Run `cargo test -p gui_app` · Expected: PASS**

- [ ] **Step 8: Commit** `feat(gui): one action catalogue behind keys, a command palette and a shortcuts sheet`

---

### Task 3: Unpushed commits marked in the graph (D1)

**Files:**
- Modify: `core_domain/src/ports.rs`, `core_domain/src/state.rs` (`ViewEvent`, `AppState`), `git_adapter/src/lib.rs`, `git_adapter/src/tests.rs`, `gui_app/src/jobs.rs` (`load_repository_up_to`), `gui_app/src/history_index.rs` (`IndexRow`), `gui_app/src/view/history.rs` (`build_index`, `history_row`), `test_support/src/lib.rs`

**Interfaces:**
- Produces: `fn unpushed_commits(&self) -> Result<Vec<String>, DomainError>` on the port. `ViewEvent::UnpushedUpdated(Vec<String>)`. `AppState::unpushed: std::collections::HashSet<String>`. `IndexRow::unpushed: bool`. `FakeGitRepository::unpushed_result`.

- [ ] **Step 1: Write the failing adapter tests**

```rust
#[test] fn unpushed_lists_commits_on_no_remote_newest_first() {
    // origin with "first"; clone; commit "second" and "third" in the clone
    // assert_eq!(adapter.unpushed_commits()?, vec![third_id, second_id]);
}
#[test] fn unpushed_is_empty_without_remotes_and_in_an_unborn_repository() {
    // TestRepo::new() with one commit, no remote → empty; TestRepo::new() with no commit → empty, Ok
}
```

- [ ] **Step 2: Run** `cargo test -p git_adapter unpushed` · Expected: FAIL.

- [ ] **Step 3: Implement**

No remotes (`git remote` prints nothing) or an unborn HEAD → `Ok(vec![])`. Otherwise `git rev-list --max-count=1000 HEAD --not --remotes`, one id per line.

- [ ] **Step 4: Wire the event**

`load_repository_up_to` pushes `UnpushedUpdated` (or `ErrorOccurred`). `apply_event` replaces `AppState::unpushed`. The existing `history_generation` bump in `RepositoryLoaded` covers the index rebuild.

- [ ] **Step 5: GUI test** in `view/history.rs` tests: a session with `state.unpushed = {"abc1234"}` builds an index whose row for `abc1234` has `unpushed == true` and the others `false`.

- [ ] **Step 6: Draw** an `↑` glyph in `tokens.accent_primary` before the short hash when `row.unpushed`. The Hash column keeps its width.

- [ ] **Step 7: Run** `cargo test -p git_adapter -p gui_app` · Expected: PASS.

- [ ] **Step 8: Commit** `feat: mark commits that are on no remote yet`

---

### Task 4: Commit summary and body (B1)

**Files:**
- Modify: `gui_app/src/session.rs` (`WorktreeSession`, `apply_retention`), `gui_app/src/view/changes.rs` (`commit_box`), `gui_app/src/view/commands.rs` (`can_commit`), `gui_app/src/app.rs` (`Message`, `CommitMessageChanged`, `WorktreeEditFinished`, `SuggestCommit` handler ≈2543)

**Interfaces:**
- Produces: `WorktreeSession::commit_body: iced::widget::text_editor::Content`. `Message::CommitBodyEdited(text_editor::Action)`. `pub(crate) fn full_commit_message(summary: &str, body: &str) -> String` in `session.rs`: `summary.trim()`, then a blank line and `body.trim_end()` when the body has any non-blank text.
- Consumes: `ActionId::Commit` (Task 2) for Ctrl+Enter.

- [ ] **Step 1: Failing tests**

```rust
#[test] fn a_body_follows_the_summary_after_one_blank_line() {
    assert_eq!(full_commit_message(" Fix parser ", "Long story.\n\n"), "Fix parser\n\nLong story.");
    assert_eq!(full_commit_message("Fix parser", "  \n"), "Fix parser");
}
#[test] fn committing_clears_summary_and_body_and_path_only_retention_clears_both() { /* WorktreeEditFinished Ok → both empty */ }
```

- [ ] **Step 2: Run** `cargo test -p gui_app commit` · Expected: FAIL.

- [ ] **Step 3: Implement**

The commit box becomes a column: the summary `text_input` (Enter moves focus to the body instead of committing), a `text_editor` body 4 lines tall, then the button row. `WorktreeEdit::Commit` carries `full_commit_message(..)`. A suggestion with several lines puts its first line in the summary and the rest in the body.

- [ ] **Step 4: Run** `cargo test -p gui_app` · Expected: PASS.

- [ ] **Step 5: Commit** `feat(gui): a commit has a summary and a body`

---

### Task 5: Amend and undo the last commit, guarded by the HEAD they saw (A3, B2)

**Files:**
- Modify: `core_domain/src/model.rs` (`DomainError`), `core_domain/src/ports.rs`, `git_adapter/src/lib.rs`, `git_adapter/src/tests.rs`, `gui_app/src/jobs.rs` (`error_message`, `WorktreeEdit`, `edit_in_background`), `gui_app/src/app.rs`, `gui_app/src/view/changes.rs`, `gui_app/src/view/confirm.rs`, `gui_app/src/context_menu.rs` (commit menu), `gui_app/src/actions.rs`, `test_support/src/lib.rs`

**Interfaces:**
- Produces:
  - `DomainError::HeadMoved { expected: String, actual: String }`. `error_message` maps it to `git_error::HEAD_MOVED`.
  - Port: `fn amend(&self, message: &str, expected_head: &str) -> Result<(), DomainError>` and `fn undo_commit(&self, expected_head: &str) -> Result<String, DomainError>`, returning the undone commit's full message.
  - Adapter helper `fn require_head(&self, expected: &str) -> Result<(), DomainError>` (compares `git rev-parse HEAD`).
  - `WorktreeEdit::Amend { message: String, expected_head: String }`. `WorktreeSession::amending: bool`. `Message::ToggleAmend`, `Message::AskUndoCommit`, `Message::ConfirmUndoCommit`, `Message::UndoCommitFinished(SessionKey, Result<String, DomainError>)`. `HistoryPrompt::UndoPushed { id: String }`.
  - `ActionId::AmendLastCommit`, `ActionId::UndoLastCommit`. Both unavailable with "No commits yet" when `!head_born`.
- Consumes: `AppState::unpushed` (Task 3), `full_commit_message` (Task 4).

- [ ] **Step 1: Failing adapter tests**

```rust
#[test] fn undo_moves_head_back_keeps_the_change_staged_and_returns_the_message() { /* message "second\n\nbody" round-trips; HEAD == first; a.txt staged */ }
#[test] fn undo_of_the_first_commit_is_refused() { /* Err containing "first commit" */ }
#[test] fn undo_and_amend_refuse_when_head_is_not_the_one_expected() {
    // expected = first id while HEAD is second → Err(DomainError::HeadMoved { .. }); HEAD unchanged
}
#[test] fn amend_rewrites_the_message_and_takes_staged_changes() { /* new id, parent unchanged, message replaced */ }
```

- [ ] **Step 2: Run** `cargo test -p git_adapter amend undo` · Expected: FAIL.

- [ ] **Step 3: Implement**

`undo_commit`: `require_head` → refuse a commit with no parent (`git rev-parse --verify HEAD~1` fails) with "The first commit can't be undone; there is nothing before it." → read `git log -1 --format=%B` → `git reset --soft HEAD~1` → return the message trimmed. `amend`: `require_head` → blank message refused with `BLANK_COMMIT_MESSAGE` → `git commit --amend -m <message>`.

- [ ] **Step 4: Failing GUI tests**

```rust
#[test] fn ticking_amend_fills_an_empty_draft_from_the_head_commit() { /* commits[0] is HEAD with "Fix\n\nWhy" → summary "Fix", body "Why" */ }
#[test] fn undo_of_an_unpushed_commit_runs_at_once_and_a_pushed_one_asks_first() {
    // head in state.unpushed → AskUndoCommit sets loading, no prompt
    // head not in unpushed (remote exists) → history_prompt == Some(UndoPushed { id })
}
#[test] fn an_undone_message_returns_to_the_draft() { /* UndoCommitFinished(Ok("Fix\n\nWhy")) → summary/body */ }
#[test] fn head_moved_is_reported_in_plain_words() { /* Err(HeadMoved) → app.error == HEAD_MOVED */ }
#[test] fn undo_and_amend_are_unavailable_before_the_first_commit() {
    let cx = ActionContext { has_session: true, head_born: false, ..Default::default() };
    assert_eq!(availability(ActionId::UndoLastCommit, &cx), Err("No commits yet"));
    assert_eq!(availability(ActionId::AmendLastCommit, &cx), Err("No commits yet"));
}
```

- [ ] **Step 5: Implement the GUI**

An "Amend" checkbox sits beside Commit. While it is ticked, the button reads "Amend" and is enabled with nothing staged. If HEAD is already pushed (`head ∉ unpushed` and the repo has a remote), the button asks first: "This commit is already on a remote. Amending rewrites it, so the next push will need a force push." Undo comes from the palette and from the commit menu "Undo commit (keep changes)", shown only on the HEAD row. Both send the HEAD id the view saw as `expected_head`.

- [ ] **Step 6: Run** `cargo test -p git_adapter -p gui_app` · Expected: PASS.

- [ ] **Step 7: Commit** `feat: amend and undo the last commit, refused if HEAD moved meanwhile`

---

### Task 6: Line counts in the changes list (B4)

**Files:**
- Modify: `core_domain/src/model.rs` (`LineCounts`), `core_domain/src/ports.rs`, `core_domain/src/state.rs`, `git_adapter/src/status.rs` (parser + reader), `git_adapter/src/tests.rs`, `gui_app/src/jobs.rs`, `gui_app/src/view/changes.rs` (file row), `test_support/src/lib.rs`

**Interfaces:**
- Produces: `pub struct LineCounts { pub staged: BTreeMap<String, Option<(u32, u32)>>, pub unstaged: BTreeMap<String, Option<(u32, u32)>>, pub untracked: BTreeMap<String, Option<(u32, u32)>> }` (`None` = binary or too large). Port `fn line_counts(&self) -> Result<LineCounts, DomainError>`. `ViewEvent::LineCountsUpdated(LineCounts)`. `AppState::line_counts: LineCounts`. `pub(crate) fn parse_numstat_z(text: &str) -> BTreeMap<String, Option<(u32, u32)>>` in `git_adapter/src/status.rs`.

- [ ] **Step 1: Failing parser tests**

```rust
#[test] fn numstat_z_reads_counts_binary_and_renames() {
    let text = "3\t1\ta.txt\0-\t-\timg.png\x002\t0\t\0old.rs\0new.rs\0";
    let counts = parse_numstat_z(text);
    assert_eq!(counts["a.txt"], Some((3, 1)));
    assert_eq!(counts["img.png"], None);
    assert_eq!(counts["new.rs"], Some((2, 0)));
    assert!(!counts.contains_key("old.rs"));
}
```

Adapter test `line_counts_cover_staged_unstaged_and_untracked`: stage a 2-line edit, leave a 1-line edit unstaged, add a 4-line untracked file. Expect `(2,_)`, `(1,_)`, `Some((4,0))`.

- [ ] **Step 2: Run** · Expected: FAIL.

- [ ] **Step 3: Implement**

Staged = `git diff --cached --numstat -z`; unstaged = `git diff --numstat -z`. Untracked: for at most the first 500 files from `status`, read the file when it is ≤ 1 MiB and its first 8,000 bytes hold no NUL; the count is (newline count, plus 1 when the last byte is not `\n` and the file is non-empty; 0). Otherwise `None`.

- [ ] **Step 4: Draw** `+n −m` after the file name in `tokens.added` / `tokens.removed`; `None` draws nothing. Load the counts in `load_repository_up_to`.

- [ ] **Step 5: Run** `cargo test -p git_adapter -p gui_app` · Expected: PASS.

- [ ] **Step 6: Commit** `feat: added and removed line counts beside each changed file`

---

### Task 7: Changes filter (B3)

**Files:**
- Modify: `gui_app/src/changes.rs`, `gui_app/src/session.rs`, `gui_app/src/view/changes.rs`, `gui_app/src/app.rs` (`select_files`, `edit_marked`, `Message`)

**Interfaces:**
- Produces: `pub(crate) enum ChangeChip { Staged, Unstaged, New, Modified, Deleted }`. `pub(crate) struct ChangeFilter { pub text: String, pub chips: BTreeSet<ChangeChip> }` (Default = shows everything). `WorktreeSession::change_filter: ChangeFilter`. `pub(crate) fn filter_matches(filter: &ChangeFilter, path: &str, place: FileDisposition, kind: ChangeKind) -> bool`. `Message::ChangeFilterChanged(String)`, `Message::ToggleChangeChip(ChangeChip)`.
- `visible_files` and the pane both apply `filter_matches`, so Shift ranges walk only visible files.

Semantics: the text is a case-insensitive substring of the path. Chips OR together, and an empty chip set matches all. `New` = Added or Untracked; `Modified` = Modified or Renamed; `Staged` / `Unstaged` match the place (Untracked counts as Unstaged). Conflicted files are never hidden.

- [ ] **Step 1: Failing tests**

```rust
#[test] fn text_and_chips_filter_and_conflicts_always_show() { /* table of (filter, path, place, kind) → bool */ }
#[test] fn stage_marked_skips_marks_the_filter_hides() {
    // marks a.rs and b.md; filter "rs"; StageMarked → the edit stages only ["a.rs"]
}
```

- [ ] **Step 2: Run** · Expected: FAIL.

- [ ] **Step 3: Implement**

A filter row above the sections: a `text_input` "Filter changes" plus five toggle chips. When filtering, a section header reads "Changes (2 of 9)". `edit_marked` filters marks through `filter_matches`. The filter is kept per session and survives refreshes.

- [ ] **Step 4: Run** `cargo test -p gui_app` · Expected: PASS.

- [ ] **Step 5: Commit** `feat(gui): filter the changes list by path and kind`

---

### Task 8: Keyboard walking of the graph and the changes list (B5, B7)

**Files:**
- Modify: `gui_app/src/app.rs` (`shortcut_message`, `PaletteMove` handler, `Message`), `gui_app/src/history_index.rs`, `gui_app/src/changes.rs`, `gui_app/src/session.rs`

**Interfaces:**
- Produces: `pub(crate) enum KeyList { Graph, Changes }` with `GitApp::key_list: KeyList`, set to `Graph` by `Message::Select(Selection::Commit{..})` and to `Changes` by a file click. `Message::ListKey { key: ListKeyName, shift: bool }`, with `enum ListKeyName { Up, Down, Left, Right }`.
- Pure helpers: `pub(crate) fn step_commit(index: &HistoryIndex, current: Option<&str>, step: i32) -> Option<String>` (clamped, never wraps). `pub(crate) fn parent_prefix(path: &str) -> Option<String>`.

Routing: ArrowUp and ArrowDown keep the palette and terminal-history behaviour first (the existing `PaletteMove` branches). Only when neither applies do they become `ListKey`. Left and Right are `ListKey` only.

- [ ] **Step 1: Failing tests**

```rust
#[test] fn down_moves_to_the_next_drawn_commit_and_stops_at_the_end() { /* step_commit clamps */ }
#[test] fn arrows_walk_visible_changes_and_shift_extends_from_the_anchor() { /* uses visible_files order */ }
#[test] fn left_folds_the_selected_files_folder_and_right_unfolds_its_chain() {
    // selected "src/ui/a.rs" (Tree mode) → Left inserts (place, "src/ui") into collapsed_dirs; Right removes every prefix of it
}
#[test] fn arrows_still_drive_an_open_palette() { /* palette open → PaletteMove, selection unchanged */ }
```

- [ ] **Step 2: Run** · Expected: FAIL.

- [ ] **Step 3: Implement**

`Graph`: Up/Down select the neighbouring drawn commit through the cached `HistoryIndex`. When its row top (from `HistoryLayout::top_of` in `session.rs`) is outside `[history_scroll, history_scroll + pane height]`, scroll with `scrollable::scroll_to(Id::new(HISTORY_SCROLL_ID), AbsoluteOffset { x: 0.0, y })`, as `restore_scroll` (`app.rs` ≈4600) does. `Changes`: Up/Down move the single selection, or extend it from `selection_anchor` with Shift. Left/Right fold and unfold as the test states. In List mode Left/Right do nothing.

- [ ] **Step 4: Run** `cargo test -p gui_app` · Expected: PASS. Then `GITCTHULHU_PERF=1` on the Backend repo: holding ↓ must not rebuild the index (the generation is unchanged).

- [ ] **Step 5: Commit** `feat(gui): walk the graph and the changes list with the arrow keys`

---

### Task 9: Publish and guarded force push (C1)

**Files:**
- Modify: `core_domain/src/model.rs` (`ForceGuard`), `core_domain/src/ports.rs`, `core_domain/src/state.rs` (`AppState::remotes`, `ViewEvent::RemotesUpdated`), `git_adapter/src/lib.rs`, `git_adapter/src/tests.rs`, `gui_app/src/tracking.rs` (`SyncButton`, `sync_buttons`), `gui_app/src/view/top_bar.rs` (`sync_group`), `gui_app/src/view/confirm.rs`, `gui_app/src/jobs.rs`, `gui_app/src/app.rs`, `gui_app/src/actions.rs`, `test_support/src/lib.rs`

**Interfaces:**
- Produces:
  - `pub enum ForceGuard { LeaseAndIncludes, LeaseOnly }`.
  - Port: `fn remotes(&self) -> Result<Vec<String>, DomainError>`, `fn publish(&self, remote: &str) -> Result<(), DomainError>`, `fn force_push(&self) -> Result<ForceGuard, DomainError>`.
  - `SyncButton` gains `pub(crate) action: SyncAction` with `enum SyncAction { Fetch, Pull, Push, Publish(String), ForcePush, Stash }`. `sync_buttons(status, remotes: &[String], running)`.
  - `Message::Publish(String)`, `Message::AskForcePush`, `Message::ConfirmForcePush`, `Message::ForcePushFinished(SessionKey, Result<ForceGuard, DomainError>)`. `HistoryPrompt::ForcePush { upstream: String, ahead: usize, behind: usize }`.
  - `ActionId::Publish`, `ActionId::ForcePush`.

Push button states: no upstream + `origin` present → "Publish to origin" (`Publish("origin")`). No upstream + exactly one remote → publish to it. No upstream + several remotes and none is `origin` → disabled with "Several remotes and none is origin: publish from the terminal". No remotes → disabled with "No remote: add one with git remote add". Diverged (`ahead > 0 && behind > 0`) → "Force push ↑a" (`ForcePush`), which always opens the confirm. Otherwise as today.

- [ ] **Step 1: Failing adapter tests**

```rust
#[test] fn publish_pushes_head_and_sets_the_upstream() { /* new branch in clone → publish("origin") → status().tracking.upstream == "origin/<branch>" */ }
#[test] fn force_push_replaces_a_diverged_remote_branch() { /* amend in clone after push → force_push() Ok; origin tip == clone tip */ }
#[test] fn force_push_refuses_when_the_remote_moved_since_the_last_fetch() {
    // clone pushes A; origin gets B directly (not fetched); clone amends → force_push() is Err
    // whose text contains "stale info" (Task 1 pins that text as ErrorKind::Rejected); origin tip == B
}
```

- [ ] **Step 2: Run** · Expected: FAIL.

- [ ] **Step 3: Implement**

`remotes` = `git remote` lines. `publish` = `git push --set-upstream -- <remote> HEAD`; refuse an option-like remote. `force_push`: parse `git version` (major.minor); read the current branch with `git symbolic-ref --short HEAD`; check `git reflog exists refs/heads/<branch>`. With git ≥ 2.30 and a reflog → `git push --force-with-lease --force-if-includes` and `LeaseAndIncludes`; otherwise `git push --force-with-lease` and `LeaseOnly`.

- [ ] **Step 4: Failing GUI tests**

```rust
#[test] fn push_reads_publish_without_an_upstream_and_force_push_when_diverged() { /* sync_buttons table */ }
#[test] fn publish_is_unavailable_before_the_first_commit() {
    // status { tracking: None, head_commit_id: None }, remotes ["origin"] → push disabled, reason "No commits yet"
}
#[test] fn force_push_always_asks_and_names_what_will_be_overwritten() { /* AskForcePush → HistoryPrompt::ForcePush{upstream:"origin/main",ahead:1,behind:2} */ }
#[test] fn a_lease_only_force_push_says_so() { /* ForcePushFinished(Ok(LeaseOnly)) → activity/status text contains "lease only" */ }
```

- [ ] **Step 5: Implement the GUI**

Confirm copy: "Force push to origin/main? Your branch replaces it. The 2 commits on origin/main that you have not pulled will no longer be on the branch. Git refuses if origin/main moved since your last fetch." The buttons read "Force push" and "Cancel". Load `remotes` in `load_repository_up_to`.

- [ ] **Step 6: Run** `cargo test -p git_adapter -p gui_app` · Expected: PASS.

- [ ] **Step 7: Commit** `feat: publish a branch and force push with a lease`

---

### Task 10: Switching branches with changes (C4)

**Files:**
- Modify: `gui_app/src/app.rs` (`CheckoutBranch` handler ≈1960, `BranchPrompt`, `Message`), `gui_app/src/view/confirm.rs`, `gui_app/src/jobs.rs`

**Interfaces:**
- Produces: `BranchPrompt::Switch { name: String }`. `Message::SwitchBringingChanges`, `Message::SwitchStashingChanges`. `pub(crate) fn stash_and_checkout_in_background(repo: SharedRepository, name: String) -> impl Future<Output = Result<(), DomainError>>`.
- Consumes: port `stash_push(message, include_untracked)`, `checkout_branch`, `stash_pop`.

Rule: a checkout with staged or unstaged changes opens the prompt; a clean tree checks out at once, as today. Untracked files alone do not prompt.

- [ ] **Step 1: Failing tests**

```rust
#[test] fn a_dirty_tree_asks_before_switching_and_a_clean_one_does_not() {}
#[test] fn stash_and_switch_pops_the_stash_back_when_the_checkout_fails() {
    // FakeGitRepository { checkout_result: Err(..), .. } → stash_pop_result consulted; error text ends with "Your changes are back."
}
```

- [ ] **Step 2: Run** · Expected: FAIL.

- [ ] **Step 3: Implement**

The stash message is `"Switching to <name>"` with `include_untracked = true`. If the checkout fails, run `stash_pop(0)` and return `"{git error}\nYour changes are back."`. If the pop also fails, return the git error plus "Your changes are saved in the stash 'Switching to <name>'." Dialog: "You have changes on <current>." with buttons "Bring my changes", "Stash and switch" and "Cancel".

- [ ] **Step 4: Run** `cargo test -p gui_app` · Expected: PASS.

- [ ] **Step 5: Commit** `feat(gui): ask whether to bring or stash changes when switching branches`

---

### Task 11: Revert a commit (D5)

**Files:**
- Modify: `core_domain/src/ports.rs`, `git_adapter/src/lib.rs`, `git_adapter/src/tests.rs`, `gui_app/src/jobs.rs` (`HistoryAction`), `gui_app/src/app.rs` (`HistoryPrompt`, `HistoryAsk`, `Message`), `gui_app/src/view/confirm.rs`, `gui_app/src/context_menu.rs`, `test_support/src/lib.rs`

**Interfaces:**
- Produces: port `fn revert(&self, commit_id: &str) -> Result<(), DomainError>`. `HistoryPrompt::Revert { id }`, `HistoryAsk::Revert`, `HistoryAction::Revert { id }`, `Message::AskRevert`, `Message::ConfirmRevert`, `ContextActionKind::Revert { id }`.
- `context_actions` needs to know merge commits: add a `commits: &[Commit]` parameter, or pass `is_merge: bool` in `ContextTarget::Commit`. **Decision:** add `parents: usize` to `ContextTarget::Commit`, filled where the target is built in `view/history.rs`.

- [ ] **Step 1: Failing tests**

```rust
// git_adapter
#[test] fn revert_records_the_inverse_commit() { /* file back to before; new HEAD subject starts "Revert \"" */ }
#[test] fn a_conflicting_revert_leaves_the_revert_in_progress() { /* status().operation == Some(RepoOperation::Revert) */ }
// gui_app
#[test] fn revert_of_a_merge_commit_is_disabled_with_a_reason() {
    // label "Revert… (merge commit: use git revert -m in the terminal)", enabled false
}
```

- [ ] **Step 2: Run** · Expected: FAIL.

- [ ] **Step 3: Implement**

`git revert --no-edit -- <hex id>`; refuse non-hex ids as `cherry_pick` does. Conflicts go through the existing `stopped_on_conflicts` path. Confirm copy: "Revert this commit?" … "A new commit undoes its changes on the current branch. History is not rewritten."

- [ ] **Step 4: Run** `cargo test -p git_adapter -p gui_app` · Expected: PASS.

- [ ] **Step 5: Commit** `feat: revert a commit from the graph`

---

### Task 12: Find and restore lost stashes (E2)

**Files:**
- Create: `gui_app/src/view/lost_stashes.rs`
- Modify: `core_domain/src/model.rs` (`LostStash`), `core_domain/src/ports.rs`, `git_adapter/src/stash.rs`, `git_adapter/src/tests.rs`, `gui_app/src/jobs.rs`, `gui_app/src/app.rs`, `gui_app/src/view/mod.rs`, `gui_app/src/actions.rs`, `test_support/src/lib.rs`

**Interfaces:**
- Produces: `pub struct LostStash { pub commit_id: String, pub message: String, pub timestamp: i64 }`. Port `fn lost_stashes(&self) -> Result<Vec<LostStash>, DomainError>` and `fn apply_stash_commit(&self, commit_id: &str) -> Result<(), DomainError>`. `GitApp::lost_stashes: Option<LostStashesView>` with `enum LostStashesView { Searching, Found(Vec<LostStash>), Failed(String) }`. Messages `FindLostStashes`, `LostStashesFound(SessionKey, Result<Vec<LostStash>, DomainError>)`, `ApplyLostStash(String)`, `CloseLostStashes`. `ActionId::FindLostStashes`.

- [ ] **Step 1: Failing adapter test**

```rust
#[test] fn a_dropped_stash_is_found_and_restored_without_dropping_anything() {
    // stash_push("keep me", true); stash_drop(0); lost_stashes() → one, message contains "keep me";
    // apply_stash_commit(id) → file content back; stashes() still empty
}
#[test] fn ordinary_dangling_commits_are_not_stashes() { /* amend a commit → old one dangling, not listed */ }
```

- [ ] **Step 2: Run** · Expected: FAIL.

- [ ] **Step 3: Implement**

`git fsck --no-reflogs --no-progress --unreachable` lines of the form `unreachable commit <sha>`. Exclude the ids in `git stash list --format=%H`. Keep a commit only when `git log -1 --format=%P%x00%s%x00%ct <sha>` shows at least 2 parents and the subject starts with `WIP on ` or `On `. Sort newest first. `apply_stash_commit` = `git stash apply <hex sha>`. Run the search in the background; it can take seconds on large repositories, so the dialog shows "Searching…" until the result arrives.

- [ ] **Step 4: Dialog** lists each stash: message, relative age, short hash, and an "Apply" button. The empty state reads "No lost stashes. Dropped stashes stay findable until git garbage-collects them." Esc closes it.

- [ ] **Step 5: Run** `cargo test -p git_adapter -p gui_app` · Expected: PASS.

- [ ] **Step 6: Commit** `feat: find stashes that were dropped and apply them again`

---

### Task 13: Milestone check

- [ ] **Step 1:** `cargo test --workspace` · Expected: PASS. `cargo clippy --workspace -- -D warnings` · Expected: clean.
- [ ] **Step 2:** The `perf_harness` test module (`gui_app/src/view/mod.rs` ≈215) stays within its budgets.
- [ ] **Step 3: Manual pass** on `target/debug/git_cthulhu.exe` against the Backend repo (1,100 branches, 16 worktrees):
  - commit with a body;
  - amend, then undo;
  - commit from the terminal pane while Undo is open (expect "HEAD moved");
  - filter plus Ctrl+S;
  - walk the graph and the changes list with the arrow keys;
  - publish a new branch, then force push after an amend;
  - switch branches with changes, both ways;
  - revert a commit;
  - drop a stash, then find and apply it.
- [ ] **Step 4:** Update `docs/analysis/2026-11-21-gitdesktop-competitive-analysis.md` §2 rows to ✓ for what shipped, and commit `docs: daily-driver milestone shipped`.
