# Layout, explorer, fused files, graph density Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reshape the iced shell so the left rail is a collapsible refs/worktrees accordion, the right pane is the fused WIP+commit files navigator, the graph is dense (pills→node→…→hash), and hunk layout/visuals stop deforming the window.

**Architecture:** No new ports. Move staging UI from `files_rail` into the Inspector surface as stacked WIP/Commit blocks; rebuild left rail as accordion sections with per-session collapse flags; reorder history columns and clamp mins; add `HunkMode` on `GitApp` with Focus restore snapshot; tighten unified diff chrome. Split/side-by-side remains ADR 0003 WP3 (out of this plan).

**Tech Stack:** Rust, iced 0.13 (`pane_grid`, `scrollable::Scrollbar`), `gui_app`, existing `DesignTokens`.

## Global Constraints

- ADR 0001: `gui_app` never names `git_adapter`.
- ADR 0002: repository data enters via `AppState::apply_event` only.
- ADR 0003: session owns `AppState` and UI bag; window chrome (`hunk_mode`) may live on `GitApp`.
- Spec: `docs/superpowers/specs/2026-10-25-layout-explorer-diff-design.md` (approved).
- Soft locks from review: **TAGS** is its own accordion section; **hide WIP block when clean**.
- British English in user-facing copy.
- Worktrees scrollbar must stay embedded (spacing) so × is never covered.

## File map

| File | Responsibility |
|---|---|
| `gui_app/src/session.rs` | `explorer_collapsed` flags on session |
| `gui_app/src/app.rs` | `HunkMode`, focus restore, messages, Escape |
| `gui_app/src/layout.rs` | Ratio floors if Wide needs tighter inspector min |
| `gui_app/src/view/explorer.rs` (new) | Left accordion: LOCAL/REMOTE/WORKTREES/STASHES/TAGS |
| `gui_app/src/view/refs.rs` | Branch/remote/tag/stash rows reused by explorer |
| `gui_app/src/view/worktrees.rs` | Worktrees list only (no left Changes coupling) |
| `gui_app/src/view/changes.rs` | Staging list widgets; called from right navigator |
| `gui_app/src/view/files_nav.rs` (new) | Right fused navigator: WIP stack + commit stack |
| `gui_app/src/view/workspace.rs` | Wire explorer left, files_nav right; hunk modes |
| `gui_app/src/view/history.rs` | Column order, anti-wrap, pills leading |
| `gui_app/src/view/patch.rs` | Unified hunk visual refresh |
| `gui_app/src/theme.rs` | Stronger diff tint if needed |
| `gui_app/src/view/mod.rs` / `lib.rs` | Module exports |
| `gui_app/src/view/top_bar.rs` | Optional hunk-mode control |

---

### Task 1: Explorer collapse state + left accordion shell

**Files:**
- Modify: `gui_app/src/session.rs`
- Create: `gui_app/src/view/explorer.rs`
- Modify: `gui_app/src/view/workspace.rs` (`files_rail`)
- Modify: `gui_app/src/app.rs` (messages)
- Modify: `gui_app/src/view/mod.rs`
- Test: `gui_app/src/session.rs` (unit) / `gui_app` tests in `app.rs` if preferred

**Interfaces:**
- Produces: `ExplorerSection { Local, Remote, Worktrees, Stashes, Tags }`, `ExplorerCollapsed { local, remote, worktrees, stashes, tags }` on `WorktreeSession`, `Message::ToggleExplorerSection(ExplorerSection)`
- Consumes: existing branch/worktree/stash data on session + `worktree_catalogue`

- [ ] **Step 1: Add failing session test for default collapse + toggle**

In `session.rs` tests (or a small `explorer` test module):

```rust
#[test]
fn explorer_collapsed_defaults_open_except_optional() {
    let session = WorktreeSession::new(/* path, adapter as existing tests */);
    assert!(!session.explorer_collapsed.local);
    assert!(!session.explorer_collapsed.remote);
    assert!(!session.explorer_collapsed.worktrees);
    assert!(!session.explorer_collapsed.stashes);
    assert!(!session.explorer_collapsed.tags);
}

#[test]
fn toggle_explorer_section_flips_one_flag() {
    let mut session = WorktreeSession::new(/* … */);
    session.toggle_explorer(ExplorerSection::Stashes);
    assert!(session.explorer_collapsed.stashes);
    assert!(!session.explorer_collapsed.local);
}
```

