---
type: spike-report
spike: S2
title: diff3 planner against git
status: done
date: 2026-10-27
git: 2.43.0
result: go
tags: [cthulhu-resolve, phase-0, merge]
---

# S2 — diff3 planner against git

**Verdict: Go, with one design correction.** The planner reproduces git's merge chunking exactly. Use git's own line diffs to build plans; keep the in-process diffs for display only.

## Questions

1. Does `Diff3Planner` partition a merge the way git does (overlap rule, touching edits, same-position insertions, identical changes)?
2. Do in-process Myers/Histogram diffs align like git's xdiff?
3. Which diff does a real `git merge` (ort) use for file content?

## Method

All runs use fixed seeds and are reproducible with `dotnet test --filter OracleTests`.

| Experiment | Inputs | Oracle | Result |
|---|---|---|---|
| E1 Unique lines | 150 cases × 2 algorithms; every line distinct, so there is a single LCS | `git merge-file -p --diff3` | **300 / 300 byte-exact** |
| E2 Ambiguous, in-process diffs | 400 cases, 4-letter alphabet | `git merge-file -p --diff3` | Myers: conflict count 76.5 %, byte-exact 49.0 %. Histogram: 67.0 % / 41.2 % |
| E3 Ambiguous, **git's diffs** fed to the planner | Same 400 cases; edits from `git diff --no-index -U0 --diff-algorithm=myers` hunk headers | `git merge-file -p --diff3` | **400 / 400 byte-exact** |
| E4 Real `git merge` | 120 fixture repos, `merge.conflictStyle=diff3`, labels normalised | Plans from git's Myers vs Histogram diffs | 99 conflicted merges. Histogram plan **99 / 99**; Myers plan 46 / 99 |

## Findings

1. **Planner semantics are git's.** E3 separates chunking from alignment. Given identical edit scripts, the output matches `xdl_do_merge` at the diff3 level byte for byte:
   - overlap test `next.start ≤ group.end`
   - EAGER identical-change rule
   - git adds a newline before a marker when a section ends without one
2. **Alignment is where tools differ.** On repetitive text many minimal edit scripts exist. xdiff picks one by its own tie-breaking and slides change groups (`xdl_change_compact`), and our diffs pick another. Both are valid, but users see git's choice in their IDE and terminal markers.
3. **`git merge` and `git merge-file` disagree.** (CI confirms histogram parity on git 2.56 for Windows and on the macOS runner's git.) `merge-file` defaults to Myers. ort's content merge matched Histogram in every conflicted case on git 2.43, so the spec's assumption of "Myers as git's default" is wrong for merges. The release that switched ort to histogram is **[unverified]**.

## Decisions (spec and ADR 0005 amended)

- **D-S2-1.** Plans are built from **git's** line diffs (`GitLineDiff`, histogram, every alignment option pinned). Regions are then identical to the conflicts git wrote, so any marker the user saw elsewhere maps 1:1 to a resolver region.
- **D-S2-2.** Algorithm choice is **verified per file, not assumed** (Phase 1). When the worktree file still holds git's markers, render the plan under histogram, then Myers, and pick the one that reproduces them. If neither does (rerere, a merge driver, hand edits), use histogram and show "regions may differ from the markers in your file".
- **D-S2-3.** In-process diffs (`LineDiff`) are used for word-level highlighting, result-vs-input views and the no-git fallback. No content transformation depends on their alignment.

## Performance (in-process, one core, Release build not yet measured)

Histogram planning on 100k lines with 600 scattered edits completes inside the 3 s guard in the Debug test run. The proposed §10.9 target is ≤ 600 ms. It is re-measured with BenchmarkDotNet in Phase 1; process cost of `GitLineDiff` is two `git diff` calls per file.

## Follow-ups

- Phase 1: the marker-reproduction check (D-S2-2), and BenchmarkDotNet for planning plus `GitLineDiff` on 10k/100k lines.
- Phase 2: E4 on the CI git matrix (2.38, 2.43, latest) to record which versions use histogram in ort.
