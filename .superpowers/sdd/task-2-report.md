# Task 2 report — Fused right files navigator

**Status:** DONE  
**Branch:** feat/worktree-sessions-wp1  
**Commits:** `decd0d6 feat: fuse WIP and commit files into the right navigator`  
**Base (start):** `74b562888b26fdd2743656799fa49742722d28e6`

## Implemented

- **`files_nav.rs`:** `show_wip_block`, `show_commit_block`, `files_navigator`, `commit_files_block` (moved from `workspace.rs`); commit/tag meta helpers; empty hint copy per brief.
- **`changes.rs`:** `wip_working_tree_block` with `Working tree · N files` title; shared `wip_toolbar` + `wip_file_sections` / `section_block`; legacy `changes_pane` kept refactored but unused (left rail still explorer-only).
- **`workspace.rs`:** Inspector body is `files_navigator` only; removed `detail_body`, `commit_files_block`, `inspector_working_hint`, and the old commit-gated working-tree hint.
- **`mod.rs`:** `mod files_nav`.

## Behaviour

| Selection | Status | Right navigator |
|-----------|--------|-----------------|
| Commit | clean | Commit meta + patch file list |
| Commit | dirty | WIP block stacked above commit meta + files |
| Branch / other | dirty | WIP only |
| Branch / other | clean | Quiet empty hint |
| Tag | (patches loaded) | Tag label + patch file list (via `show_tag_block`, not `show_commit_block`) |

Staging (message, stage/unstage, tree/list) lives in the right WIP block when the tree is dirty.

## Tests (TDD)

- `wip_block_hidden_when_clean`, `wip_block_shown_when_dirty`, `commit_block_for_commit_selection_only` in `files_nav::tests`.
- `cargo test -p gui_app files_nav` — 3 passed.
- `cargo test -p gui_app` — 204 passed.

## Self-review

- **Verdict:** Meets brief; staging restored on the right with soft-lock WIP visibility.
- **Tag vs commit:** `show_commit_block` is commit-only (per tests); tag file list uses a parallel path so tag inspection is unchanged.
- **Dead code:** `changes_pane`, unused `section_label` / refs helpers — pre-existing from Task 1; not expanded in this task.
- **Not in scope:** Hunk modes (Task 4), graph reorder (Task 3).

## Manual checks (recommended)

- Dirty working tree + commit selected: WIP above commit files.
- Clean + commit: commit files only, no WIP.
- Stage/unstage from right toolbar with dirty tree.
