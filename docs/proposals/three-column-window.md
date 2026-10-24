# Proposal: the three-column window

Status: Draft, for discussion
Wireframe: `docs/design/three-column-wireframe.html` (open in a browser; it is self-contained)
Supersedes the layout in `docs/design/graph-dock-wireframe.html`

## What changes, in one paragraph

Today the window is a header, a context ribbon, and a two-pane split: graph on the left, a "detail" that is sometimes the changes list and sometimes a patch on the right. The target is the GitLens / VS Code arrangement: a thin top bar, then four panes — **Graph**, **Inspector**, **Diff**, **Files** — and a status bar. The panes have three **layout presets**, switched from the top bar: *Columns* (all four side by side), *Inspector over Diff* (the middle two stack, so the diff takes the full middle width — the default on a wide screen), and *Stacked* (everything top to bottom). The graph grows ref pills, a date column, and a working-changes row. The inspector is one surface with three modes (Working tree, Commit, Compare). The diff is stacked per file, with a side-by-side option, line numbers, folded context, and word-level highlights. The files rail lists what the diff is showing and narrows it to one file on click. Nothing about the port boundary (ADR 0001) or the reducer (ADR 0002) changes; some new port methods are added.

## Where the pieces are today

| Target surface | Today | Files |
|---|---|---|
| Top bar | `header()` (title, branch, tracking, layout toggles, Open…, path, error) + `ribbon()` (context label, commands, Settings) | `gui_app/src/view/header.rs`, `view/ribbon.rs` |
| Graph column | `branch_switcher()` stacked above `history_pane()` in the Graph pane | `view/workspace.rs::graph`, `view/refs.rs`, `view/history.rs`, `commit_graph.rs` |
| Inspector column | `detail()` picks `changes_pane()` or a commit header + `patch_pane()` based on `Selection` | `view/workspace.rs::detail`, `view/changes.rs` |
| Diff column | `patch_pane()` unified only, no line numbers, hunk buttons after every hunk | `view/patch.rs` |
| Files rail | does not exist | — |
| Status bar | does not exist; error and path live in the header | `view/header.rs` |
| Layout presets | `pane_grid` with two panes and a side-by-side / stacked toggle (`SetLayout(bool)`) — the same idea, generalised to four panes and three presets | `layout.rs`, `app.rs::{panes, side_by_side, split_ratio}`, `view/header.rs::layout_button` |
| Theme | 7 tokens; diff colours are constants in `patch.rs`; lane colours in `commit_graph.rs` | `theme.rs`, `patch.rs`, `commit_graph.rs` |

## What the port cannot give us yet

These are the only reasons a phase might be blocked. Each one is a method on `GitRepository`, an implementation in `git_adapter`, a field on `test_support::FakeGitRepository`, and (where the window stores it) a `ViewEvent` + `AppState` field.

| Need | Why | Port change | Phase |
|---|---|---|---|
| Which commit each branch and tag points at | Ref pills on graph rows | `Branch.tip: String` (or `branches()` returns it). `Tag.commit_id` already exists. | 2 |
| Per-file `+n −m` and A/M/D/R status for a commit | Inspector Commit mode, Files rail, diff file headers | `commit_files(id) -> Vec<FileChange>` (`git show --numstat --name-status --format=`). Can be *derived* from `commit_patch` hunks as a first step without a port change. | 3 |
| Same for the working tree | Inspector Working tree mode stats | `status()` extended, or `worktree_files() -> Vec<FileChange>` (`git diff --numstat`, `git diff --cached --numstat`) | 3 |
| Commits and files between two refs | Inspector Compare mode | `compare(base, head) -> Comparison { commits, files, ahead, behind }` (`git log base..head`, `git diff --numstat base...head`, `git rev-list --left-right --count`) | 5 |
| Patch for a range | Diff column in Compare mode | `range_patch(base, head) -> Vec<Patch>` | 5 |
| Author e-mail / avatar | Avatar circle in graph rows | Not planned. Draw an initial in a circle from `Commit.author`. | — |

