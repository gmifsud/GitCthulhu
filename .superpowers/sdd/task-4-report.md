# Task 4 Report: Hunk modes Beside / Wide / Focus

**Status:** Complete  
**Base:** `aca294a`  
**Commit:** `b126b13`

## Delivered

- `HunkMode` (`Beside` default, `Wide`, `Focus`) in `layout.rs` with `for_hunk_mode` ratio bumps for Wide (`WIDE_GRAPH_RATIO`, `WIDE_MIDDLE_RATIO`).
- `FocusRestore { files_rail_open, inspector_open, hunk_mode }` on `GitApp`; `inspector_open` default `true`.
- Messages `SetHunkMode`, `CycleHunkMode` (Beside ↔ Wide; Focus explicit only).
- `install_layout` builds panes from `show_files` + `show_inspector`; Focus closes both rails.
- `handle_escape`: after context menu, restores Focus from snapshot before tips/prompts.
- Top bar: B / W / F segment beside layout preset control.
- Workspace: no collapsed explorer strip in Focus.

## Tests added (`gui_app` app module)

- `enter_focus_remembers_rails_and_escape_restores`
- `cycle_hunk_mode_skips_focus`
- `wide_mode_expands_the_diff_split`

All `cargo test --lib` in `gui_app`: **207 passed**.

## Notes / concerns

- Wide uses fixed ratio floors, not user-resized Wide overrides persisted separately from Beside ratios (same `ratios` array; Wide applies transient boost at install time).
- `CycleHunkMode` while in Focus exits to Beside and restores rails (reasonable UX; not spelled out in brief).
- Hunk toolbar labels are single letters; full mode names live in `HunkMode::label()` for future tooltips.
