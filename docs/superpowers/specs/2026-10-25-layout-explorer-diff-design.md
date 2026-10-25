# Layout, explorer, graph density, and fused files pane — design

Status: **Approved for planning** (2026-10-25)  
Related: ADR 0001, ADR 0002, ADR 0003, `docs/superpowers/specs/2026-10-25-worktree-tabs-design.md`, `docs/proposals/three-column-window.md`  
Reference: GitKraken left accordion + graph pills-on-node + dense diff; current screenshots show deformed graph headers and a wasted right inspector next to a left Changes tree.

## Problem

The four-pane shell deforms under real repos: history column headers wrap, branch/worktree pills float in the Description column instead of sitting on the graph node, the left Files rail mixes staging with refs/worktrees, and the right Inspector often only repeats “Files changed” while Diff is starved. Diff is unified-only and visually weak compared to the GitKraken hunk look the product is aiming at.

## Goals

- Stop layout deformation: fixed/min column widths, no vertical-wrapped headers, denser rows.
- Left rail: collapsible **LOCAL / REMOTE / WORKTREES / STASHES** (counts on headers). Staging file lists leave the left rail.
- **Fuse** left Changes + right Files-changed into one **right-pane files navigator**.
- Right pane shows **stacked** sections when both apply: **WIP on top**, **selected commit below**.
- Graph row order: **pills → graph → message → author → when → hash** (hash last).
- Hunk placement modes: beside the files pane, take remaining centre width, or full-screen (Esc restores).
- Improve hunk visuals toward the GitKraken reference (dense tints, gutters); split/side-by-side is a follow-up unless capacity allows in the same plan’s last slice.
- Preserve ADR 0001/0002; session ownership of repo state stays as ADR 0003.

## Non-goals (this cut)

- True side-by-side split diff with word-level highlights (ADR 0003 WP3) — may be sketched; not required to ship this redesign.
- Compare References mode (ADR 0003 WP2).
- Persisting layout ratios / hunk mode / section collapse to disk (optional follow-up; runtime state is enough for this cut).
- Redesigning the terminal pane or tab strip chrome (tabs already session-based).
- Changing worktree create/remove semantics.

## Decisions (locked with product)

| # | Decision |
|---|---|
| D1 | Right pane stays; it is the **primary files navigator**, not optional chrome. |
| D2 | Left Changes file tree and right “Files changed” **fuse into one** surface on the right. |
| D3 | When WIP and a commit are both in play: **stacked sections** — WIP above, selected commit below (not a segmented switcher). |
| D4 | Staging / unstaged / untracked UI **only** appears in the WIP block. |
| D5 | Left rail is refs/worktrees/stashes accordion only (plus existing branch chip / search as needed). |
| D6 | Graph: pills first (tied to node), hash last. |
| D7 | Hunk modes: **Beside** (default) · **Wide** · **Focus** (full workspace; Esc restores prior chrome). |

## Window layout

### Pane roles after redesign

```
┌─ top bar ─────────────────────────────────────────┐
├─ session tabs ────────────────────────────────────┤
│ Left          │ Centre              │ Right       │
│ LOCAL         │ Graph (history)     │ Files       │
│ REMOTE        │                     │  ├ WIP      │
│ WORKTREES     │ Diff / hunk         │  └ Commit   │
│ STASHES       │ (mode-dependent)    │             │
├───────────────┴─────────────────────┴─────────────┤
│ terminal (optional)                               │
├───────────────────────────────────────────────────┤
│ status                                            │
└───────────────────────────────────────────────────┘
```

- **Left (`WorkspacePane::Files` → rename conceptually to Explorer):** accordion only.
- **Centre Graph:** history table; no longer carries the only path to commit files.
- **Centre Diff:** hunk view; width governed by hunk mode.
- **Right (`WorkspacePane::Inspector` → Files navigator):** fused WIP + commit file lists; commit meta (message, author, hash) may sit as a compact header above the commit file block, not as a second full pane of dead space.
- **Preset grid** (`LayoutPreset`) keeps three shapes, but defaults bias toward Graph + Diff sharing centre and Right always present when `files_rail`-equivalent for the **right** navigator is open. Exact `pane_grid` wiring is an implementation detail of the plan; the product rule is: right files pane is first-class.

### Hunk modes

| Mode | Behaviour |
|---|---|
| `Beside` | Diff shares horizontal space with the right files pane (current Columns / InspectorOverDiff spirit). |
| `Wide` | Diff expands across the centre column(s); right files pane stays visible at a **minimum width** clamp so navigation is never lost. |
| `Focus` | Diff fills the workspace; left explorer and right files pane hidden; previous open/ratios remembered; **Esc** or a chrome control restores. |

User control: toolbar control and/or shortcut (plan picks concrete binding; avoid conflicting with existing `CycleLayout` / `ToggleFilesRail`).

## Left explorer

### Sections

Each section header: chevron · title · count. Click toggles collapse. Independent collapse flags on `GitApp` or session UI bag (recommend **per-session** so worktree tabs do not fight):

```text
explorer_collapsed: { local, remote, worktrees, stashes }
```

| Section | Contents | Notes |
|---|---|---|
| LOCAL | Local branches | Right-click keeps Open as worktree / Detach. Current branch highlighted. |
| REMOTE | Remote-tracking branches | Group by remote if already cheap; otherwise flat list + filter. |
| WORKTREES | Catalogue rows | Open / remove ×; scrollbar **embedded** (must not cover ×). |
| STASHES | Stash list | Existing select / drop flows. |

