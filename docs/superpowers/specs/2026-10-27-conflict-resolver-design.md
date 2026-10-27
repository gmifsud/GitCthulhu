---
type: spec
title: Cthulhu Resolve — standalone Git conflict resolver
status: draft-for-signoff
created: 2026-10-27
owner: grego
related: [ADR-0004, ADR-0005, ADR-0006, CONTEXT.md]
tags: [gitcthulhu, merge-tool, prd, architecture]
---

# Cthulhu Resolve — standalone Git conflict resolver

Status: **Draft for sign-off.** This is the PRD-first gate: no execution code until §0 Sign-off is ticked.
Related: ADR 0004 (stack), ADR 0005 (Git CLI is authoritative), ADR 0006 (immutable merge plan + anchored result).

> **Evidence key.** `[V-SRC]` verified from JetBrains' open-source platform strings/keymaps (intellij-community, first-hand).
> `[V-DOC]` from JetBrains help pages via search summaries (second-hand; jetbrains.com was unreachable from the
> research sandbox — re-read before quoting). `[V-GIT]` verified in `git/git` Documentation sources (the AsciiDoc that
> git-scm.com renders). `[E]` reproduced by experiment on git 2.43.0 / git-lfs 3.4.1. `[U]` unverified.
> `[P]` proposed by this spec (not a Rider feature).

---

## 0. Sign-off

| # | Decision you are signing | Default in this spec |
|---|---|---|
| D1 | Stack | .NET 10 core + Avalonia 12 + AvaloniaEdit, editor behind `IEditorSurface`; Monaco-in-WebView as the contained fallback (ADR 0004) |
| D2 | Git access | `git` CLI is authoritative; no libgit2 in the resolution path (ADR 0005) |
| D3 | Result model | Immutable merge plan from index stages; result is a text buffer with anchored regions (ADR 0006) |
| D4 | Initial result | Base + all non-conflicting changes applied (each shown as applied, individually revertible); conflicts start showing base text, **unresolved** |
| D5 | Staging | Never implicit. Mergetool mode: caller (git) stages. Repo mode: explicit *Stage* action |
| D6 | Repository | New repo `cthulhu-resolve` (.NET); GitCthulhu (Rust/iced) integrates over the CLI contract only |

---

## 1. Executive recommendation

1. **Build a .NET 10 resolver whose core is a headless library** (`Resolve.Core`): Git input acquisition, diff/diff3, merge plan, anchored result document, session journal, validation. 100 % of resolution semantics are testable without a window.
2. **UI: Avalonia 12 + AvaloniaEdit**, using the *Rider/Meld band model* for alignment (curved connector bands + piecewise-linear scroll sync) instead of VS Code's *spacer-line* model. That choice reduces the editor requirement to things AvaloniaEdit demonstrably has (line↔y mapping, background renderers, custom margins, scroll events) and avoids needing view zones, which AvaloniaEdit lacks.
3. **Isolate the editor behind `IEditorSurface`** and run spike S1 (three synced AvaloniaEdit panes + bands + 100k lines) and S3 (accessibility peers) first. Kill criterion: if S1 misses the latency budget or S3 cannot expose text to UIA/AX/AT-SPI with bounded effort, switch to Monaco hosted in `Avalonia.Controls.WebView` (MIT since 11.4) behind the same interface. The core does not change.
4. **Git CLI is the single source of truth** for conflict inputs (unmerged index stages, attributes, filters, operation state), staging and continuation. Every command goes through a typed runner with an argument vector, `-z` output, `--` separators and literal pathspecs.
5. **Four state axes plus Git state, never conflated.** A file is "safe" only when content is resolved, the result is saved, and validation is clean. Staging and operation continuation are separate, explicit actions.
6. **First vertical slice (Phase 1):** a `git mergetool` launch on a both-modified text file. Read real stages → three panes → resolve a conflict → edit → atomic save → correct exit code. Then the repo mode.
7. **GitCthulhu integration:** GitCthulhu (iced) launches the resolver over the CLI (process boundary). This keeps ADR 0001's spirit: the window names no adapter, and the iced `text_editor` is not asked to be a merge editor. It has no gutter, line backgrounds or undo `[V-SRC iced 0.13.1 text_editor.rs]`.

**Why not stay in Rust/iced?** The iced 0.13 `text_editor` has no line decorations, gutter or undo. Building a production code editor would be the project rather than the resolver. Rust remains viable for the core, but it buys nothing over .NET given your stack and the editor decision. *Formal note:* the core/editor split is a functor boundary. The core maps (inputs, actions) → (document, region states) purely, and the UI is a view of that state. This is the same reducer discipline as ADR 0002.

---

## 2. Verified Rider baseline and feature-parity matrix

### 2.1 Inventory (what Rider does)

| Area | Behaviour | Evidence |
|---|---|---|
| Conflicts dialog | Title "Conflicts", "The following file(s) have conflicts:"; buttons **Accept Yours / Accept Theirs / Merge… / Open…** | [V-SRC VcsBundle 564-569] [V-DOC Resolve_conflicts.html] |
| | Columns "Name", "Yours (branch)", "Theirs (branch)"; cells Modified / Deleted | [V-SRC VcsBundle 570; GitBundle 130-135] |
| | Header "Merging branch X into branch Y"; "Rebasing branch X onto Y, revision…"; "Conflicts during cherry-picking commit…" | [V-SRC GitBundle 1404-1408] |
| | **Resolve All Simple Conflicts** (non-conflicting + statically-resolvable, across files), with result toasts | [V-SRC VcsBundle 590-595] [V-DOC] |
| | Per file: Resolve Manually, Review Changes, Accept '{side}', **Revert Conflict Resolution** (confirm) | [V-SRC VcsBundle 576-581] |
| | Accepting a side over merged edits asks "Overwrite Changes / Discard and Accept" | [V-SRC VcsBundle 582-584] |
| | Closing with partial work: "Discard Changes? / Continue Merge" | [V-SRC VcsBundle 585-588] |
| | **Accept and Finish** once all resolved; `{resolved}/{total}` progress | [V-SRC VcsBundle 575, 596-597] |
| | "Group by directory" checkbox | [V-SRC VcsBundle 573] |
| Merge window | "Merge Revisions for {path}"; left = yours (read-only), centre = **editable result**, right = theirs (read-only) | [V-SRC VcsBundle 571] [V-DOC] |
| | Centre initially holds the **base** revision | [V-DOC Resolve_conflicts.html] |
| | Pane titles: "Your version, branch X" / "Changes from X"; generic "Base version" | [V-SRC GitBundle 1411-1419; DiffBundle 100-105] |
| Base inspection | **No base pane.** Compare actions: Base↔Left, Base↔Middle, Base↔Right, Left↔Middle, Middle↔Right, Left↔Right | [V-SRC ActionsBundle 1671-1676] |
| | "Highlight modified lines in gutter" (relative to base) | [V-SRC DiffBundle 121] |
| Non-conflicting | Toolbar "Apply non-conflicting changes: Left / All / Right" | [V-SRC DiffBundle 170, 313-315] |
| | "Resolve Simple Conflicts" (`Diff.MagicResolveConflicts`); simple = e.g. start and end of the same line changed on different sides | [V-SRC ActionsBundle 1670] [V-DOC] |
| | Settings: "Automatically apply non-conflicting changes"; "Automatically resolve conflicts in import statements" | [V-SRC DiffBundle 119-120] |
| Per-hunk | Accept Left/Right, **Append** Left/Right, Ignore Left/Right, Resolve Automatically; Ctrl+click turns apply into append | [V-SRC ActionsBundle 1660-1666; DiffBundle 254] [V-DOC] |
| | Undo entries "Accept change", "Ignore change", "Reset resolved change" | [V-SRC DiffBundle 61-63] |
| Navigation | Next/Prev Difference, Next/Prev Conflict, Next/Prev File, Select Opposite Pane | [V-SRC ActionsBundle 272-278, 1654-1659] |
| Keys ($default) | F7 / Shift+F7 diff; Ctrl+Shift+D settings; Ctrl+Shift+Tab opposite pane; Alt+Shift+←/→ file; Ctrl+Alt+R / Ctrl+Alt+A apply left/right; **conflict nav unbound** | [V-SRC $default.xml] |
| | Rider VS keymap lists F8 / Shift+F8 | [V-DOC, older page] |
| View | Synchronize Scrolling; Collapse Unchanged Fragments; Highlighting: Lines/Words/Split/Chars/None; Ignore: None/Trim/Whitespace/Whitespace+empty lines; Highlighting level | [V-SRC DiffBundle 191-209, 267-280] |
| | Changing highlighting mid-merge requires a restart | [V-SRC DiffBundle 175] |
| Finish | Accept Left, Accept Right, **Apply Changes**, **Save and Close**, Cancel; counter "N changes, M conflicts" | [V-SRC DiffBundle 33, 249-253] |
| | Apply with leftovers: "Unprocessed Changes — file will be saved exactly as shown in the Result pane" | [V-SRC DiffBundle 76-82] |
| | "Apply Changes and Mark Resolved" | [V-SRC DiffBundle 176] |
| External tool | Merge tool settings with %1 left, %2 right, %3 base, %4 output; "Mark as Resolved" after | [V-SRC DiffBundle 126-146, 214-216] |
| AI | "Merge with AI" in Merge Revisions dialog; per-change Revert; 2024.2+ | [V-DOC ai-assistant/ai-in-vcs-integration.html] — Rider availability [U] |

**Unverified Rider details** (kept as assumptions, none architecturally material):
- Whether "resolved" means a literal `git add` [U].
- Ordering of Append Left + Append Right [U].
- A "show base" toggle [U, appears absent].
- Rebase behaviour of "Changes from server" [U].
- Binary/modify-delete flows beyond Accept Yours/Theirs [U].

No recording is needed: none of these changes the architecture. Where Rider is silent, this spec makes an explicit choice.

### 2.2 Parity matrix

