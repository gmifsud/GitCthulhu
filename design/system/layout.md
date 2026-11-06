# Layout

The window is a column: header (two chrome rows and a hairline), then a row of the Repositories column beside the tab strip, banners, workspace and terminal, then the status bar. The root is always one stack of that column plus an overlay layer, so scroll offsets and text cursors survive a dialog opening.

## The workspace

Four panes in a pane grid: **Files** (staging and explorer), **Graph** (history), **Diff**, **Inspector**. Four presets, cycled with Ctrl+Shift+L from the top bar:

| Preset | Arrangement |
|---|---|
| `Log` (default) | Files \| (Graph \| Inspector) over a full-width Diff, as in the JetBrains Git log |
| `Columns` | Files \| Graph \| Diff \| Inspector |
| `InspectorOverDiff` | Files \| (Graph / Diff) \| Inspector |
| `Stacked` | Files / Graph / Diff / Inspector |

## Ratios and bounds

Three splitters, each a ratio of its parent:

- `files` — the staging column against everything else: default 0.22, dragged within 0.16 to 0.36. Before a drag the Explorer is pinned at `explorer-width` (280 px) and the ratio is derived from the window width inside those bounds.
- `graph` — history (or history + diff) against the Inspector: default 0.68, within 0.50 to 0.82.
- `middle` — Graph against Diff: `Log` 0.64, `Columns` 0.50, the stacked presets 0.62; dragged within 0.25 to 0.75.

The Diff has three hunk modes. `Beside` is the default; `Wide` clamps `graph` to at least 0.78 and `middle` to at most 0.36 so the diff gets the room; `Focus` is explicit only.

## Fixed chrome

- Header: `title-row` 36 over `branch-row` 34, both `bg-chrome`, `bar-inset` 12 horizontal, clusters `cluster-gap` apart, commands `control-gap` apart. A 1 × 16 px `border` separator sits between the mark and the repository name.
- Repositories: `repositories-column` 236 pinned, or a 40 px rail with 24 px initial tiles; rows are `repository-row` 40.
- Tab strip: content-sized tabs on the left, `pane-gap` apart, in a full-width `bg-chrome` bar padded `6px 8px`, a hairline under it.
- Terminal: `terminal-height` 240 by default, dragged between 110 and 900 by a 4 px grip.
- Status bar: 24, or 72 with an error open; `bar-inset` horizontal, items `tree-indent` 14 apart.
- Floating: the branch popup is 420 wide, 44 px from the top, list 460 tall; the command palette 640 wide, 56 px from the top, list 380 tall, rows 22; both carry the `popup` shadow. Dialogs are a 460 card centred on the scrim.

## History row

`graph-row` 24 tall: the graph column (24 to 160 px, lanes `lane-pitch` 16 apart), hash 64, subject with the ref-pill strip (pills 16 tall, 3 apart, strip clipped at 180), when 112, author 120; columns `column-gap` 10 apart under an 11 px, 20 px-tall header.
