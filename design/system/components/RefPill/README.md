# RefPill

A branch or tag label beside the commit that is its tip, in the History row. Hand-written from `gui_app/src/view/history.rs` (`ref_pill`) and `gui_app/src/ref_pills.rs`.

**Anatomy.** `pill-height` 16, padding `0 5px`, `radius-xs`, `hairline` border, a kind mark then the name, then ahead/behind counts when the branch tracks an upstream (`↑` in `outgoing`, `↓` in `incoming`). Names longer than 48 characters are ellipsised; the full name is in the hover text, one ref per line with its worktree folder.

**Kinds**, in the order they are drawn (head, worktree, local, remote, tag):

| Kind | Mark | Ink | Fill | Border |
|---|---|---|---|---|
| Head (current checkout) | `branch` | `bg-app` | `accent-primary` | `accent-primary` |
| Worktree (checked out elsewhere) | `worktree` | `accent-primary` | `bg-raised` | `accent-primary` |
| Local | `branch` | `text-secondary` | `chip` | `chip-border` |
| Remote | `cloud` | `text-muted` | `bg-raised` | `border` |
| Tag | `tag` | `added` | `chip` | `added` |

The strip of pills clips at 180 px; a `+N` pill stands in for the rest, and the hover text still names every ref.

**What the consumer provides.** The `RefPill` (label, full name, kind, ahead, behind) and the worktree list for the hover text.

**Don't** colour a kind any other way (head is the only filled pill), or let a pill wrap or grow taller than its row.
