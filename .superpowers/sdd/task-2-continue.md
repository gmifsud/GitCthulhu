# Task 2 continue brief — finish WP1-b from partial tree

Previous implementer stalled. **Do not revert** the uncommitted progress below. Complete Task 2 and commit.

## Already done (uncommitted)

1. `core_domain`: `WorktreeTabRetention { Full, Partial, PathOnly }` with `#[default] Partial`, field on `AppSettings`.
2. `config_adapter`: DTO field + From impls (+ any round-trip test started in `app/src/startup.rs` / config tests).
3. `gui_app/src/lib.rs`: `mod session;`
4. `gui_app/src/session.rs`: **tests only** for `apply_retention` — **missing** `WorktreeSession`, `apply_retention`, `test_stub`. File will not compile until you add the impl above the `#[cfg(test)]` module.

## Still required (from original Task 2 brief)

Read: `d:\Repos\GitCthulhu\.superpowers\sdd\task-2-brief.md`

1. Implement `WorktreeSession` + `apply_retention` so existing tests in `session.rs` pass (TDD: they already exist — make them green).
2. Refactor `GitApp` to `sessions: Vec<WorktreeSession>`, `active: usize`, `worktree_catalogue`. Welcome = `sessions.is_empty()`. Accessors `session()` / `session_mut()` / `session_mut_by_path`.
3. Move per-repo fields into the session (state, adapter, selection, commit_message, marks, patches, …) per the design spec Session model section.
4. Path-routed finishes: every git `*Finished` / `RepositoryLoaded` carries `path`; handlers resolve by path; unknown path = no-op.
5. `activate` applies retention then switches; tests for switch leaving other session untouched; finish for unknown path dropped.
6. Settings UI control for retention + persist via existing saver (extend `save_appearance` / settings saver in `app` to copy `worktree_tab_retention`).
7. Green: `cargo test -p gui_app -p config_adapter -p core_domain`
8. Commit + write report to `.superpowers/sdd/task-2-report.md`

## Out of scope

Tab strip, explorer Worktrees UI, create/remove modals (Task 3).
