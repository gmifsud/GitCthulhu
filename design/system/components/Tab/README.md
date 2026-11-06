# Tab

A worktree session in the strip under the header: the folder name over its branch, with a close button and, at the end, an add button that opens the picker. Hand-written from `gui_app/src/view/tabs.rs`.

**Anatomy.** Two lines 1 px apart: the folder's basename in `body` and the branch in `compact` (body − 2). A `close` glyph at `2px 6px`, `radius-xs`. The cell is `5px 8px`, `radius-md`, `hairline` border; tabs sit `pane-gap` apart in a `bg-chrome` bar padded `6px 8px` with a hairline under it.

**States.** Inactive: `chip` fill, `chip-border` edge, name `text-secondary`, branch `text-muted`; hover lifts the fill to `bg-raised`. Active: `bg-raised` fill, `accent-primary` edge, name `text-primary`, branch `accent-primary`. The close button is `text-muted` and inert on the last tab or while that session has a job running; hover fills it with `border`. The add button is a bordered `add` glyph at `6px 10px`; open, it takes `chip` fill, `accent-primary` edge and glyph.

Hover on a tab shows its full path and branch in a `mono-caption` tooltip.

**What the consumer provides.** The sessions (path, branch, loading), which is active, whether the picker is open.

**Don't** show a tab without its branch line, or let the strip wrap.
