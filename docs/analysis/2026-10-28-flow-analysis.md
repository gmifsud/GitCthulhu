---
type: analysis
title: GitCthulhu flow analysis
status: for-decision
created: 2026-10-28
owner: grego
evidence: ISL-shaped fixture (6 local, 103 remote, 3 linked worktrees, 2 stashes, dirty tree), origin/main 6eda178 vs feature/blissful-goldberg-rsnvfh 6dd913a
related: [ADR-0002, ADR-0003, docs/superpowers/specs/2026-10-27-conflict-resolver-design.md]
tags: [gitcthulhu, ux, flow, analysis]
---

# GitCthulhu flow analysis

## Verdict: Needs Attention

GitCthulhu has most of the *parts* of a Git client: refs, graph, diff, staging, worktrees, stashes, sync. What it lacks is the *flows* that join them. Seven structural causes explain almost every complaint (R1–R7 below). The bugs you reported are symptoms of them.

Fixed on the branch today: #1 stash, #2 search, #3–#5 graph space and the focus toggle. #6 (flow) needs the decisions in §6. The redesign in §5 is a proposal for sign-off under the PRD-first rule, **not yet built**.

> **Deployment note.** The build in your screenshot is `origin/main` (6eda178). Every GUI fix since then is on `feature/blissful-goldberg-rsnvfh`, which is not merged. That is why "search disabled" persisted.

## 1. Evidence

| Build | History on the same repository |
|---|---|
| `origin/main` | ![main](img/2026-10-28-main-history.png) |
| branch (6dd913a) | ![branch](img/2026-10-28-branch-history.png) |
| branch, stash selected | ![stash](img/2026-10-28-branch-stash.png) |

On `main`, Description is about 20 px wide and its header wraps one fragment per line. With your real repository's extra lanes, that header grows until every commit row falls below the fold: the "empty" History in your screenshot.

## 2. Anatomy today

| Region | Share of a 1280×800 window | Purpose | Problem |
|---|---|---|---|
| Top bar | 40 px | Path, branch chip, search, sync, layout toggles, Open/Settings | Path wraps (fixed); sync buttons are unlabelled glyphs |
| Tab strip | ~50 px | Worktree tabs | Fine; costs a full row even with one tab |
| Explorer | ~22 % width | LOCAL, REMOTE, WORKTREES, STASHES, TAGS | Five small fixed scrollers. 103 remotes in a flat list, about 3 visible at once; no filter; no folder grouping (`EYC/`, `MTL/`…) |
| History | ~50 % width × **~35 % height** | Graph and commits | The only thing you read continuously, yet it gets about a third of the height |
| Diff | below History | Patch of the selection | Takes about 25 % of the window permanently, even when it shows "No changes" |
| Inspector | ~25 % width | Selection label + commands + working tree + commit/stash files | Always shows the working tree, whatever is selected; a commit's files sit below the commit box |
| Terminal | ~130 px, open by default | Shell + AI transcript | Raw provider JSON (your screenshot); permanent height cost |
| Status bar | 24 px | State, page size, upstream | "50 commits loaded" is the whole history the app will ever show |

About **40 % of the window is chrome or idle panes** at any moment (tabs, empty Diff, terminal, five nested scroll areas).

## 3. Journey audit

Severity: **H** blocks or misleads, **M** slows, **L** polish.

### J1 Orient: "Where am I, what changed, is anything in progress?"
| Step | Today | Severity |
|---|---|---|
| Current branch and upstream | Chip + "Up to date with origin/development" ✅ | — |
| Merge/rebase/cherry-pick in progress | **Invisible.** The app never reads `MERGE_HEAD`, `REBASE_HEAD`, `rebase-merge/` or `CHERRY_PICK_HEAD`. After a conflicted merge you see only git's message | **H** |
| Conflicted files | Not listed as conflicted (CONTEXT.md: "not listed file by file yet") | **H** |

### J2 Inspect history: commit → files → diff
| Step | Today | Severity |
|---|---|---|
| Scan commits | 50 commits, then nothing. No paging, no "load more" | **H** |
| Find a commit | Search covers only those 50 (fixed today, still bounded) | M |
| Read a merge commit | Showed git's combined diff, usually **empty** (fixed today: first-parent) | **H**, fixed |
| Navigate with the keyboard | ↑/↓ do not move the selection in History | M |
| Branch → its commits | Clicking a branch selects it but shows **nothing** about it (no tip, no ahead/behind list, no scroll) | **H** |

