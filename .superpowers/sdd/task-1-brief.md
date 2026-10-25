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