- [ ] **Step 2: Run test — expect fail (types missing)**

Run: `cargo test -p gui_app explorer_collapsed -- --nocapture`  
Expected: compile fail or test not found.

- [ ] **Step 3: Implement types + session helpers**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExplorerSection {
    Local,
    Remote,
    Worktrees,
    Stashes,
    Tags,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct ExplorerCollapsed {
    pub local: bool,
    pub remote: bool,
    pub worktrees: bool,
    pub stashes: bool,
    pub tags: bool,
}

impl WorktreeSession {
    pub(crate) fn toggle_explorer(&mut self, section: ExplorerSection) {
        match section {
            ExplorerSection::Local => self.explorer_collapsed.local = !self.explorer_collapsed.local,
            ExplorerSection::Remote => self.explorer_collapsed.remote = !self.explorer_collapsed.remote,
            ExplorerSection::Worktrees => {
                self.explorer_collapsed.worktrees = !self.explorer_collapsed.worktrees
            }
            ExplorerSection::Stashes => {
                self.explorer_collapsed.stashes = !self.explorer_collapsed.stashes
            }
            ExplorerSection::Tags => self.explorer_collapsed.tags = !self.explorer_collapsed.tags,
        }
    }
}
```

Default `explorer_collapsed: ExplorerCollapsed::default()` in `WorktreeSession::new`.

- [ ] **Step 4: Wire `Message::ToggleExplorerSection` → `session_mut().toggle_explorer`**

- [ ] **Step 5: Build `explorer_pane` accordion**

Headers: `▸/▾` + `LOCAL` / `REMOTE` / `WORKTREES` / `STASHES` / `TAGS` + count.  
Body: reuse row builders from `refs.rs` / `worktrees_section` list (extract list-without-title helpers if needed).  
`files_rail` becomes: collapse rail button + `explorer_pane` only — **remove** `changes_pane` and `branch_switcher` duplication if the branch chip moves to top bar only (keep top-bar branch chip; left LOCAL lists branches).

- [ ] **Step 6: Keep worktrees scrollbar embedded**

```rust
scrollable(rows).direction(Direction::Vertical(
    Scrollbar::new().width(8).scroller_width(8).spacing(6),
))
```

- [ ] **Step 7: `cargo test -p gui_app` for new tests; manual: collapse each section**

- [ ] **Step 8: Commit**

```bash
git add gui_app/src/session.rs gui_app/src/app.rs gui_app/src/view/explorer.rs gui_app/src/view/workspace.rs gui_app/src/view/mod.rs gui_app/src/view/refs.rs gui_app/src/view/worktrees.rs
git commit -m "$(cat <<'EOF'
feat: collapsible LOCAL/REMOTE/WORKTREES/STASHES/TAGS explorer

EOF
)"
```

---

### Task 2: Fused right files navigator (WIP stacked above commit)

**Files:**
- Create: `gui_app/src/view/files_nav.rs`
- Modify: `gui_app/src/view/workspace.rs` (`inspector` → files navigator)
- Modify: `gui_app/src/view/changes.rs` (export sections without left chrome)
- Modify: `gui_app/src/view/mod.rs`
- Test: `gui_app` unit tests for visibility helpers

**Interfaces:**
- Produces: `fn files_navigator(...)`, `fn show_wip_block(status) -> bool`, `fn show_commit_block(selection) -> bool`
- Consumes: session `current_status`, `marked_files`, `commit_message`, `selection`, `patches` / `patch_stats`

- [ ] **Step 1: Failing tests for visibility**

```rust
#[test]
fn wip_block_hidden_when_clean() {
    assert!(!show_wip_block(None));
    assert!(!show_wip_block(Some(&clean_status())));
}

#[test]
fn wip_block_shown_when_dirty() {
    assert!(show_wip_block(Some(&dirty_status())));
}

