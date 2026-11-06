# StatusBar

The strip at the bottom of the window: a state dot, what the app is doing (or the last error), then commit count, upstream and encoding on the right. Hand-written from `gui_app/src/view/status_bar.rs`.

**Anatomy.** `status-bar` 24 tall (72 with an error expanded), `bg-chrome`, `hairline` border in `border`, `bar-inset` horizontal, items `tree-indent` 14 apart, all in `compact` (body − 2). Leftmost is a `status-dot` 7 px disc: `added` when idle, `accent-primary` while a job runs.

**Activity.** A running job shows `refresh` and its label with elapsed seconds in `accent-primary`. For a few seconds after it finishes: `check` in `added` with `· done in 1.2 s`, `close` in `removed` with `· failed: <first line>`, or `stop` in `text-muted` with `· stopped`. Otherwise the plain state (`Ready`, `Loading…`). The whole cluster is a quiet button that opens the Activity tab; hover fills it with `border` at `radius-xs`.

**Error.** Replaces the activity: the error's first non-empty line in `accent-primary` on one line, then a `details` fold (`chevron-right`/`chevron-down`) in `text-muted` and a `close` to dismiss. Expanded, the bar grows to 72 and the full text wraps.

**Right side.** `N commits`, the upstream name or `No upstream`, `UTF-8`, in `text-muted`.

**What the consumer provides.** The activity log (running entry, recent result), the current error and whether it is expanded, the commit count and upstream.

**Don't** colour an error red (it is the ember, as the thing needing attention), or let the bar grow for anything but an expanded error.
