# Tooltip

The hover tip under an icon button or tab. Hand-written from `gui_app/src/view/chrome.rs` (`icon_button`) and `gui_app/src/view/tabs.rs`.

**Anatomy.** `caption` text (body − 1) in `text-primary`, padding `4px 8px`, `bg-app` fill, `hairline` `border` edge, `radius-sm`, positioned below the control with a `pane-gap` 4 gap. A tab's tip is `mono-caption`: the full path on one line, the branch on the next. A ref-pill strip's tip lists every ref, one per line.

**Timing.** An icon tip waits two seconds. The hover target is never remounted when the tip becomes ready (a zero-size tip stands in before), so hover state does not flicker.

**What the consumer provides.** The title text and a stable key per control.

**Don't** put actions in a tip, use an arrow, or shadow it.