| Capability | Rider | Cthulhu Resolve v1 | Notes |
|---|---|---|---|
| 3 panes, editable centre | ✅ V | ✅ | |
| Base view | compare actions only | ✅ **[P] base as 4th toggleable pane + compare actions** | Rider's compare-only model hides intent; ours adds a pane |
| Initial result = base | ✅ V | ⚙ default base+non-conflicting applied (D4) | Setting `initialResult = Base\|BaseWithNonConflicting` |
| Accept / Append / Ignore per side | ✅ V | ✅ + **explicit "Both: L→R / R→L"** | Ordering is explicit, not modifier-dependent |
| Apply non-conflicting L/All/R | ✅ V | ✅ | Skips manually edited regions (§7.2) |
| Resolve simple conflicts | ✅ V | ✅ Phase 3 (line-internal token merge) | Off by default; preview first |
| Import-statement auto-resolve | ✅ V | Phase 5 (semantic) | C# `using`, TS `import` |
| Synced scroll / collapse / highlight / ignore-ws | ✅ V | ✅ | Ignore-ws is display-only (§10.6) |
| Conflict-only navigation keys | unbound by default | ✅ bound (F8 / Shift+F8) | |
| Explicit "mark resolved" after manual edit | partial (unprocessed dialog) | ✅ **[P] required acknowledgement** | §7.2 |
| Separate save / stage / continue | merged into Apply | ✅ **[P] four explicit actions** | §3.2 |
| Source identity (SHA, subject, role) | branch names | ✅ **[P] role + ref + SHA + subject + path** | §3.3 |
| Rebase label safety | names sides | ✅ **[P] role-first labels + banner** | |
| Crash-recovery journal | [U] | ✅ **[P]** | §9 |
| Intentional-difference records | — | ✅ Phase 5 **[P]** | §9.6 |
| AI merge | ✅ V (AI Assistant) | Phase 5, opt-in | |

---

## 3. Product scope and non-goals

### 3.1 Modes

**A. Merge-tool mode.** One file, launched by `git mergetool` or any tool that passes base/local/remote/merged paths. Blocking. The exit code is the contract (§8).

**B. Repository mode.** The resolver opens a worktree, detects the operation, lists unmerged paths, and resolves them in sequence. Supported operations: merge, rebase (merge and apply backends), cherry-pick, revert, stash apply/pop, `checkout -m`, `am`.

### 3.2 The four user actions are distinct

| Action | Effect | Reversible? | Confirmation |
|---|---|---|---|
| **Resolve** (content) | Edits the in-memory result and region states only | Undo | none |
| **Save** | Atomic write of the result to the working-tree path (after a backup on first save) | Restore from backup | Only if unresolved regions remain or the file changed externally |
| **Stage** | `git add -- :(literal)<path>` (or `git rm` / `update-index` for file-level cases) | `git restore --staged`? **No**: unmerged stages are lost on add. Use *Reset file to conflicted* (§4.7) | When validation has warnings |
| **Continue operation** | `git merge --continue` / `rebase --continue` / `cherry-pick --continue` / `revert --continue` / `am --continue` | Operation-specific | **Always.** Shows that repository hooks and the editor may run |

The resolver never calls *Continue* implicitly, never commits, and never runs `--abort`, `reset`, `clean` or `checkout -- .` on its own initiative. *Abort operation* exists as an explicit, double-confirmed action showing the exact command.

### 3.3 Source identity

Every pane header shows four lines, collapsible to one:

```
ROLE       ▸ Your commit being replayed            (never just "theirs")
REF        ▸ feature/23542-Eyesel-SL-ExternalServices
COMMIT     ▸ d9c2046  chore(fork): clean entity module…   (Gregory Mifsud, 12h)
PATH       ▸ QiSL.Fabric/.../ExternalServicesConfig.json  (renamed from …)
```

Role labels per operation. Index stage 2 is **always the left pane** and stage 3 the right pane: the stage mapping is fixed, and only the role labels change.

| Operation | Stage 2 (left) role | Stage 3 (right) role | Ref sources `[V-GIT]` `[E]` |
|---|---|---|---|
| merge | **Current branch** (`HEAD` → branch) | **Incoming branch** | `MERGE_HEAD`, `MERGE_MSG` ("Merge branch 'x'") |
| rebase (merge backend) | **Upstream / onto + already-rebased commits** | **Your commit being replayed** | `rebase-merge/onto`, `head-name`, `REBASE_HEAD`, `stopped-sha`, `msgnum`/`end` |
| rebase --apply / am | same as rebase | **Patch being applied** | `rebase-apply/` |
| cherry-pick | **Current branch** | **Picked commit** | `CHERRY_PICK_HEAD` |
| revert | **Current branch** | **Inverse of commit** (shown as "parent of X") | `REVERT_HEAD` |
| stash apply/pop | **Working tree before apply** ("Updated upstream") | **Stashed changes** | No `*_HEAD`; inferred as described below |
| checkout -m | **Target branch** | **Your local changes** | No head file; inferred |

Rebase guard `[P]`: a persistent banner reads *"Rebase: LEFT is the branch you are rebasing onto (Git's "ours"). RIGHT is your own commit d9c2046 from feature/x (Git's "theirs")."* Actions are labelled by role ("Take your commit"), never "Accept theirs". Git's own words appear only in a secondary tooltip. This follows git-rebase's statement that the sides are swapped `[V-GIT git-rebase.adoc]`.

Inference for head-less operations:
- `AUTO_MERGE` present, unmerged entries present, and no `MERGE_HEAD`/`REBASE_HEAD`/`CHERRY_PICK_HEAD`/`REVERT_HEAD`: "Stash or checkout -m (inferred)" `[E]`.
- The marker labels in the working-tree file disambiguate: `Updated upstream`/`Stashed changes` means a stash `[E]`.
- Otherwise the operation is labelled **Unknown — sides shown as Git stage 2 / stage 3**. The UI never guesses roles.

### 3.4 Non-goals (v1)

- Remote hosting integration (PRs, review threads). Resolution is fully local.
- Running a build or tests implicitly.
- Semantic or AI merging as a dependency. It is Phase 5 and optional.
- A full IDE: no language servers in v1; syntax highlighting and parse-only validation only.
- Editing the read-only source panes. They are immutable by construction.
- Interactive rebase planning and history rewriting.
- Re-implementing Git's merge strategies (ort/recursive renames). We consume Git's results.

---

## 4. UX wireframes and interaction specification

### 4.1 Repository workspace (annotated)