#[test]
fn commit_block_for_commit_selection_only() {
    assert!(show_commit_block(&Selection::Commit { id: "abc".into() }));
    assert!(!show_commit_block(&Selection::Branch {
        name: "main".into(),
        current: true,
    }));
}
```

Put helpers in `files_nav.rs` (or `changes.rs`) as `pub(crate)` pure functions.

- [ ] **Step 2: Run tests — expect fail**

Run: `cargo test -p gui_app show_wip_block -- --nocapture`

- [ ] **Step 3: Implement navigator layout**

```text
column![
  [optional] compact head / commands for inspector place,
  if show_wip: wip_block (commit message + changes sections from changes.rs),
  if show_commit: commit_block (meta + commit_files_block moved from workspace.rs),
  if neither: quiet "Select a commit or make a change."
]
```

Move `commit_files_block` from `workspace.rs` into `files_nav.rs`.  
WIP uses existing `changes_pane` innards without the left “Changes · N” duplicate title — title becomes `Working tree` / counts.

- [ ] **Step 4: Point `inspector()` body at `files_navigator`**

Remove old working-tree-only hint that blocks commit files when not commit-selected if it conflicts — commit block gates itself.

- [ ] **Step 5: Ensure left rail no longer mounts `changes_pane`**

- [ ] **Step 6: Tests pass; manual: dirty+commit shows WIP above commit files; clean+commit shows commit only; stage from right**

- [ ] **Step 7: Commit**

```bash
git commit -m "$(cat <<'EOF'
feat: fuse WIP and commit files into the right navigator

EOF
)"
```

---

### Task 3: Graph column reorder + anti-deformation

**Files:**
- Modify: `gui_app/src/view/history.rs`
- Modify: `gui_app/src/ref_pills.rs` only if pill row helper needs a non-description home

**Interfaces:**
- Produces: row layout `pills | graph | message | author | when | hash`
- Consumes: `refs_at`, `GraphCell`, existing widths

- [ ] **Step 1: Adjust constants**

```rust
const HASH_WIDTH: f32 = 72.0;
const WHEN_WIDTH: f32 = 56.0;
const AUTHOR_WIDTH: f32 = 100.0;
const MIN_MESSAGE_WIDTH: f32 = 120.0; // use in header/message cells via width hints
const PILLS_MIN_WIDTH: f32 = 48.0;
```

- [ ] **Step 2: Rewrite `column_headers` order**

Order: `""` or `Refs` (pills col) → `Graph` → `Description` → `Author` → `When` → `Hash` → focus.  
Every header: `.wrapping(Wrapping::None)`.

- [ ] **Step 3: Rewrite commit row body**

```rust
row![
  pills_cell(&pills, …),          // was inside description_with_pills
  canvas(GraphCell { … }).width(lanes_width).height(ROW_HEIGHT),
  single_line(message, Length::Fill, …),  // message only
  single_line(author, Length::Fixed(AUTHOR_WIDTH), …),
  single_line(when, Length::Fixed(WHEN_WIDTH), …),
  single_line(short_hash, Length::Fixed(HASH_WIDTH), …),
]
.spacing(COL_GAP)
```

WIP rows: empty hash; pills/WIP label in pills column; empty or spacer graph.

- [ ] **Step 4: Delete pills from `description_with_pills` (rename to message cell)**

- [ ] **Step 5: Manual / screenshot check at ~1200px wide — no vertical header wrap; hash rightmost**

- [ ] **Step 6: Commit**

```bash
git commit -m "$(cat <<'EOF'
feat: graph pills-first columns with hash last

EOF
)"
```

---

### Task 4: Hunk modes Beside / Wide / Focus

**Files:**
- Modify: `gui_app/src/app.rs`
- Modify: `gui_app/src/layout.rs` (optional min clamps)
- Modify: `gui_app/src/view/workspace.rs`
- Modify: `gui_app/src/view/top_bar.rs` (mode control)
- Test: `gui_app` app tests for mode + Escape restore

**Interfaces:**
- Produces:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum HunkMode {
    #[default]
    Beside,
    Wide,
    Focus,
}

pub(crate) struct FocusRestore {
    pub files_rail_open: bool,
    pub hunk_mode: HunkMode, // prior non-Focus mode
}
```

- Messages: `SetHunkMode(HunkMode)`, `CycleHunkMode`
- Escape: if `hunk_mode == Focus`, restore from `focus_restore` before other escape handlers that close menus

- [ ] **Step 1: Failing tests**

```rust
#[test]
fn enter_focus_remembers_rails_and_escape_restores() {
    let mut app = /* fixture with files_rail_open true, Beside */;
    let _ = app.update(Message::SetHunkMode(HunkMode::Focus));
    assert_eq!(app.hunk_mode, HunkMode::Focus);
    assert!(!app.files_rail_open); // left hidden in Focus
    // right inspector hidden via install_layout / flag — assert the flag you introduce
    let _ = app.update(Message::Escape);
    assert_eq!(app.hunk_mode, HunkMode::Beside);
    assert!(app.files_rail_open);
}

#[test]
fn cycle_hunk_mode_skips_focus() {
    // Beside -> Wide -> Beside (Focus only via explicit Set/Focus button)
}
```

