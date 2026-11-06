# Button

One bordered button in four dressings: command, accent, quiet and icon. Hand-written from `gui_app/src/view/chrome.rs` (`bordered_button`, `command_button`, `accent_button`, `labelled_button`, `quiet_button`, `icon_button`).

**Anatomy.** `body` text (or a `glyph`), `hairline` border, `radius-sm`. Rest is `bg-app` fill, `border` edge, `text-primary` ink. Hover and pressed fill with `border`. Disabled keeps the rest fill and turns the ink `text-muted`. No shadow, no transition.

- **Command** — `2px 6px`. Any secondary action: `Cancel`, `Continue`, `Abort`.
- **Accent** — `2px 8px`, `accent-primary` fill and edge, `on-accent` (black) ink. The one primary action in a dialog or bar: `Merge`, `Stash`, `Commit`. One per group; disabled it falls back to the command dressing.
- **Labelled** — a `glyph-inline` icon then a label, 5 px apart, `2px 6px`; accent variant as above with the glyph black too.
- **Quiet** — no fill, no border; `text-secondary` ink that goes `text-primary` on hover. App-level commands in the header: `Open…`, `Recent`, `Settings`.
- **Icon** — a `glyph` (body + 1) at `3px 5px`, `text-secondary` ink (`text-primary` when it is the accented command). Always wrapped in a tooltip that appears after two seconds (see Tooltip).

**What the consumer provides.** The label or icon, the message to send, `enabled`, and whether it is the accent one.

**Don't** stack two accent buttons, put the accent button last (it goes first, then `Cancel`), or add a drop shadow.
