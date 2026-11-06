GitCthulhu is a local Git client built in Rust on iced. It spends its life on history and staging, so the shell is deep charcoal and one ember accent does all the pointing: the checked-out branch, the primary command, the thing that is running. Nothing is decorated; everything is bordered.

## Content fundamentals

- Write in British English: favour, colour, analyse, behaviour. Sentence case everywhere: `Open…`, `Recent`, `Settings`, `Stash changes`, `No upstream`.
- Labels are nouns or imperatives, never chatty. Real copy: `Message (optional)`, `Include untracked files`, `Always create a merge commit (--no-ff)`, `Every worktree is already open.`
- A note under a control says what will happen, in one plain sentence: `If files conflict, the merge stays open: resolve them in the working tree, then Continue or Abort from the bar at the top.`
- Use the real characters: an ellipsis `…` on a command that opens something (`Open…`), a middle dot `·` to join facts (`fetch · done in 3 s`, `main (current branch) · worktree repo`), `↓` and `↑` for incoming and outgoing commit counts.
- Hashes, paths and ref names in a sentence stay in `mono`. Git's own words stay Git's: `--no-ff`, `HEAD`, `Detached HEAD`, `origin/main`.
- No emoji, no exclamation marks, no "Oops". An error is its first non-empty line in `accent-primary`, with a `details` fold beside it.

## Colour

One theme, `Deep Focus`. Four grounds step up from the window: `bg-app` (panes), `bg-sidebar` (ruled panels, the dialog card), `bg-chrome` (header, tab strip, status bar), `bg-raised` (the active tab, a hovered row). Keep the steps: chrome is one shade lighter than the panes it frames, and a raised thing is one shade lighter than its chrome.

