# FileStatus

A changed file's row in Changes or Explorer: an icon, the name in the ink of its status, its folder in mono. Hand-written from `gui_app/src/theme.rs` (`change_ink`) and `gui_app/src/view/changes.rs`.

**Anatomy.** `body` name, a `file` or folder glyph in `text-muted` at `glyph-inline`, the path in `mono-compact` `text-muted`; parts `control-gap` apart, row padding `2px 6px`, each tree level indented `tree-indent` 14. In Changes a `checkbox` 14 leads the row to stage it. Hover fills the row with `bg-raised`.

**Inks**, after the JetBrains changes view: `vcs-modified` (blue) modified, `vcs-added` (green) added, `vcs-deleted` (grey) deleted, `vcs-untracked` (brick) untracked, `vcs-renamed` (teal) renamed, `vcs-conflicted` (red) conflicted. All read at 4.5:1 or better on `bg-app` and `bg-sidebar`.

The status is never colour alone: untracked files sit in their own section, conflicted ones carry their own commands, and a deleted file's path still names it.

**What the consumer provides.** The path, its `ChangeKind`, whether it is staged, and the tree depth.

**Don't** reuse these inks for diff lines (`added`/`removed` are different tokens), or bold a name to mark it.