`Commit.timestamp` already exists, so the date column needs no port change.

## Decisions to take before Phase 1

1. **Layout is a preset, not a bool.** Today's Side-by-side / Stacked toggle becomes `LayoutPreset { Columns, InspectorOverDiff, Stacked }`, switched by a three-button segment in the top bar (wireframe: ▯▯▯ / ▯⊟▯ / ⊟). Each preset is a different `pane_grid::Configuration` tree over the same four panes:

   ```text
   Columns           Graph | Inspector | Diff | Files
   InspectorOverDiff Graph | ( Inspector / Diff ) | Files
   Stacked           Graph / Inspector / Diff / Files
   ```

   Splitter ratios are remembered **per preset**, so switching back restores what you had. The Files pane still collapses to a rail in every preset (a 36 px column, or a one-line strip when stacked). In the two stacked presets the Inspector is short and wide, so its blocks lay out left to right (message box · commands · file list) instead of top to bottom — the wireframe shows this. Default preset on first run: `InspectorOverDiff`. Later: a keyboard shortcut to cycle presets.
2. **The ribbon dissolves into column heads.** `ribbon_commands()` in `selection.rs` stays as the single source of "which commands does this selection allow" (it is tested). The view changes: branch commands render in the Graph head, file/commit/stash commands render in the Inspector head or inside its blocks, hunk commands stay in the Diff. Fetch / Pull / Push / Stash are always-available and move to the top bar. `context_label()` becomes the Inspector tab title.
3. **Confirmations stay full-window** for now (`confirm.rs`, `branch.rs`). iced 0.13 has `stack` so a modal overlay is possible later; it is not on this path.
4. **Collapse to rails.** `CONTEXT.md` already promises refs and changes can collapse to a rail. In the new layout only the **Files** column collapses (to 36 px). Graph, Inspector and Diff always show. If you want Graph or Inspector collapsible too, say so; it is cheap but it is a decision.
5. **Theme identity.** Keep the orange `deep_focus` palette rather than copying VS Code blue. The wireframe does this.

## Phases

Each phase compiles, passes `cargo test`, and is usable on its own. Order matters: 1 before anything, 2–4 can be parallel after 1, 5 last.

### Phase 1 — The shell (no new data)

Goal: the four columns, the top bar, and the status bar exist. Everything that was visible before is still visible somewhere.

