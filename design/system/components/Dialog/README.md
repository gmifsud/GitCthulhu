# Dialog

A modal card centred on a scrim, for confirmations and short forms: discard, detach, drop stash, new branch, merge, stash, worktree, settings. Hand-written from `gui_app/src/view/confirm.rs` (`modal`, `heading`, `note`, `stash_dialog`, `merge_dialog`).

**Anatomy.** The scrim is `scrim-dialog` (`bg-app` at 72 %) over the whole window and swallows clicks. The card is `dialog-width` 460, `dialog-inset` 20 padding, rows `bar-inset` 12 apart, `bg-sidebar` fill, `hairline` `border` edge, `radius-lg`. No shadow.

**Rows**, top to bottom: a `heading` (body + 2, `text-primary`); the fields (TextField, pick list, checkboxes with `body` labels); a `note` in `text-secondary` saying what will happen; then the actions, `cluster-gap` apart, the accent Button first and `Cancel` as a command Button.

**Copy.** Heading names the act and its object: `Stash changes`, `Merge into main`, `Discard 3 files`. The note is one sentence of consequence: `Every tracked change is stashed and the working tree is reset to HEAD.`

**What the consumer provides.** The heading, the fields and their messages, the note, and the two actions; the dialog is one column, so order the rows yourself.

**Don't** add a title bar, an icon, a close cross or a third button; the modal closes by its own actions only.
