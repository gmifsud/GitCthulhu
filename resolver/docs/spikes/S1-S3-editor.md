---
type: spike-report
spike: [S1, S3]
title: AvaloniaEdit three-pane editor — latency and accessibility
status: done-on-linux
date: 2026-10-27
stack: Avalonia 12.1.4, Avalonia.AvaloniaEdit 12.0.0, .NET 10.0.113
environment: Ubuntu 24.04 VM, Xvfb, CPU-only Skia raster (no GPU)
result: S1 conditional-go, S3 decision-needed
tags: [cthulhu-resolve, phase-0, editor, accessibility]
---

# S1 / S3 — AvaloniaEdit three-pane editor

## Verdict

| Spike | Verdict |
|---|---|
| **S1 latency** | **Conditional go.** The band model works with AvaloniaEdit at 100k lines × 3 panes. Medians hit frame rate on a CPU rasteriser. The p95 targets miss by 4–11 ms on CPU raster, so they must be re-measured on real GPU hardware before closing |
| **S3 accessibility** | **Kill criterion met as written.** AvaloniaEdit has no automation peers, and Avalonia 12.1.4 has no text-range provider API to build one on. Needs your decision (below) |

## S1 setup

`spikes/S1.ThreePane/Spike.cs` (throwaway harness):

- **Workload:** synthetic C#-shaped base of 100,000 lines; each side gets ~333 scattered edits (¼ insertions). The plan uses the in-process histogram planner; the result follows D4.
- **Panes:** three `TextEditor`s plus two `BandCanvas` controls. The bands are cubic-Bézier quads drawn only for on-screen regions (binary search + walk).
- **Sync:** piecewise-linear `LineMap` (spec §10.5); the master pane drives the others.
- **Harness:** waits for real frames with `RequestAnimationFrame`, using `RunContinuationsAsynchronously`. Without that, the first run measured fake 0.01 ms "frames".
  - 600 frames of 40 px scroll.
  - 40 conflict jumps.
  - 200 keystrokes mid-file, timed to the next rendered frame.
- **Reproduce:** `S1_OUT=out.json dotnet run -c Release` under any display. `S1_LINENUMBERS=stock|fast|none` selects the line-number margin.

## S1 results (Release, 3 runs each, medians of p-values shown)

| Metric | 100k lines, stock margin | 100k, custom margin | 100k, no margin | 2k lines | Spec §10.9 target |
|---|---|---|---|---|---|
| Scroll frame p50 / p95 (ms) | 16.6 / 35.8 | 16.4 / 27.5 | 16.4 / 18.3 | 16.4 / 24.6 | ≥ 55 fps p50 ✅; p95 ⚠ |
| Keystroke → next frame p50 / p95 (ms) | 16.4 / 23.7 | 16.3 / 20.8 | 16.4 / 16.9 | 16.4 / 21.1 | ≤ 16 ms p95 ⚠ (one frame = 16.7) |
| Sync handler p50 / p99 (ms) | 0.08 / 0.7 | 0.08 / 0.7 | 0.08 / 0.7 | — | Band model ✅ |
| Conflict jump → frame (ms) | ~20 | 18–55 | — | — | — |
| First frame after launch (ms) | ~3,000 | ~2,300 | — | **~1,300** | ≤ 800 (2k) ❌ |
| Working set (MB) | ~525 | ~525 | ~535 | ~210 | ≤ 250 (100k) ❌ |
| Managed heap (MB) | 129 | 176 | 129 | 11 | — |

The 100k first-frame figure includes generating the workload and planning it in-process (0.4–0.6 s).

## Findings

1. **The band model is cheap.** Mapping and repainting the bands costs 0.08 ms median per scroll event. Alignment needs no spacer lines (ADR 0004's premise holds).
2. **AvaloniaEdit defect: `TextEditor.ScrollToVerticalOffset` is a no-op in 12.0.0.** The template's `PART_ScrollViewer.Offset` works. The `IEditorSurface` adapter will own scrolling.
3. **The stock line-number margin dominates edit latency.** Before brush caching it pushed keystroke p95 to 37 ms; without a margin it is 16.9 ms. A custom margin that caches shaped numbers and re-measures only on digit-count change recovers about half the gap. The rest is glyph rasterisation on the CPU raster used here.
4. **Text-model cost is not the problem.** `Document.Insert` takes ≈0 ms on a detached document, a bare `TextArea`, or an off-screen editor. It is 3–4 ms only on an on-screen editor (visual-layer reaction), within budget.
5. **Allocation hygiene matters.** Per-line brush allocation in a background renderer produced dozens of gen-2 collections. All render-path objects must be cached and immutable (a Phase 1 rule).
6. **Not met here:** cold start and memory.
   - Cold start runs with JIT only; ReadyToRun / Native AOT have not been tried.
   - The 100k working set includes three 5 MB documents, their line trees, the workload copies, and Skia/font caches.

   Both are Phase 1 measurements on Windows with R2R. The targets stay as proposed until then.

**What this environment cannot tell us:** GPU-backed rendering. Avalonia uses Skia over GL/Vulkan/Direct3D on real desktops. The p95 numbers above are an upper bound, not a forecast.

## S3 desk check (accessibility)

| Check | Result | Evidence |
|---|---|---|
| AvaloniaEdit automation peers | **None.** No `AutomationPeer` types and no `OnCreateAutomationPeer` override in `AvaloniaEdit.dll` 12.0.0 | Reflection over the shipped assembly |
| Avalonia text-range provider API | **None.** `Avalonia.Automation.Provider` has Value, RangeValue, Scroll, Selection, Toggle, Invoke and ExpandCollapse providers. There is no `ITextProvider`/`ITextRangeProvider` | Reflection over Avalonia 12.1.4 |
| Avalonia's own `TextBox` | Implements `IValueProvider` only (the whole value as one string) | Same |

**Implication:** a screen reader cannot navigate or edit the result pane line by line or by caret with AvaloniaEdit on Avalonia 12, and we cannot fix that inside our application. It needs platform-level text-pattern support on three OSes:

- Windows: UIA TextPattern.
- macOS: NSAccessibility text attributes.
- Linux: AT-SPI Text.

### Options (ranked)

| Option | What you get | Cost | Recommendation |
|---|---|---|---|
| **A. Keep AvaloniaEdit; accessible *conflict navigator*** | Full keyboard operation, plus a navigator built from standard controls: each region is a list item whose name announces kind, position and side texts (`IValueProvider`), with live announcements on navigation and actions. Screen-reader **editing** of the result is a documented v1 limitation | S | **Recommended if no screen-reader user depends on v1** |
| B. Monaco in `Avalonia.Controls.WebView` (the ADR 0004 fallback) | Monaco's screen-reader mode, via WebView2 (Win) and WKWebView (mac). The .NET core is unchanged | L: JS bridge, second runtime, startup/memory, Linux engine unverified | If accessible editing is a v1 requirement |
| C. Contribute text-pattern providers to Avalonia upstream | Fixes the root cause for everyone | XL, outside our control | Long-term, in parallel with A |

## Next

- **Decision O1** (spec §14), A or B. Everything else in Phase 1 is independent of it.
- **Phase 1 measurement:**
  - The harness on Windows (GPU, R2R) and macOS.
  - A custom margin that pre-shapes digit glyphs once.
  - A memory breakdown with `dotnet-counters`/`dotnet-gcdump`.
