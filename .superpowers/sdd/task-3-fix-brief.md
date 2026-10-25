### Task 3 fix: worktree_opening lockout (+ clear error on ask)

Review: Approved with Important finding. Fix before Task 4.

Work from: `d:\Repos\GitCthulhu` on `feat/worktree-sessions-wp1` (HEAD `c26a1c6`).

## Important — `worktree_opening` lockout

**Location:** `gui_app/src/app.rs` (`worktree_opened`), `gui_app/src/jobs.rs` (`open_repository_in_background` or the worktree-open job).

**Bug:** Interrupted-thread fallback returns `(String::new(), Err(..))`. `worktree_opened` fails `same_path(pending, "")` and drops the result **without clearing** `worktree_opening`. Later `OpenWorktree` calls (including create-success) silently return `Task::none()`.

**Fix (prefer both if cheap):**
1. Clear `worktree_opening` when the returned path is empty / does not match pending.
2. Or have the job echo the requested path on failure (like `worktrees_in_background`).

**Test:** After `OpenWorktree`, send `WorktreeOpened("", Err(..))` (or whatever the empty-path failure shape is), then assert a second `OpenWorktree` is accepted (opens / sets `worktree_opening` again).

## Minor (do in same pass)

- Clear `self.error` when asking create/remove (`AskCreateWorktree` / `AskRemoveWorktree`) so a stale status-bar error is not shown inside the modal.

## Out of scope

- Catalogue fallback when runner session closed
- Create-success while open in flight
- WorktreesLoaded ordering
- Visual layout / Task 4

## Verify + commit

```bash
cargo test -p gui_app -p app
cargo clippy -p gui_app -p app --all-targets -- -D warnings
```

New commit (do not amend). Report: `.superpowers/sdd/task-3-fix-report.md`