### J3 Change: edit → stage → commit → push
| Step | Today | Severity |
|---|---|---|
| See working changes | WIP row + Inspector tree ✅ | — |
| Review a file | Click the file, and the diff appears in a different pane (centre) from the list (right) | M |
| Stage a hunk | Only from the Diff pane | — |
| **Safety** | Hunk buttons appeared on commit and stash diffs. "Discard hunk" reverse-applied *that commit's* hunk to the working tree (silent undo of HEAD's change) | **H**, fixed today |
| Commit | Message box at the top of the Inspector ✅ | — |
| Push | Unlabelled glyph in the top bar (↑ vs ⬆ for pull/push) | M |

### J4 Integrate: fetch / pull / merge / rebase → conflicts → continue
| Step | Today | Severity |
|---|---|---|
| Merge / rebase | Confirm dialogs ✅ | — |
| Conflict | Git's message in the status bar; nothing to resolve with | **H** (the Cthulhu Resolve CLI is the planned remedy) |
| Continue / abort | No UI | **H** |

### J5 Switch context: branch / worktree / stash
| Step | Today | Severity |
|---|---|---|
| Find a branch | Scroll a 3-row list, or Ctrl+P (added today) | M |
| Worktrees | Tabs + catalogue ✅ (the strongest flow in the app) | — |
| Stash | Selecting showed nothing; the list could fail on some git configs (fixed today) | **H**, fixed |

## 4. Root causes

