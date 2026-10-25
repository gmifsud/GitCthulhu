# Task 3 fix report

**Status:** DONE
**Commit:** `30a857a` (on top of `c26a1c6`, not amended)

## What changed
- `gui_app/src/jobs.rs`: `open_repository_in_background`'s interrupted fallback now echoes the requested path instead of `String::new()` (same as the success path). The only other caller (`RepositoryOpened`) ignores the path on error, so it is unaffected.
- `gui_app/src/app.rs` `worktree_opened`: if the result does not match the pending open AND its path is empty, `worktree_opening` is cleared and the error is shown. Mismatched non-empty paths are still dropped without touching the lock, so a stale result from a replaced repository cannot release a newer in-flight open.
- `AskCreateWorktree` and `ask_remove_worktree` clear `self.error` once the prompt is actually shown. Rejected asks (busy, main worktree, only open tab) leave the error alone, because those paths set an error that is meant to be seen.

## Tests
New:
- `an_empty_path_open_failure_releases_the_open_lock`: OpenWorktree, then `WorktreeOpened("", Err)`, then a second OpenWorktree is accepted.
- `asking_to_create_or_remove_clears_a_stale_error`
- `jobs::an_interrupted_open_echoes_the_requested_path` (panicking opener)

Results:
- `cargo test -p gui_app -p app`: all pass (gui_app 181, plus 11 in the other crate).
- `cargo clippy -p gui_app -p app --all-targets -- -D warnings`: clean.

## Concerns
- `cargo fmt -p gui_app` reformats unrelated files in the working tree, so the tree is not fmt-clean from earlier tasks. I reverted that noise and committed only the two files touched.
- The interrupted-open test panics a worker thread on purpose, so a panic message may appear in test stderr.
- `.superpowers/` is untracked and was not committed.
- Out-of-scope items from the brief were left alone.
