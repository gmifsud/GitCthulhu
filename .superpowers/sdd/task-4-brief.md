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


