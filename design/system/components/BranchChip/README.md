# BranchChip

The checked-out branch as a pill in the header's branch row; pressing it opens the branch popup. Hand-written from `gui_app/src/view/chrome.rs` (`branch_chip`).

**Anatomy.** A `branch` glyph, the name on one line, then a chevron (`chevron-right` closed, `chevron-down` open) at body − 1 in `text-muted`; parts `control-gap` apart, padding `3px 10px`, `radius-pill`. The name clips at `branch-chip-max` (280 px); it never wraps.

**States.** Rest: `chip` fill, `chip-border` edge, glyph and name in `accent-primary`. Hover and open: `bg-raised` fill, `accent-primary` edge. Disabled (no repository, detached): `text-muted` glyph and name, no hover.

**What the consumer provides.** The branch name (or `Detached HEAD`), the message that toggles the popup, `enabled` and `open`.

**Don't** reuse the pill for anything but the current branch; branches elsewhere are ref pills in the graph (see RefPill), and a branch to pick is a row in the popup.