- Text has three weights of grey, by job: `text-primary` for headings, the active tab, field values and button labels; `text-secondary` for everything a pane says by default; `text-muted` for pane titles, column headers, paths, placeholders and disabled labels. All three read on every ground (`text-muted` is 5.5:1 on `bg-app`).
- `accent-primary` is the ember, and it means *current* or *go*: the branch you are on (the chip's ink, the Head pill's fill, the active tab's branch line), a focused field's border, the one primary button in a dialog, a running job in the status bar. Never use it as decoration or on more than one control in a group.
- Text on an ember fill is `on-accent` (black), never white.
- `chip` and `chip-border` are the resting fill and edge of every chip-like control: the branch chip, inactive tabs, Local and Tag pills, the tab-picker button. Hover lifts the fill to `bg-raised`; the border goes to `accent-primary` only when the control is open or current.
- Diff inks are `added` and `removed` on their own solid washes `added-bg` and `removed-bg`. The washes are opaque: never a translucent overlay on `bg-app`. Word-level changes get the stronger `added-word-bg`, `removed-word-bg` and, in a side-by-side modified pair, `modified-bg` with `modified-word-bg` behind the words.
- File names in Changes and Explorer take the `vcs-*` inks after the JetBrains changes view: `vcs-modified` blue, `vcs-added` green, `vcs-deleted` grey, `vcs-untracked` brick, `vcs-renamed` teal, `vcs-conflicted` red. A file's status is also its icon and its section, never colour alone.
- Commit counts are not diffs: `incoming` (blue, `↓`) and `outgoing` (violet, `↑`) deliberately avoid green and red.
- The commit graph cycles `lane-0` to `lane-5` by lane index; lane 0 is the ember so the tip lane and the brand agree.
- Modals sit on `scrim-dialog`; the command palette on `scrim-palette`; the branch popup on `scrim-popup`. The only shadow in the app is `popup`, under those two floating lists. Panels, cards and buttons are bordered with `border` at `hairline`, never shadowed.

## Type

- The reading size is a setting (12 to 18 px, default 14) and every style is an offset from it: `body` is the size itself; `heading` is +2; `wordmark` and `glyph` are +1; `caption` is −1 (floor 10); `compact` is −2 (floor 11). Build with the offsets, not the pixel values, so the Appearance setting still works.
- The face is also a setting, one of three: `Segoe UI` (default), `Familjen Grotesk`, `Fragment Mono`. `sans` is whichever is chosen; `mono` is the system monospace unless `Fragment Mono` is the UI face, in which case everything is mono.
- Hashes, paths, ref names in the popup and diff text are always `mono`, whatever the UI face.
- Line height is 1.3 everywhere (iced's default). A cell that must stay on one line is `ceil(size × 1.3) + 1` tall and clips; it never wraps.
- iced has no letter spacing: the `wordmark-caps` mark spaces its capitals with a thin space (U+2009) between letters.
- Icons are text in the `icon` family (`codicon`) at line height 1, coloured with the same tokens as the text beside them.

## Spacing and layout

- Gaps are small and named by where they sit: `pane-gap` 4, `control-gap` 6, `cluster-gap` 8, `column-gap` 10, `bar-inset` 12, `tree-indent` 14, `dialog-inset` 20. Use the one for the place, not the nearest number.
- Buttons are tight: `2px 6px` for a command, `2px 8px` for the accent button, `3px 5px` for an icon button, `3px 10px` for the branch chip, `5px 8px` for a tab.
- Rows have fixed heights so lists scroll in whole rows: `graph-row` 24, `diff-row` 20, `palette-row` 22, `repository-row` 40. The header is `title-row` 36 over `branch-row` 34 with a hairline under them; the status bar is 24, or 72 with an error open.
- Tool-window widths are pixels, not ratios: `explorer-width` 280, `repositories-column` 236, folded to a 40 px rail. Narrowing the window narrows the log, never the tree. See Layout for the presets and the drag bounds.
- Dialogs are one `dialog-width` card (460) centred on the scrim, rows `bar-inset` apart, `dialog-inset` padding, buttons `cluster-gap` apart with the accent one first.

## Borders, radii, states

- Every border is `hairline` in `border` or `chip-border`. Radii are small and by kind: `radius-xs` 3 for ref pills, `radius-sm` 4 for buttons, fields and tooltips, `radius-md` 5 for tabs and the badge, `radius-lg` 6 for the dialog card, `radius-pill` for the branch chip.
- Hover on a bordered button fills it with `border`; on a chip, tab or row it fills with `bg-raised`. Pressed looks like hover. There is no transition and no motion anywhere.
- Focus on a text field is its border going `accent-primary`; the selection colour is `accent-primary` too.
- Disabled is `text-muted` ink with the resting fill and border; nothing fades.
- Current or open (the active tab, an open picker, the branch chip while its popup is open) is an `accent-primary` border on the resting or raised fill.
- The status dot is `added` when idle and `accent-primary` while a job runs; a finished job shows `check` in `added`, `close` in `removed`, or `stop` in `text-muted` for a few seconds.

## Iconography

Every icon is a glyph from the bundled Codicons font (`fonts/codicon.ttf`, Microsoft, CC BY 4.0): 44 of them, listed in Icons with their codepoints. Draw them with `glyph` styles in the `icon` family, never as images, and never from a system font (Windows lacked several Unicode symbols and drew boxes). Ref kinds have fixed marks: `branch` for a local branch or HEAD, `worktree` for a branch checked out elsewhere, `cloud` for a remote, `tag` for a tag. A fold is `chevron-right` closed and `chevron-down` open.

## Marks

The app icon is in `assets/Logos`: an ember circuit-tentacled Cthulhu on a charcoal rounded square with the name set under it. Use it at the window and taskbar only; the header carries a typeset mark instead, in one of five styles the user chooses under Appearance (see Wordmark): a two-tone `Git`/`Cthulhu`, a 20 px ember badge with a heavy `G` beside the name, spaced capitals, a shell prompt `> gitcthulhu`, or the badge alone. The name is always written `GitCthulhu`, one word, capital G and C.

## Not synced

From `main@17dccb6`. Not carried over: the three HTML wireframes under `docs/design` define a slightly different scale (`--text #d1d1d1`, `--muted #5a5a5a`, `--orange-ink #1a1208`, translucent diff washes) that the Rust code later replaced; the code is the source here. `Familjen Grotesk` and `Fragment Mono` are named faces with no files in the repository. Components are hand-written static renditions of the iced widgets (no bundle), each README naming its source file.