```
┌─ ① Operation bar ───────────────────────────────────────────────────────────────────────────┐
│ ISL-Backend ▸ REBASE 3/7  feature/23542 onto origin/development   ▸ 6/9 files resolved ▸ [Continue rebase…] │
│ ⚠ LEFT = branch you rebase onto (git "ours") · RIGHT = your commit d9c2046 (git "theirs")                   │
├─ ② Files ─────────┬─ ③ LEFT (stage 2) RO ─┬─ ④ RESULT (editable) ─┬─ ⑤ RIGHT (stage 3) RO ─┬▕⑥│
│ Filter [   ] ⏷    │ Upstream+rebased       │ Result  Path.cs        │ Your commit d9c2046      │▕ │
│ ▾ Unresolved (3)  │ origin/development     │ ● 2 conflicts left     │ feature/23542            │▕▒│
│  ✕ UU Action…cs  │ 8ea6c53 #24473-MTL…    │ ◐ 1 manual (ack?)      │ d9c2046 chore(fork)…     │▕ │
│  ✕ UD Detail…cs  │────────────────────────┼────────────────────────┼──────────────────────────│▕▒│
│  ✕ AA Seeder.cs  │ 12  using Abp…      ⟫ │ 12  using Abp…         │ ⟪  using Abp…         12 │▕ │
│ ▾ Resolved (6)    │ 13 ▌using QiSL.Ent… ⟫✕│▓13  ░base text░        │✕⟪ using QiSL.Cust… ▐ 13 │▕█│
│  ✓ Entity…cs  ◇S │ 14 ▌                    ╲│▓14                     │╱                  ▐ 14 │▕ │
│  ✓ Config.json ◇ │ …                      ⟍ band ⟋                  │ …                        │▕▒│
│ Legend ✕ ◐ ✓ ◇staged│                        │                        │                          │▕ │
├───────────────────┴────────────────────────┴────────────────────────┴──────────────────────────┴──┤
│ ⑦ Inspector tabs: Diagnostics · History (actions + undo) · Validation · Git output                 │
│ ✓ No conflict markers  ⚠ JSON parse error at 41:7  ℹ Encoding UTF-8 BOM (matches LEFT/RIGHT)       │
├────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ ⑧ [Accept Left] [Accept Right] [Both L→R] [Both R→L]   │ [Save ⌃S] [Stage ⌃⇧S] [Next file ⌥⇧→]   │
└────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

① Operation bar: repository, operation, step (`msgnum/end`), progress counts, the *Continue* button (disabled until the gate in §4.8 holds), and the role banner.
② File list: grouped Unresolved/Resolved. Each row shows the porcelain XY code (UU/UD/DU/AA/AU/UA/DD), a type icon (text/binary/symlink/submodule/LFS), and state glyphs for the content (✕◐✓), saved (●) and staged (◇) axes. Filters: state, conflict type, path glob.
③⑤ Read-only sources: line numbers, syntax highlighting, change gutter. Hunk controls sit on the inner edge: `⟫` apply into the result, `✕` ignore this side.
④ Result: editable. Region tints: unresolved conflict (red band), resolved-from-left (blue), resolved-from-right (green), both (purple), manual (amber hatched), applied non-conflicting (faint tint).
⑥ Overview ruler: the whole file with conflicts █ and changes ▒. Click to jump.
⑦ Inspector: diagnostics, the action history (which mirrors the undo stack), validation, and raw Git output.
⑧ Action bar: whole-file actions on the left, persistence on the right.

### 4.2 Focused single-file (merge-tool mode)

The file list ② and operation bar ① collapse to a one-line header: `Merging <path> · LEFT <label> · RIGHT <label> · [Base ⌥B]`. The footer holds **[Save & exit 0] [Exit unresolved (1)] [Cancel — leave file untouched (1)]**, and the exit codes are printed on the buttons.

### 4.3 Repository-wide overview

A table view: path, XY, kind, conflict count, content state, saved, staged, validation, and "last touched". Bulk actions for selected rows:
- Apply all non-conflicting (skips any file with manual work).
- Take LEFT for file / Take RIGHT for file. Shows a per-file list of what will be discarded, then needs confirmation.
- Stage resolved and validated.

Bulk actions never include *Continue*.

### 4.4 Base-revision inspection `[P]`

- `⌥B` toggles a **4th pane, BASE (stage 1)**, to the left of RESULT. The layout becomes LEFT | BASE | RESULT | RIGHT, with bands drawn between adjacent pairs.
- A compare popover (`⌥C`) offers BASE↔LEFT, BASE↔RIGHT, BASE↔RESULT, LEFT↔RESULT, RESULT↔RIGHT and LEFT↔RIGHT as a two-pane overlay. This matches Rider's compare set [V-SRC].
- No base (add/add, or a missing stage 1): the pane shows "No common ancestor — every difference is a conflict" (§5).
- With multiple merge bases (virtual ancestor): the BASE header reads "virtual merge base (recursive)". Base content then comes from stage 1 as Git produced it, which may itself contain nested conflict markers. The header warns about this.

### 4.5 Layout behaviour

| Width | Behaviour |
|---|---|
| ≥ 1600 px | Full: files + 3 panes (+ base) + inspector docked bottom |
| 1200–1599 | File list collapses to a rail (⌥1 toggles); base pane opens as overlay |
| 900–1199 | **Two-pane mode**: RESULT plus one source at a time (tabs LEFT / RIGHT / BASE); bands become a gutter strip |
| < 900 | Single pane with a source picker; hunk actions move to an inline lens above each region |

Every pane can be resized by splitter drag (minimum 240 px of text), collapsed to a rail, and maximised (`⌥Enter` toggles maximise for the focused pane). Layout is saved per mode.

### 4.6 Editing and navigation

| Concern | Spec |
|---|---|
| Highlighting | TextMate grammars (via TextMateSharp) for C#, TS/JS, JSON, YAML, XML/csproj/props, PowerShell, sh, SQL, Markdown, Dockerfile, Bicep, HCL. Unknown types are plain. Highlighting must never block the UI and runs incrementally per viewport |
| Line/word diff | Line regions from §10. Intra-line token diff (word, punctuation, whitespace runs) inside changed regions only, with a time budget (§10.7) |
| Alignment | Band model: a monotone piecewise-linear map between the panes' line spaces, computed from region boundaries. Bands are drawn as Bézier quads in the gutters between panes |
| Sync scroll | On by default. The focused pane is the master. Others scroll by φ(y) using the region map: proportional inside a changed region, 1:1 in equal runs. Toggle `⌥S`; when off, panes scroll independently and bands still draw (clamped to the viewport) |
| Overview & counters | Ruler per pane; header counter `2 conflicts · 5 changes · 1 manual` |
| Navigation | **F8 / Shift+F8 next/prev unresolved conflict**; F7 / Shift+F7 next/prev change (any region); Alt+Shift+→/← next/prev file; Ctrl+G go to line; Ctrl+F find (per pane), Ctrl+Shift+F find across the three panes; Ctrl+Shift+Tab focus opposite source |
| Hunk keys (focused region) | Ctrl+Alt+← accept LEFT; Ctrl+Alt+→ accept RIGHT; Ctrl+Alt+↓ both L→R; Ctrl+Alt+↑ both R→L; Ctrl+Alt+Backspace ignore (keep base); Ctrl+Alt+R reset region; Ctrl+Enter mark manual region resolved |
| Whitespace | Toggle visible whitespace (`·` `→` `¶` `␍`). CR is always shown in mixed-EOL files. Toggle soft wrap |
| Focus & a11y | Tab order: files → left → result → right → action bar. Every hunk control is a focusable element with an accessible name such as "Conflict 2 of 3, lines 13–14, accept left". Region state is announced on navigation. High contrast theme. No information carried by colour alone (glyph + pattern per state) |
| High DPI | Per-monitor DPI; band geometry in device-independent units; pixel-snapped 1 px rules |
| Undo/redo | One linear history for the result. Hunk actions are compound entries that carry region-state transitions, so undo restores text **and** state together (§7.4) |
| Manual-edit preservation | Hunk identity never depends on result text (ADR 0006). Edits around a region move its anchors and never re-identify it |

### 4.7 Reset semantics

- **Reset region** (Ctrl+Alt+R): replaces the region's text with its initial content and sets the state back to initial. It is a single undoable edit.
- **Reset file** (Ctrl+Alt+Shift+R): confirms with a count ("Discard 4 resolutions and 2 manual edits?"), then rebuilds the result from the merge plan. It is undoable within the session.
- **Reset file to conflicted (Git)**: shown only after Stage. Uses `git checkout -m -- :(literal)<path>` `[V-GIT git-checkout.adoc]`. Confirmation is required and the command is shown. Git rebuilds the conflict from the index's resolve-undo record, recreating it in the index and worktree, so it is destructive to the saved result. A backup copy is taken first.

### 4.8 Completion gates

| Gate | Requires |
|---|---|
| Save enabled | Always, once loaded. Saving with unresolved or unacknowledged regions → dialog "Saves exactly what RESULT shows; N regions still unresolved" (Rider parity [V-SRC DiffBundle 79-82]) |
| Stage enabled | Saved ∧ no unresolved ∧ no unacknowledged manual regions ∧ (validation clean ∨ user confirms the warnings) |
| Continue enabled | Repo mode ∧ every unmerged path is staged ∧ no index lock ∧ operation state unchanged since load |
| Exit 0 (merge-tool) | Saved ∧ no unresolved ∧ no unacknowledged regions. Validation warnings do not block exit 0 but are printed to stderr |

---

## 5. Conflict support matrix

Detection always starts from `git status --porcelain=v2 -z` plus `git ls-files -u -z` `[V-GIT git-status.adoc, git-ls-files.adoc]`. A missing stage appears as mode `000000` and a zero OID `[E]`.

| Case | Detection | UI | Actions | Limitations | Tests |
|---|---|---|---|---|---|
| Both modified | `u UU`, stages 1+2+3 | 3-pane text | All hunk actions | — | F-UU-* |
| Add/add | `u AA`, stage 1 absent | 3-pane, **no base**; whole file is one region unless a common prefix/suffix splits it (zdiff3-style trim) | All, Both | Every diff is a conflict | F-AA-1..3 |
| Modify/delete | `u UD` (they deleted) / `u DU` (we deleted) | **File-level card**: the surviving side's diff vs base, plus the deleting commit's identity | *Keep file* (→ result = survivor, then save + `git add`) / *Delete file* (`git rm --cached -- :(literal)p` + remove worktree file, with backup) / *Keep with edits* (opens the 2-pane editor survivor↔result) | No 3-way text, by definition | F-UD-1, F-DU-1 |
| Both deleted | `u DD` | File card | *Confirm deletion* (`git rm`) / *Restore from LEFT/RIGHT parent* | rename/rename produces DD+AU+UA triples | F-DD-1 |
| Added by us/them only | `u AU` / `u UA` | File card | Keep / delete | Usually part of a rename/rename | F-AU-1 |
| Rename/modify | ort records it; status shows the new path as UU with stages from both names. `git merge-tree --write-tree -z` messages give `CONFLICT (rename/…)` types `[V-GIT git-merge-tree.adoc]` | 3-pane with "renamed from X" in the header | All | Rename detection is Git's; we display it and never re-derive it | F-RM-1 |
| Rename/rename | Same content to two names: one path AU, another UA, and the source DD | **Grouped card** linking the three paths | Choose name A / B / both; then text merge if contents differ | Grouping relies on merge-tree messages (repo mode) or same-OID heuristics | F-RR-1 |
| Rename/delete | DU/UD at the new path plus a message | File card | Keep renamed / delete | | F-RD-1 |
| File/directory | Stage mode or path collision; Git renames the file to `path~branch` `[U, behaviour from ort docs]` | File card showing both paths | Keep file (rename back) / keep directory | Manual moves needed; we show the commands | F-FD-1 |
| Binary | `merge` attr unset/`binary`, `-text`, or NUL in the first 8000 bytes (Git's heuristic) | File card: size, hash, image preview for png/jpg/gif/svg, hex peek otherwise | Take LEFT / Take RIGHT (`git checkout-index --stage=2\|3 -f -- p`), *Open external tool…*, *Mark resolved as is* | No content merge | F-BIN-1 |
| Symlink | mode 120000 in any stage | File card showing the target strings | Take LEFT/RIGHT target, or edit the target string | Windows without symlink privilege writes a plain file (`core.symlinks=false`); disclosed | F-LNK-1 |
| Executable bit | modes 100644 vs 100755 differ between stages | Mode chip in the header; separate "mode conflict" row even if content merges cleanly | Choose mode | On Windows `core.filemode=false`: the mode lives only in the index, so we stage with `git update-index --chmod` | F-MODE-1 |
| Submodule | mode 160000 (`u UU S…`) `[E]` | Gitlink card: three commit SHAs with subjects (if the submodule is present) and ahead/behind | Take LEFT/RIGHT/BASE SHA → `git update-index --cacheinfo 160000,<sha>,<p>` (what mergetool does `[V-GIT git-mergetool.sh]`) | No recursive resolution in v1 | F-SUB-1 |
| Missing base | AA, or stage 1 unreadable | Banner + no-base mode | as AA | | F-AA-* |
| Multiple merge bases | `git merge-base --all` > 1 (repo mode) | BASE header "virtual base"; nested markers in stage 1 are possible | as UU | We show Git's virtual base verbatim and never compute our own | F-MB-1 |
| Case-only rename | Paths differing only in case; `core.ignorecase=true` | Warning chip; file ops use the index path's exact case | as underlying | Case-insensitive filesystems can hold one worktree file for two index paths; we refuse to write when two index paths case-fold to one worktree path | F-CASE-1 (on CI macOS and Windows) |
| Sparse checkout | `core.sparseCheckout`; conflicted paths get materialised `[V-GIT git-sparse-checkout.adoc][E]` | Info chip "outside sparse cone" | as underlying; after Continue we suggest `git sparse-checkout reapply` (shown, never run) | | F-SPARSE-1 |
| Linked worktree | `rev-parse --git-dir` ≠ `--git-common-dir`; operation files via `--git-path` `[E]` | Header shows "worktree: <name>" | all | Never read `.git/` paths directly | F-WT-1 |
| LFS pointer | `filter=lfs` attr; stage blobs match the pointer grammar (`version https://git-lfs.github.com/spec/v1`, `oid sha256:`, `size`) `[V git-lfs spec.md]` | File card: two LFS objects (size, oid, preview if `git lfs smudge` succeeds) | Take LEFT/RIGHT pointer (stage the pointer blob via `update-index --cacheinfo`), external tool | **Without the LFS merge driver Git text-merges the pointers and leaves markers inside a pointer file `[E]`.** Detected and labelled "corrupted pointer — do not stage as text" | F-LFS-1,2 |
| Custom merge driver | `git check-attr merge -- p` → driver name; `merge.<d>.driver` in config `[V-GIT gitattributes.adoc]` | Info chip "Git ran driver *x*" | Normal (the driver's output is the worktree state; our plan still comes from the stages) | We never invoke drivers ourselves | F-DRV-1 |
| `merge=union` / `-merge` | check-attr | Chip explains the policy; for `-merge` the worktree holds LEFT | Normal | | F-ATTR-1 |
| `text`/`eol`/`working-tree-encoding` | check-attr | Fidelity panel (§9.1) | Normal | | F-ENC-* |
| rerere | `rerere.enabled` or `rr-cache/` exists `[V-GIT git-rerere.adoc]` | Chip "rerere replayed a resolution" when the worktree differs from a fresh merge with no markers | Normal; never call `rerere forget` implicitly | | F-RR-RE-1 |

**Authoritative input acquisition.**
- Read stages from the index, never by parsing markers. Parsing markers cannot recover base content under the default `merge` style, `rerere` rewrites, or driver output.
- Read blob OIDs with `ls-files -u -z`.
- Read content in **working-tree form**, so filters, EOL conversion and `working-tree-encoding` match what is written back, using one of:
  - `git checkout-index --temp --stage=all -z -- p` (the same mechanism as mergetool `[V-GIT git-mergetool--lib.sh]`)
  - `git cat-file --batch --filters` with `:N:path` objects `[V-GIT git-cat-file.adoc][E]`
- `git show :N:p` returns raw blobs without EOL conversion `[E]`. It is used only for the *Raw* fidelity view.

---

## 6. Architecture and technology decisions

### 6.1 Stack comparison

| Criterion | **A. .NET 10 + Avalonia 12 + AvaloniaEdit** | B. .NET core + Monaco in WebView (Avalonia WebView / Photino) | C. TypeScript: Electron + Monaco |
|---|---|---|---|
| Three-pane editor | Custom: 3 `TextEditor` instances + band canvas. Has `IBackgroundRenderer`, `IVisualLineTransformer`, `AbstractMargin`, line virtualisation `[V AvaloniaEdit src]`. **No diff, no view zones** | Monaco: decorations, glyph margin widgets, view zones, content widgets `[V monaco.d.ts 0.57]`. **No merge editor in the package**; VS Code's lives in the workbench `[V vscode mergeEditor]` | Same as B, and the VS Code merge editor (MIT) is the closest reference but uses internal APIs |
| Custom hunk controls | Margins + adorner layer: full control | Glyph widgets / view zones: good | Same |
| Large files | Rope document + virtualised visual lines; proven by AvalonEdit lineage | Excellent; tokenisation in a worker | Excellent |
| Keyboard / a11y | Avalonia has UIA (Win), AX (mac), AT-SPI (Linux, 12.0+) `[V Avalonia src]`. **AvaloniaEdit has no AutomationPeers `[V]` → must build (spike S3)** | Monaco has a mature screen-reader mode; WebView a11y bridging varies by OS `[U]` | Best (Chromium a11y + Monaco) |
| Packaging | Single-file self-contained per RID; ~70–100 MB `[U, measure]` | Plus a WebView runtime: WebView2 (Win), WKWebView (mac), WebKitGTK/WPE (Linux) `[V Photino; Avalonia WebView Linux engine U]` | Electron ~150 MB+ `[U]` |
| Git integration | Same CLI runner in all options | Same | Node `child_process` (fine) |
| Memory / startup | Native; expect < 150 MB / < 1 s `[U, benchmark]` | + WebView process | Highest `[U]`; no authoritative figures found |
| Licensing | MIT (Avalonia, AvaloniaEdit), MIT/BSD TextMateSharp | MIT (Monaco), MIT (Avalonia WebView ≥ 11.4) | MIT |
| Testing | Headless core + Avalonia.Headless UI tests | Core same; UI needs WebDriver/Playwright through the bridge | Playwright for Electron (strong) |
| Fit with you | **Highest** | Two languages + a bridge | TS-only; the .NET core is lost |
| Risk | Editor depth (IME, a11y) is on us | Bridge latency and two runtimes; Linux WebKitGTK quirks | Footprint; core outside your strength |

**Recommendation: A**, with B as the pre-wired fallback (ADR 0004). The decisive factor is the **band alignment model**. Rider/Meld do not insert spacer lines, so the editor only needs y↔line mapping, background layers and a margin, all of which AvaloniaEdit has. VS Code's spacer-line model would need view zones, which favours Monaco. Choosing the band model removes Monaco's main advantage.

**Explicit verification (not assumed):**
- Neither Monaco nor `@codemirror/merge` ships a 3-way merge editor. Monaco's DiffEditor is `{original, modified}` only, and CodeMirror's `MergeView` is two panes plus a unified view `[V package typings]`.
- Every option therefore needs custom three-way orchestration. The work is comparable; the question is which host makes the custom work cheapest and keeps the core in .NET.

### 6.2 Git: CLI vs library (ADR 0005)

| Need | git CLI | LibGit2Sharp 0.32 / libgit2 | Verdict |
|---|---|---|---|
| Unmerged stages | `ls-files -u -z`, porcelain v2 | `Index.Conflicts` | Either |
| Working-tree-form content (filters, eol, encoding) | `checkout-index --temp`, `cat-file --filters` | Partial filter support, no `working-tree-encoding` parity `[U]` | **CLI** |
| Operation state (ort, rebase backends, AUTO_MERGE) | Native | Lags behind (no ort) `[U]` | **CLI** |
| Merge-file oracle | `git merge-file -p --zdiff3`, `--object-id` (2.43+) `[V-GIT]` | libgit2 `git_merge_file`, but **LibGit2Sharp has no binding `[V]`** | CLI |
| Staging semantics (clean filters, `--chmod`, cacheinfo) | Native | Divergence risk | **CLI** |
| User config (safe.directory, includeIf, credential helpers, fsmonitor) | Native | Partial | **CLI** |
| Licence | GPLv2 executable, invoked as a process: no linking concern | libgit2 GPLv2 + linking exception `[V]` | — |

The CLI is authoritative. A `GitRunner` enforces these rules:
- `ProcessStartInfo.ArgumentList` only, with no shell.
- `-c core.quotepath=off`, plus `-z` wherever available.
- `--` before paths, and `:(literal)` pathspecs (or `GIT_LITERAL_PATHSPECS=1`).
- `GIT_TERMINAL_PROMPT=0`, plus `GIT_OPTIONAL_LOCKS=0` for read-only status calls so refreshes never take `index.lock`.
- Stdout/stderr captured with byte-exact decoding.
- Cancellation kills the process tree.
- Minimum supported Git: **2.38** (merge-tree `--write-tree`). Features gated by version: `merge-file --object-id` (2.43), `--diff-algorithm` (2.44), `zdiff3` (2.35) `[V-GIT RelNotes]`.

### 6.3 Component diagram

```
          ┌──────────────── Resolve.App (Avalonia) ─────────────────┐
CLI ─────►│ Resolve.Cli  (System.CommandLine)  ──► ShellViewModel   │
          │   ▲ exit codes                         │                 │
          │   │                 ┌──────────────────┴─────────────┐   │
          │   │                 │ Workspace VMs (files, panes,   │   │
          │   │                 │ bands, ruler, inspector)       │   │
          │   │                 └──────┬─────────────▲───────────┘   │
          │   │       IEditorSurface ◄─┘             │ observes      │
          │   │  (AvaloniaEditSurface | MonacoSurface)               │
          └───┼──────────────────────────────────────┼───────────────┘
              │                                      │
┌─────────────┴────────── Resolve.Core (netstandard-free, net10.0, no UI refs) ───────────────────┐
│ Discovery ─► OperationContext   InputAcquisition ─► ConflictInputs (bytes + attrs + modes)    │
│        │                                  │                                                     │
│        ▼                                  ▼                                                     │
│   GitRunner (process) ◄──── Staging/Continuation     Decoding/Fidelity ─► TextInputs           │
│                                                         │                                       │
│                                    Diff (histogram/myers) ─► Diff3Planner ─► MergePlan (immutable)│
│                                                         │                                       │
│                         ResolutionDocument (rope + anchors + region states + undo log)          │
│                                                         │                                       │
│        Validation (markers, encoding round-trip, JSON/XML/YAML, Roslyn C# parse)               │
│        Persistence (atomic writer, backups, session journal)   Assist (optional, Phase 5)      │
└─────────────────────────────────────────────────────────────────────────────────────────────────┘
```

### 6.4 Domain model (C#)

```csharp
public enum Stage : byte { Base = 1, Left = 2, Right = 3 }

public sealed record StageEntry(Stage Stage, GitMode Mode, ObjectId Oid);   // Mode 000000 => absent

public enum UnmergedKind { BothModified, BothAdded, DeletedByUs, DeletedByThem, AddedByUs, AddedByThem, BothDeleted }

public sealed record UnmergedPath(RepoPath Path, UnmergedKind Kind,
    StageEntry? Base, StageEntry? Left, StageEntry? Right, PathKind PathKind /* Text|Binary|Symlink|Gitlink|LfsPointer */);

public enum OperationKind { None, Merge, RebaseMerge, RebaseApply, Am, CherryPick, Revert, StashInferred, CheckoutMergeInferred, Unknown }

public sealed record SourceIdentity(string Role, string? RefName, ObjectId? Commit, string? Subject, string? Author, RepoPath Path, RepoPath? RenamedFrom);

public sealed record OperationContext(RepoLocation Repo, OperationKind Kind, SourceIdentity LeftIdentity,
    SourceIdentity RightIdentity, OperationFingerprint Fingerprint /* hashes of *_HEAD, stopped-sha, msgnum */);

public sealed record TextInput(Stage Stage, byte[] Bytes, TextEncoding Encoding, EolProfile Eol,
    bool FinalNewline, IReadOnlyList<Line> Lines /* each with its own terminator */);

public enum RegionKind { Equal, LeftOnly, RightOnly, BothSame, Conflict }

public sealed record PlanRegion(int Id, RegionKind Kind, LineRange Base, LineRange Left, LineRange Right,
    ConflictRefinement? Refinement /* zdiff3 common prefix/suffix */);

public sealed record MergePlan(ImmutableArray<PlanRegion> Regions, PlanOptions Options, PlanDigest Digest);

public enum ResolutionChoice { Unresolved, Base, Left, Right, LeftThenRight, RightThenLeft, Manual }

public sealed record RegionState(int RegionId, ResolutionChoice Choice, bool Acknowledged, ContentHash ExpectedText);

public enum SaveState { Clean, Dirty, Saving, SaveFailed, ExternallyModified }
public enum ValidationState { NotRun, Running, Clean, Warnings, Errors }
public enum IndexState { Unmerged, StagedResolved, StagedOther /* index changed outside */ }
```

### 6.5 Key interfaces

```csharp
public interface IGitRunner {
    Task<GitResult> RunAsync(GitInvocation invocation, CancellationToken ct);   // argv, env, stdin bytes, timeout
}

public interface IOperationDetector { Task<OperationContext> DetectAsync(RepoLocation repo, CancellationToken ct); }

public interface IConflictSource {
    Task<IReadOnlyList<UnmergedPath>> ListAsync(RepoLocation repo, CancellationToken ct);
    Task<ConflictInputs> AcquireAsync(RepoLocation repo, UnmergedPath path, CancellationToken ct);   // working-tree form
}

public interface IMergePlanner { MergePlan Plan(TextInput? @base, TextInput left, TextInput right, PlanOptions options); } // pure

public interface IResolutionDocument {
    string Text { get; }                                   // rope-backed snapshot
    IReadOnlyList<RegionView> Regions { get; }              // id, current TextRange, state
    void Apply(ResolutionCommand command);                 // Accept/Both/Ignore/Reset/Ack/ApplyNonConflicting(side)
    void Edit(TextChange change);                          // manual edit; anchors + states update
    bool Undo(); bool Redo();
    IObservable<DocumentChanged> Changes { get; }
}

public interface IResultWriter {
    Task<SaveOutcome> SaveAsync(SaveRequest request, CancellationToken ct);   // checks fingerprint, backs up, atomic replace
}

public interface IStager {
    Task<StageOutcome> StageAsync(RepoLocation repo, StageRequest request, CancellationToken ct);       // add/rm/cacheinfo/chmod
    Task<ContinueOutcome> ContinueAsync(RepoLocation repo, OperationContext op, CancellationToken ct); // explicit only
}

public interface IValidator { Task<ValidationReport> ValidateAsync(ValidationInput input, CancellationToken ct); }

public interface ISessionStore {
    Task<SessionSnapshot?> TryLoadAsync(SessionKey key, CancellationToken ct);
    Task CheckpointAsync(SessionSnapshot snapshot, CancellationToken ct);   // debounced, atomic
}

public interface IEditorSurface {    // UI port; AvaloniaEdit and Monaco adapters
    double LineToY(int line); int YToLine(double y); void ScrollTo(double y);
    void SetDecorations(IReadOnlyList<Decoration> decorations); event Action<TextChange> Edited;
    event Action<double> Scrolled; bool IsReadOnly { get; init; }
}
```

**Error contract.** Core methods return `Result<T, ResolveError>` for expected failures. Exceptions are reserved for bugs.

`ResolveError` cases:
- `GitFailed(exit, stderr)`
- `IndexLocked(path)`
- `StaleSession(expected, actual)`
- `ExternalModification(path, expected, actual)`
- `UnsupportedEncoding(name)`
- `CaseCollision(paths)`
- `IoFailure(kind: DiskFull|PermissionDenied|PathTooLong|Other, path)`
- `Cancelled`

`OperationCanceledException` is translated to `Cancelled` at the boundary. A cancelled save leaves the target untouched (§9.3).

### 6.6 Concurrency model

- **UI thread** owns the `ResolutionDocument`. All mutations are synchronous and fast (≤ 16 ms budget). This is a single-writer design, so the document needs no locks.
- **Background** (`Channel<T>` workers):
  - Git process calls.
  - Planning large files.
  - Intra-line token diffs per visible region.
  - Validation.
  - Highlighting.

  Results carry the `DocumentVersion` they were computed for and are **dropped if stale**. This is the same pattern as GitCthulhu's session-path tagging of background jobs.
- **Cancellation**: every background task takes a `CancellationToken` linked to the file view's lifetime and the document version.
- **File watcher** (`FileSystemWatcher`) on the MERGED path and on `git-path index`, debounced to 250 ms. It is only a hint: the authoritative check is the fingerprint comparison at save time and before staging.
- **Single instance per worktree** (repo mode). A lock file is held in the session directory, `O_EXCL` with PID + start time. A second instance opens read-only with the banner "Another Cthulhu Resolve session owns this worktree".

### 6.7 Persistence format & versioning

```jsonc
// <AppData>/CthulhuResolve/sessions/<sha256(common-dir|worktree)>/<op-fingerprint>/<sha256(path)>.json
{
  "schema": 1,
  "tool": "cthulhu-resolve/0.3.0",
  "repo": { "commonDir": "...", "worktree": "...", "gitVersion": "2.47.1" },
  "operation": { "kind": "RebaseMerge", "fingerprint": "sha256:…" },
  "path": "QiSL.App.Customer/…/Service.cs",
  "stages": { "1": "oid…", "2": "oid…", "3": "oid…", "modes": [33188, 33188, 33188] },
  "planDigest": "sha256:…",            // detects planner-version drift
  "worktreeAtLoad": { "size": 4711, "mtime": "…", "sha256": "…" },
  "result": { "encoding": "utf-8-bom", "textSha256": "…", "text": "…" },
  "regions": [ { "id": 3, "choice": "Manual", "ack": false, "start": 1203, "end": 1388 } ],
  "undo": { "depth": 200, "ops": [ /* compact edit log */ ] }
}
```

- Writes use temp-file + `fsync` + atomic rename, debounced to 500 ms after the last change.
- **Version policy:**
  - A reader accepts `schema ≤ current` and migrates through a chain of pure functions.
  - `schema > current` opens as read-only "made by a newer version".
  - A `planDigest` mismatch (the planner changed between versions) restores only the result text and marks all regions `Manual, unacknowledged` for review. It never restores stale region mappings.
- Session text is **source code at rest**. It is stored in the user profile with per-user ACLs, never in the repository, and is purged when the operation fingerprint disappears (operation finished) or after 14 days.

### 6.8 Data flow (Git → saved → staged)

```
status -z / ls-files -u -z ──► UnmergedPath
check-attr -z (merge,text,eol,working-tree-encoding,filter,diff,binary) ──► AttributeSet
checkout-index --temp --stage=all -z ──► ConflictInputs(bytes ×3, modes)
        │ Decode (BOM / attr encoding / UTF-8 strict → fallback rules §9.1)
        ▼
TextInputs ──► Diff(base→left), Diff(base→right) ──► Diff3Planner ──► MergePlan (immutable)
        ▼
ResolutionDocument(initial = D4) ◄── session journal (if fingerprint matches)
        │ user actions / edits (undoable)
        ▼
Validation ──► Save: fingerprint check → backup → encode (original enc/BOM/EOL rules) → temp → fsync → replace
        ▼
Stage (explicit): check index.lock → `git add -- :(literal)p` → verify `ls-files -s` shows stage 0 only
        ▼
Continue (explicit, confirm): `git <op> --continue` with GIT_EDITOR policy (§9.4) → re-detect operation
```

---

## 7. State machines

### 7.1 Five orthogonal axes

The file's state is the product $S = C \times P \times V \times I \times O$. Content $C$ = {Unresolved, PartiallyResolved, Resolved}, with manual-unacknowledged counted as unresolved. Persistence $P$ = {Clean, Dirty, Saving, SaveFailed, ExternallyModified}. Validation $V$ = {NotRun, Running, Clean, Warnings, Errors}. Index $I$ = {Unmerged, StagedResolved, StagedOther}. Operation $O$ belongs to the repo: {InProgress(fp), Changed, Finished}.

The UI never collapses these into one badge. "Safe" is the predicate $C{=}\text{Resolved} \wedge P{=}\text{Clean} \wedge V \in \{\text{Clean}\} $, and staging is a separate fact. *Lattice view:* each axis is a finite poset, and completion gates (§4.8) are monotone predicates on the product. A downgrade on any axis (an edit after save sets $P$ to Dirty) re-closes the gates automatically.

### 7.2 Region state machine

```
            ┌──── accept L/R, both L→R/R→L, ignore(base) ────┐
            ▼                                                │
  [Unresolved] ──manual edit inside──► [Manual, unack] ──Ctrl+Enter──► [Manual, ack]
      ▲  │                                   ▲                              │
      │  └────────────── reset ──────────────┼──────────── reset ───────────┘
      │                                      │
 [Resolved(choice)] ──manual edit inside─────┘     (text ≠ ExpectedText ⇒ Manual)
```

- A non-conflicting region starts as `Applied(side)` under D4, or `Pending(side)` under the base-only initial result. It has the same transitions, but `Pending` does not block completion.
- Every transition is an undo entry together with its text edit.
- `ExpectedText` is the hash of the text an action produced. On each edit touching the region the hash is recomputed: if it differs, the region becomes Manual; if an undo makes it match again, the choice is restored.

### 7.3 File session state machine

```
Loading ─► (Stale? ─► StaleReview) ─► Ready ⇄ Editing
Ready/Editing ─save─► Saving ─ok─► Ready(Clean) │ ─conflict(ext. mod)─► ExternalChangeReview │ ─io error─► SaveFailed
Ready(Clean ∧ Resolved ∧ ValidOk) ─stage─► Staging ─ok─► Staged │ ─index.lock─► StageBlocked
Staged ─edit─► Editing (index now differs from worktree; chip "staged version differs")
any ─operation fingerprint changed─► OperationChanged (read-only until reload)
```

### 7.4 Undo model

- History is an operation log of `(TextChange[], RegionTransition[])` pairs, so the document is a deterministic left fold of the log over the initial state.
- Undo pops and applies the inverse of a pair.
- Typing coalesces under the usual rules (same region, < 1 s, no cursor jump).
- Hunk actions never coalesce.
- History survives saves; it is cleared on file reset only if the user confirms "also clear history".

---

## 8. CLI specification and Git configuration

### 8.1 Commands

```text
cthulhu-resolve merge --base <file> --local <file> --remote <file> --merged <file>
                      [--label-base <s>] [--label-local <s>] [--label-remote <s>]
                      [--repo <dir>] [--no-repo-context] [--wait|--no-wait] [--reuse-window]
                      [--initial base|base+nonconflicting] [--readonly-sources]
cthulhu-resolve repo   [--path <worktree>] [--file <path>]...      # repository mode
cthulhu-resolve diff   <left> <right>                              # 2-way viewer (difftool)
cthulhu-resolve --version | --help
```

**Semantics:**
- Paths are taken verbatim; no globbing. A path beginning with `-` must follow `--` or use the `--merged=<file>` form. System.CommandLine supports both.
- `--base` may name an **empty file**. Git passes an empty 0-byte file when stage 1 is absent `[V-GIT git-mergetool--lib.sh][E]`. Empty is ambiguous, so the resolver probes repository context:
  - It uses `--repo`, or discovers the repo from MERGED's directory with `git rev-parse --show-toplevel`, unless `--no-repo-context` is given.
  - Then `git ls-files -u -z -- :(literal)<rel>` decides whether stage 1 exists.
  - With no repo, an empty base is treated as an **absent base**, with a banner and a one-click "treat as empty base".
- Repo context also supplies labels: git's `mergetool.<tool>.cmd` exposes no branch labels `[V]`, so labels come from operation detection (§3.3).
- `--wait` is the default. The process blocks until the window for this file closes.
- `--reuse-window` (Phase 3) connects to a running instance over a per-user named pipe / Unix socket (`$XDG_RUNTIME_DIR` or `%LOCALAPPDATA%`). The CLI process still blocks and relays the exit code, so `git mergetool`'s sequential contract holds.

### 8.2 Exit codes

| Code | Meaning | What git does (with `trustExitCode=true`) |
|---|---|---|
| 0 | MERGED saved, all regions resolved and acknowledged | `git add` the file `[V-GIT git-mergetool.sh]` |
| 1 | User closed with unresolved regions, or chose *Cancel*. MERGED untouched on Cancel, or saved partial on "Exit unresolved" | Treats as failed; restores MERGED from its backup and moves on/asks |
| 2 | Usage error (bad arguments) | failure |
| 3 | Input unreadable / unsupported (binary passed to text mode, decode failure without consent) | failure |
| 4 | I/O failure writing MERGED (disk full, permission) | failure |
| 5 | External modification detected and the user declined to overwrite | failure |
| 130 | Interrupted (SIGINT / console close) | failure |

**Recommended configuration is `trustExitCode = true`.** Without it git compares mtimes and may ask "seems unchanged" `[V-GIT]`. The resolver never returns 0 with unresolved regions. That is the hazard `[E]` showed: with `trustExitCode=true`, a tool that exits 0 lets git stage a file that still has markers.

### 8.3 Ownership and cleanup

- `BASE`/`LOCAL`/`REMOTE` temp files belong to git. The resolver opens them read-only, never writes or deletes them, and holds no handles after load (Windows sharing violations would otherwise break git's cleanup).
- `MERGED` is written only through the atomic writer.
- The resolver's own backups and journal live under AppData (§6.7).
- **Staging is owned by the caller** in merge-tool mode. git mergetool runs `git add` itself on success `[V-GIT]`, so the resolver never stages there, even when repo context is available.

### 8.4 Example Git configuration

```ini
# ~/.gitconfig — Windows paths shown; on WSL use the Linux build, not the .exe, to keep path semantics consistent.
[merge]
    tool = cthulhu
    conflictStyle = zdiff3          # for humans reading markers elsewhere; the resolver reads stages, not markers
[mergetool "cthulhu"]
    cmd = cthulhu-resolve merge --base \"$BASE\" --local \"$LOCAL\" --remote \"$REMOTE\" --merged \"$MERGED\"
    trustExitCode = true
[mergetool]
    keepBackup = false              # the resolver keeps its own backup before first save
    prompt = false
    writeToTemp = true              # temp files out of the worktree (mktemp -d) [V-GIT]
[diff]
    tool = cthulhu
[difftool "cthulhu"]
    cmd = cthulhu-resolve diff \"$LOCAL\" \"$REMOTE\"
```

`$BASE` and the other names are **shell variables, not environment variables**, so they must appear in `cmd` `[V-GIT][E]`. For GitCthulhu, the planned button "Resolve conflicts…" runs `cthulhu-resolve repo --path <session path>` through the composition root.

---

## 9. Safety, recovery and trust model

### 9.1 File fidelity

| Concern | Rule |
|---|---|
| Representation | Inputs stay `byte[]`. Decoded text keeps a **per-line terminator** (`\n`, `\r\n`, `\r`, none-at-EOF). The result's lines carry the terminator of the source line they came from. Lines typed by hand get the file's dominant terminator, defined as the dominant EOL of LEFT, else RIGHT, else the platform default |
| Byte-for-byte guarantee | A region taken from one side re-encodes to exactly that side's bytes. If the result consists only of side-derived regions, the saved bytes equal a deterministic splice of the input bytes (property P1) |
| Mixed EOL | Preserved per line; a banner shows the counts (`LF 812 · CRLF 3`). No normalisation unless the user chooses "Normalise to LF/CRLF" (an explicit, undoable whole-document edit) |
| Final newline | Tracked per side. If sides disagree and the last region is conflicted, it is part of the conflict; otherwise it follows the chosen side |
| Encoding detection | 1) BOM (UTF-8/16LE/16BE/32). 2) `working-tree-encoding` attribute. 3) Strict UTF-8 decode. 4) On failure: **refuse text mode** and offer "Open as Windows-1252 / Latin-1 (lossless single-byte)" or binary handling. Never silently replace with U+FFFD; round-trip is verified before the editor opens |
| BOM | Saved iff LEFT has one. If LEFT and RIGHT differ, the result follows the side chosen in a one-time file-level prompt |
| Tabs, trailing whitespace | Never modified. Ignore-whitespace is a **display/diff option only** (§10.6) |
| `.gitattributes` / filters | Inputs are in working-tree form (smudge and eol applied by Git). The saved file is in working-tree form. `git add` reapplies clean/eol. The fidelity panel discloses `text=auto`, `eol=crlf`, `filter=x` and the effect: *"Git will convert CRLF→LF when staging; the saved worktree file keeps CRLF"* |
| Paths | `RepoPath` type (repo-relative, `/`-separated, NFC-preserving: **no Unicode normalisation**, so macOS NFD names round-trip). Filesystem paths built with `Path.Join`; Windows long paths enabled (`\\?\` when > 260). Leading `-` safe via `--` + `:(literal)` |
| Permissions | The atomic writer copies mode bits (Unix) and ACL/attributes (Windows `File.Replace`). Symlink targets are never followed for writing: if MERGED is a symlink the save refuses (symlink conflicts are file-level) |

### 9.2 Concurrency and external change

- **Fingerprint** at load: (size, mtime, SHA-256) of MERGED, plus the OIDs of index stages 1–3, plus the operation fingerprint.
- **Before save:** re-hash MERGED. On a mismatch, open *External change review*: a 3-way between the original at load, the current disk version, and our result. The options are *Merge disk changes into result*, *Overwrite (confirm)*, and *Save as…*.
- **Before stage:** re-read `ls-files -u` for the path. If the stages changed, the session is stale (someone ran `git checkout --ours` or similar). Refuse, and offer reload.
- **Index lock:** `git-path index.lock` present → *StageBlocked* with the message "Another Git process is running, or a stale lock exists". We **never delete the lock**. We show its age and the command to remove it, which the user runs manually.
- **Second instance:** the per-worktree lock (§6.6).
- **IDE autosave/formatters:** detected by the watcher and handled by the fingerprint check. We never fight a watcher: if the file changes while the result is clean, a banner offers "Reload from disk".

### 9.3 Crash, interruption, disk failures

- **Atomic save:**
  1. Write `<dir>/.<name>.cthulhu-<rand>.tmp` with `FileMode.CreateNew`.
  2. Flush to disk (`fsync`; `FlushFileBuffers` on Windows).
  3. Replace: `File.Replace` on Windows, `rename(2)` on POSIX.
  4. `fsync` the directory (POSIX).

  A crash at any step leaves either the old file or the new one, never a torn file. Orphaned temp files matching the pattern are cleaned on the next start (only our own pattern, only older than 1 h).
- **Backups:** the first save per session copies the pre-resolution working-tree file to `<AppData>/backups/<repo>/<op-fp>/<path>.<ts>`. Retention is 14 days or 500 MB, whichever comes first, oldest first. Backups are listed in Inspector → History and can be restored.
- **Disk full / permission denied:** the save fails before replace, the target stays untouched, the state goes to `SaveFailed` with the reason, and the in-memory result plus journal are retained.
- **Crash recovery:** on start, sessions whose fingerprint still matches a live operation are offered for restore. Restore requires matching stage OIDs. A mismatch → "stale: view read-only / discard".

### 9.4 Trust boundaries

| Boundary | Policy |
|---|---|
| Repository content (file text, commit messages, branch names, `.gitattributes`) | **Untrusted data.** Rendered as text, never executed or interpreted as instructions (relevant for AI, §11). Branch names and paths are escaped in every UI string and never interpolated into a command line |
| Git config (filters, merge drivers, fsmonitor, hooks path) | User-trusted. Note that `git add` / `checkout-index` **execute configured clean/smudge filters**. Disclosed in the fidelity panel ("Staging will run filter `lfs`") |
| Hooks | `git add` runs none. `--continue` may run `pre-commit`, `commit-msg`, `post-rewrite` and similar hooks, and opens `GIT_EDITOR`. The Continue confirmation lists these. Default `GIT_EDITOR` is the resolver's message dialog (an `--editor-shim` sub-command) rather than a terminal editor |
| Process invocation | argv only, absolute path to `git` resolved once and shown in Settings, no shell, `safe.directory` errors surfaced verbatim (never auto-added) |
| Network | **None in the core.** No telemetry, no update check without opt-in. AI assist is the only egress, opt-in per provider (§11) |
| Destructive Git | `--abort`, `checkout -m`, `rm` behind explicit confirmations that display the exact command. No `reset`, `clean` or `stash drop`, ever |

### 9.5 Silently never

The resolver never silently stages, commits/continues, aborts/resets/cleans, overwrites externally modified files, runs hooks or scripts, or sends source off-machine. Each of these needs the explicit action in the table above.

### 9.6 Intentional differences (Backend ↔ ISL-Backend) — Phase 5

- **Record:** for the region under the cursor, "Mark as intentional difference: keep LEFT/RIGHT". The record holds the path, a region fingerprint (SHA-256 of normalised base/left/right texts), a note, the author and the date.
- **Storage:** user-level by default; an opt-in repo file `.cthulhu/intentional.json` can be shared with the team.
- **Matching:** exact on (path, fingerprint) only, with **no generalisation** to other paths or similar text. A match shows a hint chip and the recorded choice as a *preview*. It is applied only by an explicit "Apply recorded decision".
- **Relationship to `git rerere`:** rerere replays whole-conflict resolutions keyed by conflict text. Records add the *why* and are never auto-applied.

---

## 10. Diff and merge algorithms

### 10.1 Two-way diffs

- Compute `base→left` and `base→right` on line arrays using **histogram diff** (Git's `diff.algorithm=histogram`, a patience extension `[V-GIT config/diff.adoc]`), with **Myers O(ND)** (Myers 1986) as the fallback for pathological inputs.
- Line equality is on decoded text including the terminator. That gives the *exact* mode; display modes compare normalised keys instead (§10.6).

### 10.2 diff3 planning

- Walk both edit scripts over base coordinates (the classic diff3 merge; formalised by Khanna, Kunal & Pierce, *A Formal Investigation of Diff3*, FSTTCS 2007).
- Partition base into maximal *stable* runs (unchanged by both) and *unstable* chunks between them.
- Classify each unstable chunk:
  - `LeftOnly` if right is unchanged there, and `RightOnly` symmetrically.
  - `BothSame` if left and right texts are equal (a "false conflict").
  - Otherwise `Conflict`.
- **Touching rule:** edits whose base ranges overlap, **or** zero-length insertions at the same base position, **or** adjacent edits with no stable line between them, are one chunk. This matches Git's conservative behaviour.
- **Refinement:** for `Conflict`, strip the longest common prefix/suffix *lines* of left and right into `BothSame` sub-regions (zdiff3-style `[V-GIT config/merge.adoc]`). "Accept both" then duplicates only the true core.
- Khanna et al. show that diff3 is not idempotent and not "stable" under small perturbations. That is why the plan is computed **once** from the immutable inputs and frozen (ADR 0006). Re-planning on every edit would make region identity depend on unstable heuristics.

### 10.3 Oracle and differential testing

- `git merge-file -p --zdiff3 -L … left base right` (or `--object-id` on 2.43+) is the oracle `[V-GIT git-merge-file.adoc]`.
- For each fixture and property-based case, our plan rendered with markers must match Git's **conflict count**, and the union of non-conflicting output must match exactly.
- Divergences are allowed only in a documented list (e.g. Git's `XDL_MERGE_ZEALOUS` refinements). Each allowed divergence needs a test that names it.

### 10.4 Range mapping and anchors

- The result is a rope. Each plan region gets two **anchors**: a start anchor that *sticks left* and an end anchor that *sticks right*.
- On a text change at range $[a,b)$ with replacement length $\ell$, each anchor offset $x$ maps by $x \mapsto x$ if $x < a$, $x + \ell - (b-a)$ if $x \ge b$, and to $a$ or $a+\ell$ by stickiness if $x \in [a,b)$. This is the standard monotone transform used in OT.
- An edit straddling two regions marks **both** as Manual (unacknowledged). Their anchors keep their relative order because the map is monotone, so regions never swap.
- Region identity is `PlanRegion.Id` and never moves.

### 10.5 Scroll synchronisation as a monotone map

- Each adjacent pane pair gets a piecewise-linear, monotone, surjective map $\varphi:[0,H_1]\to[0,H_2]$ with breakpoints at region boundaries; it is recomputed from anchors after each edit.
- The master pane drives $y_2=\varphi(y_1)$. Composition gives LEFT→RESULT→RIGHT. Monotonicity guarantees no scroll inversion.
- Bands are the images of each region's interval under the pair map.

### 10.6 Whitespace modes (display only)

- Modes: `Exact` (default), `TrimEol`, `IgnoreAll` (collapse runs), `IgnoreAllAndBlankLines`.
- The mode changes the **equality key** used to *display* and *align* diffs, plus a "differs only in whitespace" badge. It **never** changes the plan, which is always computed in `Exact`. Content actions copy exact bytes.
- Rider requires a restart to change highlighting mid-merge [V-SRC]. We avoid that because display diffs are separate from the plan.

### 10.7 Word level, repeats, moves, large files

| Topic | Approach |
|---|---|
| Word diff | Tokenise (identifier/number/punctuation/whitespace run) inside a changed region only; Myers on tokens; 20 ms budget per region, falling back to line-only highlighting with an "approximate" badge |
| Repeated text / ambiguous alignment | Histogram prefers low-occurrence anchors. Ties break deterministically (earliest match), so the same inputs give the same plan on every OS (property P4) |
| Moved blocks | v1: none (shown as delete + insert). Phase 4: display-only move detection (hash of ≥ 3-line blocks), drawn as dashed bands, never altering the plan |
| Large files | ≥ 50k lines: planning on a worker with a progress bar; highlighting viewport-only. ≥ 2 MB single line (minified): token diff disabled, line diff only, with a banner. ≥ 50 MB: refuse text mode → file-level handling (take side / external tool) |
| Incremental recompute | The plan is immutable, so there is no recompute. Display diffs (result vs base/left/right) are recomputed per region on edit, bounded by region size |
| Cancellation | Every background computation is versioned and cancellable (§6.6) |

### 10.8 Libraries to evaluate

| Library | Licence | Use | Custom work |
|---|---|---|---|
| DiffPlex 1.9 | Apache-2.0 `[V]` | Myers/line+word chunkers; has a 3-way API since 1.9 | Histogram, diff3 refinement, anchors: ours. Evaluate as the Myers fallback only |
| Own `Histogram` port | — | Primary line diff | ~400 LOC, verified against `git diff --histogram` |
| AvaloniaEdit `TextDocument` + `TextAnchor` | MIT | Rope + anchors with movement types | Region model on top |
| TextMateSharp | MIT `[U licence check]` | Grammars | — |
| Microsoft.CodeAnalysis.CSharp | MIT | Parse-only C# validation | — |
| YamlDotNet / System.Text.Json / XmlReader | MIT | Parse validation | — |
| FsCheck / CsCheck | BSD/MIT | Property tests | — |

### 10.9 Performance targets (proposed — validate in spike S1/S2)

| Metric | Target | Measured on |
|---|---|---|
| Cold start → interactive (merge-tool, 2k lines) | ≤ 800 ms | Win11 i7, macOS M2, Ubuntu VM |
| Plan 10k lines / 3 conflicts | ≤ 50 ms | |
| Plan 100k lines | ≤ 600 ms (worker) | |
| Hunk action → repaint | ≤ 16 ms p95 | 50k lines |
| Keystroke → repaint in result | ≤ 16 ms p95 | 50k lines, 3 panes, highlighting on |
| Scroll at 60 fps with bands | ≥ 55 fps p50 | 100k lines |
| Memory | ≤ 250 MB for 100k-line file | |

Benchmarks use BenchmarkDotNet for the core and a UI frame-time harness (Avalonia render timer hooks) for the panes. Fixtures: real files from Backend / ISL-Backend (kept local, never committed), plus synthetic generators. Results are tracked in CI as JSON artefacts, and a regression of more than 10 % fails the benchmark job.

---

## 11. Optional semantic and AI assistance (Phase 5)

- **Offline core is complete without it.** Assist is a plugin implementing `IAssistProvider`, disabled by default.
- **Explain conflict:** sends (base, left, right) of one region plus N lines of context, the source identities, and the file path. Returns prose. Display only.
- **Suggest resolution:** returns candidate text for one region. It is shown as a **4th preview pane** with diffs against LEFT, RIGHT and BASE (§4.4 compare set). Accepting it is a normal `Manual` edit (undoable, unacknowledged until the user presses Ctrl+Enter). No silent mutation.
- **Contract/risk flags** (local analysers first, AI second):
  - Roslyn-based checks: enum member value changes, public signature changes, duplicated method bodies after Both, `using` ordering.
  - TS: exported symbol changes via the `typescript` parser run out-of-process, Phase 5b.

  Flags are diagnostics, never auto-fixes.
- **Authorised validation:** user-configured commands (stored in the user profile, **not** in the repository, so a repo cannot plant commands), e.g. `dotnet build <sln> -v q`. They run only on an explicit click, with output in the Inspector. The results are a separate validation axis (Compile/Test), distinct from Syntax.
- **Prompt-injection posture:**
  - Source is wrapped as data with a delimiter and a fixed system instruction.
  - Model output is parsed only as candidate text; tool use is disabled.
  - Suggestions containing conflict markers or changes outside the region are rejected.
- **Consent:**
  - The first use per repository shows the provider, model and the exact bytes to be sent (preview).
  - Each provider is configured explicitly; there is no default provider.
  - Credentials are kept in the OS keychain.
  - A deny-list of path globs (e.g. `**/appsettings*.json`, `**/*.pfx`, `.env*`) is never sent.
- **Honesty:** confidence is never shown as a percentage of correctness. Its label is "model suggestion — unverified". Syntax, compile and tests stay separate signals.

---

## 12. Testing strategy and acceptance criteria

### 12.1 Layers

| Layer | Tooling | Scope |
|---|---|---|
| Unit | xUnit | Histogram, diff3 classification, refinement, anchors, region transitions, undo fold, encoding/EOL codec, path types |
| Property | CsCheck | P1 splice fidelity; P2 `undo^n ∘ do^n = id`; P3 anchors monotone under random edits; P4 determinism (same inputs → same plan digest); P5 choosing LEFT everywhere yields LEFT's bytes exactly (base present, no BothSame trimming in LEFT); P6 plan conflict count = `git merge-file` count (oracle, modulo the allow-list) |
| Integration (real Git) | Fixture builder that scripts real repositories in temp dirs (like the fixture used to verify GitCthulhu's graph in this session) | Every row of §5 × {merge, rebase-merge, rebase-apply, cherry-pick, revert, stash} where applicable; Git versions 2.38, 2.43, latest |
| UI | Avalonia.Headless + xUnit; keyboard-only scripts | Hunk workflows, focus order, accessible names, navigation keys |
| Cross-platform FS/encoding | CI matrix: windows-latest, macos-latest (case-insensitive APFS), ubuntu-latest (+ a WSL job on a Windows runner) | CRLF/LF/mixed, BOM, UTF-16 + `working-tree-encoding`, NFD names, long paths, leading `-`, spaces, symlinks, exec bit |
| Recovery | Fault-injecting `IFileSystem` (fail at each atomic-save step), process kill between journal writes | |
| Concurrency | Simulated external writer during save; `index.lock` present; second instance | |
| Performance | BenchmarkDotNet + frame-time harness | §10.9 |

### 12.2 Acceptance criteria (Given / When / Then)

1. **Mixed conflicting and non-conflicting.**
   - **Given** a UU file where LEFT changed lines 3 and 20, RIGHT changed lines 20 and 40, and base exists,
   - **When** it opens with the default initial result,
   - **Then** lines 3 and 40 are applied and tinted; line 20 is one unresolved conflict; the counter reads "1 conflict · 2 changes"; F8 jumps to line 20; Save is enabled; Stage is disabled.
2. **Manual edit then hunk action.**
   - **Given** conflict C1 is unresolved and the user types inside it,
   - **When** they press Ctrl+Alt+← (accept LEFT) on C1,
   - **Then** a confirmation "Replace your manual edit in this region?" appears. On confirm, C1 = LEFT and the state is Resolved(Left). Ctrl+Z restores the manual text **and** the Manual state in one step.
   - **And when** "Apply all non-conflicting" runs instead, C1 and any other Manual region are skipped and listed as "skipped (manual)".
3. **Rebase labels.**
   - **Given** `git rebase origin/development` stops on commit d9c2046 from `feature/23542`,
   - **When** the resolver opens the file,
   - **Then** the LEFT header role reads "Upstream / onto + already-rebased" with the ref `origin/development`, and RIGHT reads "Your commit being replayed · d9c2046 · chore(fork)…". The banner explains Git's ours/theirs swap. No button reads "ours" or "theirs".
4. **Modify/delete.**
   - **Given** a `UD` path,
   - **When** opened,
   - **Then** a file card (not the text editor) shows the survivor's diff and the deleting commit. *Delete file* runs `git rm --cached -- :(literal)<p>` and removes the worktree file only after taking a backup. *Keep file* saves the survivor and enables Stage.
5. **Cancellation.**
   - **Given** merge-tool mode with edits made,
   - **When** the user chooses Cancel,
   - **Then** MERGED's bytes and mtime are unchanged, the exit code is 1, and no files are left in the worktree.
6. **Crash recovery.**
   - **Given** two resolved regions and one manual unacknowledged region, and the process is killed,
   - **When** the resolver restarts on the same operation (same stage OIDs),
   - **Then** it offers restore. After restore the text, region states and acknowledgement flags are identical to the journal.
   - **And given** the stage OIDs differ, **then** restore is refused as stale and the journal is viewable read-only.
7. **External modification.**
   - **Given** the file was loaded and an IDE writes MERGED,
   - **When** the user saves,
   - **Then** the save is blocked and *External change review* opens. Nothing is written until the user chooses merge, overwrite (confirmed) or save as.
8. **`git mergetool`.**
   - **Given** the §8.4 config and a UU conflict,
   - **When** `git mergetool` launches the resolver and the user resolves and saves,
   - **Then** the exit code is 0, git stages the file (`ls-files -s` shows stage 0), `$BASE`/`$LOCAL`/`$REMOTE` are untouched by us, and git removes them.
9. **Save without staging.**
   - **Given** repo mode with a fully resolved file,
   - **When** the user presses Save,
   - **Then** the worktree file is written, `git ls-files -u` still lists stages 1–3 for the path, and the row shows ● saved, ◇ not staged.
10. **Stage without continuing.**
    - **Given** every unmerged path is staged,
    - **When** the user presses Stage on the last file,
    - **Then** `MERGE_HEAD`/`REBASE_HEAD` still exists, no commit was created, and *Continue…* becomes enabled but has not been invoked.
11. **Markers are not resolution** `[P]`.
    - **Given** the user deletes the conflict region's text manually, leaving a marker-free file,
    - **Then** the region is Manual and unacknowledged, and Stage stays disabled until Ctrl+Enter.
    - **And given** the user pastes `<<<<<<< x`-style lines, **then** validation shows "conflict-marker-like line at N".
12. **LFS pointer corruption.**
    - **Given** an LFS-tracked path without the LFS merge driver,
    - **When** opened,
    - **Then** a "corrupted pointer" file card appears and text staging is unavailable.

### 12.3 Mechanical vs judgement

| Mechanically testable | Requires user judgement |
|---|---|
| Inputs equal Git's stages (bytes, modes); fidelity properties P1–P6; exit codes; no writes on cancel; atomicity; staging and operation state; label mapping per operation; no network without consent | Whether a resolution is semantically correct; whether "both" ordering is right; whether a difference is intentional; whether an AI suggestion is acceptable; whether validation warnings may be accepted |

---

## 13. Phased implementation backlog

Relative effort assumes one senior engineer and is given as S/M/L/XL (≈ 1 / 2–3 / 4–6 / 8+ weeks).

### Phase 0 — Foundation & spikes (M)
- **Scope:**
  - Repo, CI matrix, `GitRunner` with argv/`-z`/literal pathspecs.
  - Fixture builder.
  - **S1** three AvaloniaEdit panes + bands + sync scroll at 100k lines.
  - **S2** histogram + diff3 planner vs the `git merge-file` oracle.
  - **S3** AvaloniaEdit accessibility peer (UIA text pattern on Windows, AX on macOS).
  - **S4** atomic replace semantics on NTFS/APFS/ext4 + WSL `/mnt/c` (9P), with ACL/mode preservation.
- **Excluded:** any product UI beyond the spikes.
- **Risks:** S1/S3 fail → fallback B. S4 on `/mnt/c` may not support atomic rename semantics `[U]` → document "run the Windows build for Windows paths".
- **Exit:** spike reports with numbers against §10.9, and the S1/S3 go/no-go.

### Phase 1 — First vertical slice (L)
- **Scope:** merge-tool mode, UU text files only. Real stage acquisition through repo context, plan, three panes, accept L/R/both/ignore, manual edit + ack, undo, atomic save + backup, exit codes 0/1/2/3/4. UTF-8/BOM/CRLF fidelity.
- **Excluded:** repo mode, file-level cases, base pane, journal, validation beyond the marker scan.
- **Dependencies:** Phase 0.
- **Tests:** acceptance criteria 1, 2, 5 and 8; P1–P5; Windows + Linux CI.
- **Exit:** `git mergetool` on the ISL-Backend-style fixture resolves end-to-end with byte-exact output.

### Phase 2 — Reliable initial release (XL)
- **Scope:**
  - Repo mode: discovery, operation detection and the label table, file list, Save/Stage/Continue gates, external-change review, session journal + crash recovery.
  - File-level cards (UD/DU/AA/DD/binary/symlink/mode), base pane + compare set.
  - Validation (markers, encoding round-trip, JSON/XML/YAML, C# parse).
  - Responsive layouts and the keyboard map, accessibility pass.
  - macOS build.
- **Excluded:** submodules, LFS actions (detection only), rename grouping, reuse-window.
- **Tests:** acceptance criteria 3, 4, 6, 7, 9, 10 and 11; recovery fault injection; Git 2.38/2.43/latest.
- **Exit:** a dogfooding week on real Backend ↔ ISL-Backend merges with zero data-loss incidents and an empty P1 bug list.

### Phase 3 — Advanced conflict support (L)
- **Scope:** rename/rename grouping via `merge-tree -z` messages; submodule gitlink cards; LFS cards (+ acceptance criterion 12); case-collision guard; sparse/worktree chips; "resolve simple conflicts" (line-internal token merge with preview); `--reuse-window`; GitCthulhu "Resolve conflicts…" integration.
- **Tests:** F-RR, F-SUB, F-LFS, F-CASE on macOS/Windows.

### Phase 4 — Display intelligence (M)
- **Scope:** moved-block display, whitespace-only badges, import-statement assist (C# `using` / TS `import`), large-file degradations polished.

### Phase 5 — Semantic & AI (L)
- **Scope:** `IAssistProvider`, explain/suggest preview pane, Roslyn risk flags, intentional-difference records, authorised validation commands.
- **Exit:** every acceptance criterion still passes with assist disabled, and an injection test suite (malicious comments in sources) passes.

---

## 14. Open decisions and technical spikes

| # | Question | Default | Consequence if wrong | How to validate |
|---|---|---|---|---|
| O1 | AvaloniaEdit accessibility depth | Build a UIA/AX peer ourselves | +3–4 weeks or fallback to Monaco | Spike S3 with NVDA + VoiceOver scripts |
| O2 | Initial result default (D4) | Base + non-conflicting applied | Users may expect Rider's base-only | Setting exists; dogfood survey |
| O3 | Pane order for rebase | Fixed stage order (2 left, 3 right) with role labels | Some users want "my code left" | Optional display swap in Phase 3 |
| O4 | Where the code lives (D6) | Separate `cthulhu-resolve` repo | Mixed-stack monorepo friction if merged | Revisit after Phase 1 |
| O5 | WSL handling | Linux build inside WSL; Windows build for Windows paths; never cross the boundary in one session | Users invoking the `.exe` from WSL git get `\\wsl$` paths | Detect `/mnt/<drive>` and `\\wsl$` and warn; S4 |
| O6 | Minimum Git | 2.38 | Older distros (e.g. RHEL 8 ships 2.31 `[U]`) | Version probe with a clear error |
| O7 | Diff library | Own histogram; DiffPlex for Myers fallback | Maintenance of ~1k LOC | S2 results vs the oracle |
| O8 | Session text at rest | Plain in user profile with ACL | Sensitive repos | Optional DPAPI/Keychain-wrapped key, Phase 3 |
| O9 | Monaco fallback host | `Avalonia.Controls.WebView` (MIT ≥ 11.4) | Linux engine is unverified | Only matters if S1/S3 fail |

---

## Appendix A — Sources

- Rider docs: https://www.jetbrains.com/help/rider/Resolve_conflicts.html · https://www.jetbrains.com/help/idea/resolve-conflicts.html · https://www.jetbrains.com/help/rider/Differences_Viewer.html · https://www.jetbrains.com/help/rider/Settings_Tools_Diff_and_Merge.html · https://www.jetbrains.com/help/ai-assistant/ai-in-vcs-integration.html (via search summaries — re-verify wording)
- IntelliJ platform strings: https://github.com/JetBrains/intellij-community/blob/master/platform/vcs-api/vcs-api-core/resources/messages/VcsBundle.properties · …/platform/diff-api/resources/messages/DiffBundle.properties · …/plugins/git4idea/shared/resources/messages/GitBundle.properties · …/platform/platform-resources-en/src/messages/ActionsBundle.properties · …/platform/platform-resources/src/keymaps/$default.xml
- Git: https://github.com/git/git/blob/master/Documentation/ — `git-mergetool.adoc`, `config/mergetool.adoc`, `git-merge.adoc`, `git-ls-files.adoc`, `git-status.adoc`, `git-checkout-index.adoc`, `git-cat-file.adoc`, `git-rebase.adoc`, `git-checkout.adoc`, `git-merge-file.adoc`, `git-merge-tree.adoc`, `config/merge.adoc`, `gitattributes.adoc`, `git-check-attr.adoc`, `git-sparse-checkout.adoc`, `git-update-index.adoc`, `git-rerere.adoc`, `config/diff.adoc`, `revisions.adoc`, `RelNotes/2.34–2.44`; scripts `git-mergetool.sh`, `git-mergetool--lib.sh`; `lockfile.h`
- Git LFS: https://github.com/git-lfs/git-lfs/blob/main/docs/spec.md · …/docs/man/git-lfs-merge-driver.adoc
- Editors: monaco-editor 0.57.0 typings; https://github.com/microsoft/vscode/tree/main/src/vs/workbench/contrib/mergeEditor; @codemirror/merge 6.12.3 typings; https://github.com/AvaloniaUI/AvaloniaEdit; https://github.com/AvaloniaUI/Avalonia; https://github.com/tryphotino/photino.NET; https://github.com/dotnet/maui; iced 0.13.1 `widget/src/text_editor.rs`
- Libraries: DiffPlex 1.9.0, LibGit2Sharp 0.32.0, libgit2 COPYING, imara-diff, similar, diffy, node-diff3 (versions as of 2026-10-27)
- Theory: E. W. Myers, *An O(ND) Difference Algorithm and Its Variations*, Algorithmica 1986 · S. Khanna, K. Kunal, B. C. Pierce, *A Formal Investigation of Diff3*, FSTTCS 2007 · Ellis & Gibbs, *Concurrency Control in Groupware Systems*, SIGMOD 1989 (OT position transforms)
