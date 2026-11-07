# ADR 0007: `view` is proportional to what is visible

Status: Accepted (2026-11-07)

## Context

iced rebuilds the whole widget tree after every message. Three sprints in a row cut the cost of one pane per rebuild (the graph window, the measured history rows, the text cache) while the window was still rebuilt at pointer rate: every mouse move was a message. Each sprint looked like the last because the number of rebuilds was never addressed, only the price of each.

On a repository with 1,100 branches, 16 worktrees and 5,000 loaded commits, the Explorer built every branch row on every rebuild and looked each row's branch up by scanning the whole list, and every history chunk re-read the log from the newest commit and rebuilt every row.

## Decision

1. **No message for a pointer move.** The event subscription records the pointer in `gui_app::pointer` and sends nothing. The one case that must follow the pointer, dragging the terminal edge, subscribes to moves as messages only while the drag lasts.
2. **Nothing in `view` may grow with the repository.** Work proportional to branches, commits, tags or worktrees is done once per state change into a cached index (as `history_index` does, keyed by a generation that the state bumps), or kept to the rows in view. A per-row lookup is a map, never a scan.
3. **History is appended, not re-read.** Older commits arrive from a cursor that continues the walk; a chunk never re-walks what is already loaded.
4. **Every change of this kind is measured first.** `GITCTHULHU_PERF=1` prints each message's `update` and `view` cost; the `perf_harness` test reports the window's build time on synthetic data; the `profiling` cargo profile keeps symbols for a profiler.

## Why

The window is rebuilt on every state change, and that is the right model for a Git client: state changes are rare and come from git or the user, not from the pointer. Once rebuilds happen only then, each pane's cost is paid per change, not per frame, and the `lazy` widget is not needed. The quadratic Explorer and the re-read history were the two places where a single change still cost time proportional to the whole repository.

## Consequences

- A new pane that lists repository objects must window its rows or cache its index before it is merged. The `perf_harness` numbers go in the commit message.
- `Message` carries no pointer position. A widget that needs one reads `pointer::position()` in its handler.
- The iced 0.14 upgrade, which fixes the `Wrapping::None` bug behind `one_line`, is its own change so its effect is measured on the same baseline.
