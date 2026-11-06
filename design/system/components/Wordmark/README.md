# Wordmark

The app name at the left of the header, in one of five styles the user picks under Appearance. Hand-written from `gui_app/src/view/wordmark.rs`.

**When to use.** Only in the header's title row, once. Everywhere else the name is plain text: `GitCthulhu`, one word.

**The five marks** (`HeaderMark`, default `TwoTone`):

- `TwoTone` — `Git` in `text-secondary` and `Cthulhu` in `accent-primary`, both `wordmark` (body + 1, bold).
- `Badge` — a 20 px `accent-primary` tile, `radius-md`, with a `badge-letter` (weight 900) `G` in `on-accent`, then `GitCthulhu` in `text-primary` bold, `cluster-gap` apart.
- `SmallCaps` — `GITCTHULHU` in `wordmark-caps` (body − 1, semibold, `text-muted`), a thin space U+2009 between letters because iced has no letter spacing.
- `Prompt` — a `chevron-right` glyph in `accent-primary` then `gitcthulhu` in `mono` at body size, `text-primary`, 4 px apart.
- `BadgeOnly` — the tile alone.

**What the consumer provides.** The chosen `HeaderMark`, the tokens and the reading (face and size). Nothing else is configurable: the words, case and colours are fixed.

**Don't** set the name in the icon artwork's lettering, recolour `Cthulhu` to anything but the ember, or use white on the badge (it is black, `on-accent`).
