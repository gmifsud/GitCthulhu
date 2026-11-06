# CommitGraph

The lane drawing in the History column, painted on a canvas one row at a time so the slices join into continuous lines. Hand-written from `gui_app/src/commit_graph.rs`.

**Geometry.** Rows are `graph-row` 24 tall; lane centres sit `lane-pitch` 16 apart, lane 0 against the left gutter so pills can touch the tip. Lines are `graph-line` 2 wide; a commit is a `graph-dot` 4 radius disc at the row's middle.

**Colour.** A lane's colour is `lane-<index mod 6>`: ember, blue, green, violet, pink, teal. Lane 0 is the ember, so the tip lane and the brand agree. Every segment (passing, incoming, outgoing) takes its own lane's colour, not the row's.

**Tip marks** (`NodeMark`, chosen by the dominant ref at the commit: local or worktree beat remote):

- Plain — the dot.
- Local (including HEAD) — the dot inside a `graph-ring` 6.5 ring in the lane colour.
- Worktree — as Local, with the ring stroked the same way (the pill beside it says which).
- Remote — a ring at dot + 1 stroked `graph-line` wide, hollow.

**What the consumer provides.** The `GraphRow` (lane, incoming, passing, outgoing lanes) and the `NodeMark`; the geometry is cached per row and redrawn only when the row changes.

**Don't** recolour lanes by author or branch, draw curves (segments are straight within a row), or add arrowheads.