Tags: either a fifth **TAGS** section or under LOCAL as a sub-block. **Recommendation:** fifth section `TAGS` for parity with GitKraken density and existing `refs.rs` data — include in this cut if collapse plumbing is shared; otherwise park under LOCAL and note in plan.

### Removed from left

- `changes_pane` (staged/unstaged/untracked tree, stage toolbar).
- Commit message box moves with staging to the **right WIP header** (or a slim strip directly above the WIP file list).

## Right files navigator (fused)

### WIP block (top)

Visible when the active session’s working tree has staged, unstaged, or untracked entries (or always shown empty with a quiet “Clean working tree” line — plan picks one; prefer **hide when clean** to save height).

- Sections: Staged · Unstaged · Untracked (existing fold behaviour).
- Actions: stage / unstage / discard / select-all / tree|list — moved from left `changes.rs`.
- Commit message + Commit/Stash commands live here (WIP-only).

### Commit block (below)

Visible when selection is `Commit` or `Tag` (and optionally when a stash’s tree is shown later).

- Header: short hash · subject line (one or two lines) · `+a −d` summary.
- File list: same click → `FocusDiffPath` behaviour as today’s `commit_files_block`.
- No staging actions on commit files.

### Stacking rules

- WIP only → only WIP block.
- Commit only (clean tree) → only Commit block.
- Both → WIP stacked above Commit; each block independently collapsible via a local chevron if height is tight.
- Selecting a graph WIP row / “working tree” focuses Diff on working-tree patches and may scroll the WIP block into view.

## Graph / history

### Column order (left → right)

1. **Ref pills** (Head / local / remote / tag / worktree markers) — visually attached to the node  
2. **Graph** canvas (`GraphCell`)  
3. **Message** (fill; no pills here)  
4. **Author** (fixed min width)  
5. **When** (fixed)  
6. **Hash** (fixed mono, **last**)

Focus toggle stays a trailing icon or moves into the history toolbar — must not steal hash’s column.

### Anti-deformation

- Header labels use `Wrapping::None` and column mins that fit the label (`Description` never stacks to `De-scri-pti-on`).
- Pills capped (existing +N) but drawn in the pills column / leading cell, not inside message.
- Row height may grow slightly if dual-line pills are needed; prefer single-line pills + truncation.

### WIP rows

Keep session WIP row(s); pill still marks working changes; hash column empty or `······`.

## Diff / hunk visuals

### This cut (unified refresh)

Match the reference as far as unified rendering allows:

- Stronger row backgrounds for add/remove (still theme tokens; raise alpha from ~12% toward a clearer GitKraken-like wash).
- Gutter markers `+` / `-` / space beside line numbers.
- Tighter vertical padding / line height.
- Keep dual line-number columns (old/new).
- Retain hunk stage/discard for working-tree hunks.

### Follow-up (ADR 0003 WP3)

Side-by-side panes, diagonal empty placeholders, intra-line highlight. Out of scope unless the implementation plan’s final optional task is explicitly pulled in.

## State

| Field | Owner | Notes |
|---|---|---|
| `explorer_collapsed` | session UI bag | local/remote/worktrees/stashes/(tags) |
| `hunk_mode` | `GitApp` (window chrome) | Beside / Wide / Focus |
| `focus_restore` | `GitApp` | Snapshot of rail visibility + layout when entering Focus |
| Changes UI (`marked_files`, `collapsed_*`, commit message) | session | Unchanged ownership; view moves right |
| `focused_diff_path` | session | Unchanged |

No new port methods. No adapter changes for the layout cut.

## Errors and edge cases

- Focus mode + Escape: restore exact prior left/right visibility and hunk mode; do not clear selection.
- Opening context menu / modal while Focus: allowed; dismiss restores Focus underneath or exits Focus — prefer **keep Focus** under modal.
- Embedded worktrees scrollbar must reserve layout space (already required; regression-guard in plan).
- Empty REMOTE / STASHES: section still listed with count `0`, collapsed by default optional.

## Testing (acceptance)

- Collapse each left section independently; counts match catalogue / refs.
- Clean WIP + commit selected → only commit files on the right; dirty WIP + commit → both stacked, WIP on top.
- Stage from right WIP; file click on commit block focuses Diff path.
- Graph headers do not wrap at default window size; hash is the rightmost data column; pills precede the canvas.
- Hunk Focus hides explorer + files; Esc restores.
- Worktrees × not covered by scrollbar.

## Implementation slices (for the plan)

1. Left accordion (LOCAL / REMOTE / WORKTREES / STASHES [/ TAGS]) + collapse state; remove left changes tree.
2. Move changes UI into right stacked navigator (WIP + commit).
3. Graph column reorder + anti-wrap mins + pills leading.
4. Hunk modes (Beside / Wide / Focus) + Esc restore.
5. Unified hunk visual refresh.
6. *(Optional)* Split diff WP3 spike — only if scheduled.

## Open points (non-blocking)

- Exact shortcut for hunk mode cycle.
- Whether TAGS is its own accordion section (recommended yes).
- Whether clean WIP shows a one-line placeholder or hides entirely (recommended hide).

## Rejected alternatives

- **Drop the right pane** and put files under the graph only — rejected; right pane is required for change navigation and dual WIP+commit context.
- **Segmented WIP | Commit switcher** — rejected; stacked sections preferred.
- **Keep duplicate left Changes + right Files** — rejected; fuse into one.
