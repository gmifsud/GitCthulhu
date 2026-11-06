# TextField

A single-line input, and the pick list that shares its dressing. Hand-written from `gui_app/src/view/chrome.rs` (`field_style`, `embedded_field_style`) and `gui_app/src/view/confirm.rs` (`pick_list`).

**Anatomy.** `body` text in `text-primary`, placeholder in `text-muted`, padding 6 px (`6px 8px` for a pick list), `bg-app` well, `chip-border` edge at `hairline`, `radius-sm`. Selection is `accent-primary`.

**States.** Focused: the border goes `accent-primary`. Nothing else changes: no glow, no shadow, no label movement. Embedded (inside a shell that already draws the border, such as the filter bar's search row): no fill, no border.

**Pick list.** Same well and edge; placeholder `text-muted`, value `text-primary`, handle (chevron) `text-secondary`. Its menu is a list of `body` rows.

**Search.** In the filter bar, a `search` glyph in `text-muted` leads an embedded field, at most `search-max` 320 wide, with a 130 px pick list beside it, parts 6 px apart.

**What the consumer provides.** Placeholder, value, the on-input and on-submit messages; for a pick list, the options and the chosen one.

**Don't** put a label inside the border, use a 2 px focus ring, or tint the well.