- `layout.rs`: replace `WorkspacePane::{Graph, Detail}` with `{Graph, Inspector, Diff, Files}`. Add `LayoutPreset { Columns, InspectorOverDiff, Stacked }` and `LayoutRatios { graph, middle, files }` (three `f32`s; `middle` is the Inspector/Diff split in whichever axis the preset uses). `new_panes(preset, &ratios) -> State<WorkspacePane>` builds the matching `Configuration` tree — this is the same shape as today's `new_panes(side_by_side, ratio)`, with one more level. Defaults from the wireframe: graph 0.34, middle 0.30 (columns) / 0.28 (stacked), files 0.86. Replace `axis()` with `preset_of(&State) -> LayoutPreset` if a test wants it, or drop it.
- `app.rs`: replace `side_by_side: bool` + `split_ratio: f32` with `layout: LayoutPreset` + `ratios: [LayoutRatios; 3]` (one per preset). `Message::SetLayout(bool)` becomes `Message::SetLayout(LayoutPreset)`: store the preset, rebuild `panes` from that preset's ratios. `PaneResized` writes into the current preset's `LayoutRatios` — iced reports the split handle, so keep a small map from `pane_grid::Split` to which of the three ratios it is, rebuilt whenever `new_panes` runs. Add `files_rail_open: bool` and `Message::ToggleFilesRail` (in the stacked presets this hides the strip rather than narrowing a column).
- `view/mod.rs`: `column![top_bar, workspace, status_bar]`. Delete the two `hairline` calls or keep one under the top bar.
- `view/header.rs` → rename to `view/top_bar.rs`: one 40 px row. Left: mark, path (ellipsised, mono), branch chip (opens the branch list — see Phase 2), tracking summary. Centre: a `text_input` search field (wired in Phase 2; render disabled now). Right: Fetch, Pull, Push, Stash, the three-button layout segment (today's `layout_button()` generalised to `LayoutPreset`), Open…, Settings. Error text moves to the status bar.
- `view/ribbon.rs`: delete. Its `command_message()` and `stage_message()` move to a new `view/commands.rs` so the column heads can call them.
- `view/workspace.rs`: `graph()` keeps `branch_switcher` + `history_pane` for now. `inspector()` returns the old `detail()` body. `diff()` returns `patch_pane()`. `files_rail()` returns a placeholder list of `app.patches` paths (this already exists in memory); when `layout == Stacked` it renders as a horizontal `row` of chips instead of a `column`. Every pane body takes `layout` so Phase 3's inspector can choose row-wise blocks in the stacked presets.
- New `view/status_bar.rs`: 24 px. Left: loading dot (`state.loading`), `app.error` in accent colour. Right: commit count, `tracking.upstream`.
- `theme.rs`: add `bg_chrome`, `bg_raised`, `chip`, `chip_border`, `added`, `removed`, `added_bg`, `removed_bg`. Move `ADDED`/`REMOVED` out of `patch.rs`. Leave lane colours in `commit_graph.rs`.
- Tests: replace `the_detail_sits_beside_the_graph_until_it_is_stacked` with one per preset asserting the `Configuration` shape (outer axis, nested axis, pane order); one that `PaneResized` on a middle split changes only that preset's `middle` ratio and leaves the other presets' ratios alone; one for `ToggleFilesRail`.

Visible result: same information, GitLens-shaped frame, and the three presets already switch.

### Phase 2 — The graph column

Goal: the left column reads like the target's graph.

- **Row columns.** `history.rs`: `[lanes][pills + message][author][when][hash]`. `when` is a relative age from `Commit.timestamp` (new pure fn `relative_age(now, ts) -> String`, tested). Author gets a 16 px initial-in-a-circle drawn with `canvas` or a `container` with radius 8. Row stays `ROW_HEIGHT = 28`.
- **Ref pills.** Needs `Branch.tip` (port change above). New pure fn `refs_at(commit_id, &branches, &tags, &stashes) -> Vec<RefPill>`; render as small bordered `container`s before the message. Current branch pill uses `accent_primary`; remotes use `text_muted`; tags use `added`. Test the pure function with the fake.
- **Working-changes row.** When `current_status` has any files, push a first row with a dashed dot (new `GraphCell` variant or a flag on `GraphRow`), italic "Working changes", and a `wip` pill with the branch name. Clicking it sends `Message::ShowWorkingTree`. This replaces the "Working tree" button that `detail()` used to show.
- **Branch chip.** `branch_switcher()` moves out of the graph body: the chip lives in the top bar; the search + list opens as a dropdown panel at the top of the Graph column (same `branch_list_open` / `branch_query` state, same `ChooseBranch` message). Remote, tag and stash sections stay in that list.
- **Graph head.** `[All branches | Current only]` toggles (new `graph_scope` state; filter `dag_nodes` client-side by reachability — or defer and render the toggles disabled) and the branch commands from `ribbon_commands()` for a `Selection::Branch`.
- **Search.** Wire the top-bar input to `graph_query: String`. Rows whose `message`, `id`, or `author` do not match are dimmed (not removed, so the lanes stay continuous). Pure fn `matches_commit(&Commit, &str) -> bool`, tested.
- Lanes: `commit_graph.rs` already produces continuous lanes. Optional: replace the straight incoming/outgoing segments with a cubic curve when the lane changes, as the wireframe does. Pure geometry, test the path endpoints.

### Phase 3 — The inspector column

Goal: one surface, three modes, with per-file stats.

- `app.rs`: add `inspector_mode: InspectorMode { WorkingTree, Commit, Compare }`. It is **derived from `Selection` by default** (File/Branch → WorkingTree, Commit/Tag → Commit) and only overridden when the user clicks a tab; store `Option<InspectorMode>` and resolve with a pure fn. Test that selecting a commit switches to Commit and clicking the WIP row switches back.
- `view/inspector.rs` (new, absorbs `changes.rs`):
  - Head: three tabs + List/Tree toggles (existing `files_as_tree`).
  - Body direction follows the preset: a `column` of blocks in *Columns*, a `row` of blocks (message · commands · file list) in *Inspector over Diff* and *Stacked*, since the pane is then short and wide. One `fn inspector_blocks() -> Vec<Element>` feeds either container, so the two directions cannot drift apart.
  - **Working tree**: commit message as a multi-line `text_editor` (iced 0.13) instead of `text_input`; Commit (primary, gated by `can_commit`), Suggest, Amend (disabled until a port method exists). Then the Changes block: Select all / Clear / Stage / Unstage / Discard and the "n selected" count. Then the grouped file list with mark boxes — this is `change_rows()` + `change_line()` today, gaining `+n −m` and a status letter.
  - **Commit**: title, meta row (hash, author, age, parent), commands from `ribbon_commands()` for `Selection::Commit`, then "Files changed (n)  +a −d" and the file list without mark boxes.
  - **Compare**: rendered disabled with a note until Phase 5.
- **Stats without a port change (first step):** pure fn `patch_stats(&Patch) -> (added, removed)` counting `DiffLineKind`. This gives correct numbers for whatever is already loaded into `app.patches`. Replace with `commit_files()` / `worktree_files()` when the port grows them so the inspector does not need every patch loaded to show counts.
- Clicking a file in either mode sets `selected_file: Option<String>`, which the Diff column (Phase 4) uses to narrow itself. Clearing it ("All files" in the rail) shows every file again.

### Phase 4 — The diff column and the files rail

Goal: stacked per-file diffs, side-by-side option, line numbers, folds, hunk actions where they belong.

- `view/diff.rs` (replaces `patch.rs`):
  - Head: title (`Working tree · 4 files` / `11f6db4 · 2 files`), Unified / Side by side toggle (`diff_split: bool`, persisted on `AppSettings` later), Wrap toggle.
  - Per-file block: sticky-looking header row (path, `+n −m`, Open file, Stage/Unstage file in Working tree mode), then hunks. "⋯ n hidden lines ⋯" bars come from the gap between consecutive hunk headers; the first bar from `old_start - 1`.
  - Hunk header parsing: new pure fn `parse_hunk_header("@@ -a,b +c,d @@") -> Option<HunkRange>`. `git_adapter/src/diff.rs::parse_patches` keeps the header as a raw string and does not parse the ranges, so this is new. Put it in `core_domain` beside `Hunk` (so a later adapter can fill `HunkRange` directly) and handle the short forms `@@ -8 +9 @@` and `@@ -1 +0,0 @@`, which the adapter's own tests already produce. Line numbers are counted from there. Tested.
  - **Unified**: `[old no][new no][sign][text]` in mono, with `added_bg` / `removed_bg` row tints.
  - **Side by side**: pure fn `pair_lines(&Hunk) -> Vec<PairedLine { left: Option<DiffLine>, right: Option<DiffLine> }>` that pairs each run of Removed with the following run of Added and pads the shorter side with blanks. Tested on: pure addition, pure removal, equal-length replace, unequal replace. Rendered as two equal columns in a `row`, each a `column` of lines, so they scroll together.
  - **Word highlights**: pure fn `word_diff(left, right) -> (Vec<Span>, Vec<Span>)` using a token LCS; rendered with `rich_text`/`span` (iced 0.13). Can land after the rest of Phase 4.
  - Stage hunk / Discard hunk buttons stay, but only in Working tree mode and once per hunk, below the hunk, as today.
  - Syntax highlighting: iced's `highlighter` feature (syntect) exists but is built for `text_editor`; do not plan on it for this phase.
- `view/files_rail.rs`: "Files · n", collapse button, "All files", then one row per file: status dot (M orange / A green / D red), file name, `+n −m`. Click sets `selected_file`. Collapsed state is a 36 px rail with only the button. Scrolling the Diff to a file instead of filtering is possible with `scrollable::scroll_to` + `Id`, but needs per-block heights; filtering is the first step.

### Phase 5 — Compare

Goal: the middle column can compare two refs, like "Comparing References" in the target.

- Port: `compare(base, head)` and `range_patch(base, head)` as above. Add to `FakeGitRepository`.
- `app.rs`: `compare: Option<CompareRefs { base, head }>`, `Message::{SetCompareBase, SetCompareHead, SwapCompare, CompareLoaded}`. Load in the background like `load_patch_in_background`, with the same generation guard against stale results.
- Inspector Compare mode: two `pick_list`s of local + remote branches, "head is n ahead, m behind base", Merge / Rebase buttons that reuse `AskMerge` / `AskRebase` when `head` is the current branch, a Commits list (click → `Select(Commit)`), and the Files changed list.
- Diff column shows `range_patch`; Files rail follows.

### Phase 6 — Persistence and polish

- `AppSettings`: `layout` (preset), `ratios` (per preset), `files_rail_open`, `diff_split`, `files_as_tree`. Saved through the existing `SettingsSaver`, so the window reopens the way it was left.
- Keyboard: ↑/↓ in the graph moves selection; `Ctrl+K` focuses search; `Esc` closes the branch list; `Ctrl+Shift+L` cycles the layout preset. iced `keyboard::on_key_press` subscription.
- Hover tooltips on pills (full ref name) and hashes (full id).
- Remove dead code: `ribbon.rs`, `layout::axis` if unused, the old `Side by side` / `Stacked` strings and `SetLayout(bool)`.
- `CONTEXT.md`: rewrite the "What a session can do" paragraph for columns, and the "Later: the changes list" section (most of it ships in Phase 3).

## What does not change

- `gui_app` still depends on `core_domain` and `iced` only.
- `AppState::apply_event` is still a reducer; new data arrives as `ViewEvent`s.
- Every Git operation still runs on a background thread via `Task::perform`.
- `selection.rs` still decides what a selection allows; the view only lays it out.
- Tests still use `FakeGitRepository`; no test opens a repository on disk.

## Risks

- **iced text performance.** A 300-line side-by-side diff is ~1200 `text` widgets. iced 0.13 handles this, but a 5 000-line patch will not be smooth. Mitigation: cap rendered lines per file (e.g. 400) with a "Show all" button, from Phase 4.
- **`pane_grid` with three splits and three presets.** Resizing one splitter changes the ratio of its own split only; the outer ratios are unaffected, which is what we want. The `Split` handles are new every time `new_panes` runs, so the handle → ratio-slot map must be rebuilt on every `SetLayout`; a stale map would write a resize into the wrong slot. Test it.
- **A ratio that is right in one axis is wrong in the other.** The Inspector/Diff split is a width in *Columns* and a height in the stacked presets, which is why ratios are stored per preset rather than shared.
- **Ref pills need `Branch.tip`.** Until the port change lands, Phase 2 can ship pills for tags only (`Tag.commit_id` exists) and the WIP row.
- **Relative age needs a clock.** `relative_age(now, ts)` takes `now` as a parameter so tests are deterministic; the view passes `SystemTime::now()`.

## How to review each phase

Verdict → Location → Issue → Impact → Remediation → Verification, as in `CLAUDE.md`. Verification for every phase: `cargo test -p gui_app`, then open `D:\Repos\GitCthulhu` in the binary and compare against the matching wireframe state (Working tree / Commit / Compare tabs) in each of the three layout presets.