- [ ] **Step 2: Implement state + `install_layout` behaviour**

- `Beside`: current default presets (explorer | graph/diff | files nav).
- `Wide`: bump graph/middle ratios so Diff grows; keep right pane ≥ `MIN_FILES_RATIO`-style floor (reuse inspector ratio clamp).
- `Focus`: hide left explorer and right navigator (`files_rail_open = false` plus `inspector_open = false` **or** rebuild panes with only Graph+Diff — prefer a dedicated `inspector_open: bool` default true, mirrored to `files_rail_open` naming cleanup only if cheap; otherwise stash both in `FocusRestore` and set left+right closed).

Simplest acceptable approach: store `FocusRestore { files_rail_open, inspector_open, hunk_mode }`; on Focus set both rails closed and `install_layout()`; Diff fills. If today Inspector cannot close, add `inspector_open: bool` (default `true`) parallel to `files_rail_open`.

- [ ] **Step 3: Top-bar or Diff header control** — three-state or cycle button labelled `Hunk` / icons; tooltip lists modes.

- [ ] **Step 4: `handle_escape` — Focus restore first after context menu / tips**

Order already starts with detach/context; insert Focus restore immediately after context menu clear (or before) so Esc exits Focus without needing two presses when menu closed.

- [ ] **Step 5: Tests + manual Focus/Esc**

- [ ] **Step 6: Commit**

```bash
git commit -m "$(cat <<'EOF'
feat: hunk Beside/Wide/Focus layout modes

EOF
)"
```

---

### Task 5: Unified hunk visual refresh

**Files:**
- Modify: `gui_app/src/view/patch.rs`
- Modify: `gui_app/src/theme.rs` (`DIFF_TINT` or dedicated stronger constants)

**Interfaces:**
- Consumes: `DiffLineKind`, `DesignTokens::{added_bg, removed_bg, added, removed}`
- Produces: denser `diff_line` rows with gutter `+`/`-`/` `

- [ ] **Step 1: Raise tint**

In `theme.rs`:

```rust
const DIFF_TINT: f32 = 0.22; // was 0.12
```

- [ ] **Step 2: Gutter marker column**

In `diff_line`:

```rust
let mark = match kind {
    DiffLineKind::Added => "+",
    DiffLineKind::Removed => "-",
    DiffLineKind::Context => " ",
};
// row: [old#][new#][mark 12px][text]
```

Style mark with `added` / `removed` / `text_muted`.

- [ ] **Step 3: Tighten padding** — reduce vertical padding on line rows (e.g. `[0, 4]` → `[0, 2]`).

- [ ] **Step 4: Manual compare to GitKraken reference image (unified approximation)**

- [ ] **Step 5: Commit**

```bash
git commit -m "$(cat <<'EOF'
feat: denser unified hunk gutters and tints

EOF
)"
```

---

### Task 6: Full verification sweep

**Files:** none new

- [ ] **Step 1: `cargo test -p gui_app`**
- [ ] **Step 2: `cargo clippy -p gui_app -- -D warnings`** (or workspace clippy policy)
- [ ] **Step 3: `cargo build -p app --release`**
- [ ] **Step 4: Manual checklist from spec §Testing**
  - Accordion counts + independent collapse
  - Clean/dirty × commit selection stacking
  - Stage from right; commit file focuses diff
  - Graph headers / hash last / pills first
  - Focus + Esc
  - Worktrees × not covered
- [ ] **Step 5: Commit any fixups; mark plan tasks done**

---

## Spec coverage check

| Spec requirement | Task |
|---|---|
| Left LOCAL/REMOTE/WORKTREES/STASHES (+ TAGS) collapsible | 1 |
| Staging leaves left rail | 1–2 |
| Fused right navigator | 2 |
| WIP above commit when both | 2 |
| Hide WIP when clean | 2 |
| Graph pills→…→hash | 3 |
| Anti-wrap / density | 3 |
| Hunk Beside/Wide/Focus + Esc | 4 |
| Unified hunk visual refresh | 5 |
| Embedded worktrees scrollbar | 1 (preserve) |
| Split diff WP3 | Out of plan (non-goal) |

## Out of plan

- Side-by-side split diff / word highlights (ADR 0003 WP3).
- Compare References (WP2).
- Persisting layout / hunk mode / explorer collapse to disk.
