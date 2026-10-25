# Task 3 report — Graph column reorder + anti-deformation

**Status:** DONE  
**Branch:** feat/worktree-sessions-wp1  
**Commits:** `d399d27 feat: graph pills-first columns with hash last`  
**Base (start):** `decd0d668beb5c2d97cf179cbe5bb51135317e09`

## Implemented

- **`history.rs`:** Column order `Refs (pills) | Graph | Description | Author | When | Hash | focus`.
- Width constants per brief (`HASH_WIDTH` 72, `AUTHOR_WIDTH` 100, `MIN_MESSAGE_WIDTH`, `PILLS_MIN_WIDTH`).
- Split `description_with_pills` into `pills_cell` and `message_cell`; `fill_column_with_min` uses zero-height `Space` for min-width floors on flexible columns.
- All header labels use `Wrapping::None` (including Graph).
- WIP rows: wip pill in Refs column, empty graph spacer, message only in Description, empty hash last.

## Tests

- `cargo test -p gui_app` — **204 passed**, 0 failed.

## Self-review

- **Verdict:** Meets brief; `ref_pills.rs` unchanged.
- **Manual:** No GUI screenshot at ~1200px — header wrap and hash-rightmost layout unverified by eye.

## Concerns

- `fill_column_with_min` reserves min width via a fixed `Space` sibling; behaviour under extreme narrow panes should be checked visually once.
