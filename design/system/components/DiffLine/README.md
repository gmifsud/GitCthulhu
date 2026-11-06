# DiffLine

One line of a patch: gutter, mark, text, on a solid wash that says what happened to it. Hand-written from `gui_app/src/theme.rs` (the diff tokens), `gui_app/src/diff_view.rs` (row heights) and `gui_app/src/view/patch.rs` (column widths).

**Anatomy.** `diff-row` 20 tall in `mono`; a `line-number-column` 44 gutter in `text-muted` at `compact`, a 14 px mark column (`+`, `−`, or nothing), a 1 px divider in a side-by-side view, then the text at `0 6px`. File headers are `diff-file-row` 26 on `bg-sidebar` with a hairline under; hunk headers `diff-hunk-row` 24 in `text-muted`; notes 22.

**Washes.** Added: `added` ink on `added-bg`. Removed: `removed` ink on `removed-bg`. A changed pair in the side-by-side view: `text-primary` on `modified-bg`. The washes are opaque — never alpha over the pane — so the inks are tuned to them. Word-level changes inside a line get `added-word-bg`, `removed-word-bg` or `modified-word-bg` behind the words only.

**What the consumer provides.** Line numbers (old and new), the kind, the text, and the changed-word ranges if any.

**Don't** colour the line numbers with the line's ink, use green and red for anything that is not a line change, or tint context lines.