| # | Cause | Symptoms it explains |
|---|---|---|
| **R1** | **Every pane is always on; there are no modes.** Inspect and Change share one fixed five-region layout | Squeezed history; idle Diff; "space everywhere" (#3, #4) |
| **R2** | **Shallow selection model.** One `Selection` drives everything, but Branch/Remote/Stash had no content and the Inspector always prepends the working tree | Stash showed nothing (#1); branch click does nothing; the Inspector reads the same whatever is selected |
| **R3** | **Commands scattered over four surfaces** (top bar, History toolbar, Inspector head, context menus), mostly unlabelled glyphs with a 2 s tooltip delay | "Flow not there"; focus toggle "gone" when its host overflowed (#5) |
| **R4** | **Truncated data.** Fixed 50-commit page, search over loaded commits only, flat remote list | History feels shallow; finding things requires Ctrl+P |
| **R5** | **Rendering defect upstream.** iced_graphics 0.13.0 ignores `Wrapping::None`, so any squeezed cell wraps and grows. No layout rule protected the primary column | Header collapse, two-line explorer rows, path wrapping (#3) |
| **R6** | **No operation awareness.** In-progress merge/rebase/cherry-pick/revert and conflicted paths are not modelled | J1/J4 dead ends |
| **R7** | **Terminal doubles as a debug log.** Provider exchanges are dumped verbatim | Raw JSON on screen; terminal "needed" open |

## 5. Proposed flow model (for sign-off)

A tourist map, not a final design: two modes over one stable frame, plus an operation banner.

```
┌ Top bar: repo ▾ (recent) · branch ▾ · ↓Fetch ↓Pull ↑Push (counts) · [ search / Ctrl+K ] · ⌘ Ctrl+P ┐
├ Operation banner (only when a merge/rebase/cherry-pick is in progress):                            ┤
│   REBASE 3/7 onto origin/development · 2 conflicted files  [Resolve…] [Continue] [Abort]          │
├──────────────┬─────────────────────────────────────────────┬──────────────────────────────────────┤
│ Refs (one    │  LOG MODE (default)                         │ Details of the selection             │
│ scroll,      │  History full height, paged 200 at a time,  │  commit: message, meta, files ▸      │
│ filter box,  │  ↑/↓ moves selection, Enter = open diff     │  branch: tip + ahead/behind lists    │
│ folders by   │                                             │  stash: files ▸   (no working tree)  │
│ prefix)      │  CHANGES MODE (Ctrl+1/Ctrl+2 or the WIP row)│ Working tree + commit box            │
│              │  files list ▸ diff side by side             │  (only in Changes mode)              │
├──────────────┴─────────────────────────────────────────────┴──────────────────────────────────────┤
│ Diff opens *in place of* History (Log mode) or beside the file list (Changes mode); Esc returns.  │
│ Terminal: collapsed by default (Ctrl+`); AI exchanges go to a log file, not the terminal.         │
└───────────────────────────────────────────────────────────────────────────────────────────────────┘
```

Principles:
1. **The primary column owns the space.** History, or the Diff when one is open, is never squeezed by secondary panes. The Diff appears on demand instead of always reserving height.
2. **The selection decides the Inspector.** One block per selection kind. The working tree appears only for the working-tree selection or in Changes mode.
3. **One command surface per scope.** Repository sync lives in the top bar with labels and counts. Selection commands appear in the Inspector header with labels. Everything is also in the Ctrl+P palette, with actions under a `>` prefix.
4. **State before action.** The operation banner makes in-progress Git state visible, and it is the entry point to Cthulhu Resolve.
5. **Bounded rendering.** Every single-line cell uses `one_line` until iced 0.14 removes the need.

## 6. Decisions needed (ranked)

| # | Decision | Recommendation | Why |
|---|---|---|---|
| D1 | Merge the branch into `main` now? | **Yes, via a PR** | Your running build is missing every fix since 6eda178 |
| D2 | Adopt Log/Changes modes (§5) | **Yes** | Removes R1 without a new framework; reuses pane_grid presets |
| D3 | Terminal collapsed by default, AI exchanges moved to a log file | **Yes** | Recovers about 130 px; R7 |
| D4 | History paging (200 per page, load on scroll) plus full-history search via `git log --grep/--author/-S` in the background | **Yes** | R4. Search over 50 commits is not search |
| D5 | Operation banner + conflicted-file list (detect state via `git rev-parse --git-path` + porcelain v2 `u` entries) | **Yes, before the resolver UI** | R6. It is also the integration point for Cthulhu Resolve |
| D6 | iced 0.14 upgrade | **Later, as its own work package** | Removes R5's workaround; breaking API changes across `gui_app` |

## 7. Backlog (in order; effort S ≈ ≤1 day, M ≈ 2–3 days, L ≈ a week)

| # | Item | Fixes | Effort | Acceptance |
|---|---|---|---|---|
| 1 | PR the branch to `main` | D1 | S | `main` shows "Filter history…" and Recent |
| 2 | Operation detection + banner + conflicted list | R6, J1, J4 | M | Given a conflicted rebase, the banner shows step, onto and conflicted count; Continue stays disabled until all paths are staged |
| 3 | History paging + ↑/↓/Enter navigation | R4, J2 | M | 1,000-commit repo: scrolling to the end loads pages and the frame stays under 16 ms |
| 4 | Selection-driven Inspector (branch tip + ahead/behind; no working tree for commit/stash) | R2 | M | Clicking a local branch shows its tip and its commits not in HEAD |
| 5 | Log/Changes modes; Diff on demand; terminal collapsed by default | R1, D2, D3 | L | At 1280×800, History shows ≥ 20 rows in Log mode |
| 6 | Explorer as one scroll with filter + prefix folders | R4, J5 | M | 103 remotes fold under `EYC/`, `EBS/`, `MTL/`; typing filters every section |
| 7 | Labelled commands; palette actions (`>` prefix) | R3 | M | Every command reachable from Ctrl+P |
| 8 | "Resolve conflicts…" opens Cthulhu Resolve (CLI contract) | J4 | S once the resolver exists | — |
| 9 | iced 0.14 upgrade; remove `one_line` | R5, D6 | L | No height-clamp workarounds left |

## 8. Fixed in this pass (branch `6dd913a`)

| Report | Root cause | Fix |
|---|---|---|
| #1 Stash | No Stash arm in `patch_query`; no stash block in the Inspector; human-format `stash list` parsing | Stash loads its first-parent diff and its files are listed; explicit `--format`; `Stash.commit_id` |
| (found) Merge commits empty | `git show` combined diff | `-m --first-parent` |
| (found) Hunk discard on commits | Missing selection guard | View and reducer allow hunk edits on working-tree files only; tested |
| #2 Search | Not merged to `main` (fixed on the branch yesterday) | See D1 |
| #3/#4 Graph space | 260 px gutter + unbounded lanes + iced wrap defect | Gutter removed, lanes capped at 10, header clamped, one pill + "+N", History 62 % of the split |
| #5 Focus toggle | Last header cell, clipped by overflow | Moved to the History toolbar |
