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


