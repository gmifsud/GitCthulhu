# Cthulhu Resolve Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A `cthulhu-resolve merge …` executable that `git mergetool` can launch on a both-modified (UU) text file: three panes, accept left/right/both/ignore, manual edit with acknowledgement, undo, atomic save with backup, and exit codes git can trust.

**Architecture:** Everything that decides anything lives in `Resolve.Core` and is tested headless: argument parsing, input loading, role labels, planning (git's own diffs), the resolution document (text + anchors + region states + undo), the pane line maps, and the session that saves and chooses the exit code. `Resolve.App` is a thin Avalonia shell: one `IEditorSurface` adapter over AvaloniaEdit per pane, band canvases between panes, a keymap, and the accessible conflict navigator of decision O1-A. The document is the single writer; the editors only display it and report keystrokes.

**Tech Stack:** .NET 10 (SDK 10.0.4xx), C# latest, Avalonia 12.1.4, Avalonia.AvaloniaEdit 12.0.0, xUnit 2 + CsCheck (core), xUnit v3 + Avalonia.Headless.XUnit 12.1.4 (UI), git ≥ 2.38.

**Spec:** `docs/superpowers/specs/2026-10-27-conflict-resolver-design.md` (Phase 1 scope §13; merge-tool mode §3.1A, §4.2; editing §4.6–4.8; model §6.4–6.5; state machines §7.2, §7.4; CLI §8.1–8.4; fidelity §9.1; algorithms §10.2, §10.4–10.5; acceptance §12.2 criteria 1, 2, 5, 8; properties §12.1 P1–P5). S1/S3 findings: `resolver/docs/spikes/S1-S3-editor.md`. Decision O1 = option A (spec §14).

## Global Constraints

- All code under `resolver/`. Build settings come from `resolver/Directory.Build.props` (net10.0, nullable, `TreatWarningsAsErrors`, `InvariantGlobalization`, central package versions in `Directory.Packages.props`).
- `Resolve.Core` references no UI assembly (no Avalonia, no AvaloniaEdit) — spec §6.3.
- Git is invoked only through `IGitRunner` (argv, no shell, `GIT_LITERAL_PATHSPECS=1`); minimum git 2.38 (`GitVersion.Minimum`).
- Merge-tool mode never stages and never runs any git command that writes (spec §8.3). It never writes, deletes or keeps open `$BASE`/`$LOCAL`/`$REMOTE` (read with `File.ReadAllBytes`).
- `MERGED` is written only through `AtomicFileWriter.Save`. Backups go under `%LOCALAPPDATA%/CthulhuResolve/backups` (`Environment.SpecialFolder.LocalApplicationData`), never in the worktree.
- Exit codes (spec §8.2): `0` saved with every conflict resolved and acknowledged; `1` cancelled or exited unresolved; `2` usage error; `3` input unreadable or unsupported; `4` I/O failure writing MERGED; `5` external modification and the user declined to overwrite. Never `0` with an unresolved or unacknowledged conflict.
- Stage 2 is always the LEFT pane and stage 3 the RIGHT pane; buttons and names use role labels (§3.3), never "ours"/"theirs".
- Fidelity (§9.1): inputs stay bytes; each result line keeps the terminator of the line it came from; typed newlines use the dominant EOL of LEFT, else RIGHT, else `"\n"`; BOM follows LEFT unless the user picks RIGHT's encoding; tabs and trailing whitespace are never changed; no U+FFFD.
- Phase 1 excludes: repo mode, file-level cases (anything not UU), the base pane, the session journal, validation beyond the marker scan, `--reuse-window`, `--initial`, `--readonly-sources`, exit code 130.
- Screen-reader editing of RESULT is a documented v1 limitation (O1-A); every action must be reachable from the keyboard and the navigator.

## Review Focus

1. **CRLF and mixed-EOL files:** a file whose lines end in CRLF, edited by typing Enter in RESULT, must save with CRLF on the typed line and every untouched line byte-identical. → Task 5 test `Typed_newline_uses_the_dominant_eol_of_left`, Task 7 test `Crlf_file_round_trips_byte_for_byte_after_a_typed_line`.
2. **No final newline on one side, conflict on the last line:** the result keeps whichever side's ending the user takes. → Task 5 test `Accepting_right_on_a_last_line_without_newline_keeps_no_newline`.
3. **Window closed with the title-bar ✕ after edits:** must behave as Cancel (MERGED untouched, exit 1), after a confirmation, never as a silent save. → Task 11 test `Closing_the_window_after_edits_asks_then_cancels`.
4. **Awkward MERGED paths:** spaces, a leading `-`, non-ASCII names, passed by `git mergetool` as `$MERGED`. → Task 1 test `Leading_dash_paths_parse_after_double_dash_and_with_equals`, Task 13 acceptance run uses `dir with space/-ünïcode.txt`.
5. **A base that is an empty file because stage 1 is absent** (git passes a 0-byte `$BASE`): planning must treat every difference as a conflict and the header must say "No common ancestor". → Task 2 test `An_empty_base_with_no_stage_1_is_absent`.

---

## File Structure

```
resolver/
  Directory.Packages.props                       (modify: add xunit.v3, Avalonia.Headless.XUnit)
  CthulhuResolve.slnx                            (modify: add 3 projects)
  README.md                                      (modify: Phase 1 status, merge-tool config)
  src/Resolve.Core/
    Cli/CommandLine.cs                           ExitCode, MergeToolArguments, ParseOutcome, CommandLine.Parse
    Session/MergeInputs.cs                       MergeInputs, BaseKind, LoadOutcome, MergeInputLoader
    Session/OperationRoles.cs                    OperationKind, SideLabels, OperationRoles.DetectAsync
    Merge/MergePlanning.cs                       MergePlanning.PlanAsync
    Resolution/TextChange.cs                     TextChange, ResolutionChoice, RegionView, DocumentCounts
    Resolution/ResolutionDocument.cs             text + anchors + region states + undo
    Resolution/RegionCommands.cs                 RegionCommand, CommandOutcome, region text building
    Resolution/LineMap.cs                        monotone piecewise-linear pane map
    Session/MergeToolSession.cs                  open, save, exit decisions, marker scan
  src/Resolve.App/                               Avalonia, OutputType Exe, AssemblyName cthulhu-resolve
    Resolve.App.csproj
    Program.cs                                   CLI → session → window → exit code
    App.cs
    Editor/IEditorSurface.cs                     IEditorSurface, Decoration, RegionTint
    Editor/AvaloniaEditSurface.cs                adapter; RegionBackground renderer
    Panes/BandCanvas.cs                          Bézier bands between two panes
    Panes/ThreePaneView.cs                       panes, bands, sync scroll
    Shell/MergeToolWindow.cs                     header, counters, action bar, footer, dialogs
    Shell/Keymap.cs                              key → command table
    Shell/ConflictNavigator.cs                   O1-A accessible region list + live status
  tests/Resolve.Core.Tests/                      (extend) one file per core unit
  tests/Resolve.App.Tests/                       xUnit v3 + Avalonia.Headless.XUnit
  tests/Resolve.MergeToolProbe/                  test-only console: drives MergeToolSession like a user, for acceptance 8
```

---

### Task 1: Command line and exit codes

**Files:**
- Create: `resolver/src/Resolve.Core/Cli/CommandLine.cs`
- Test: `resolver/tests/Resolve.Core.Tests/CommandLineTests.cs`

**Interfaces:**
- Produces:
  - `public enum ExitCode { Resolved = 0, Unresolved = 1, Usage = 2, InputUnreadable = 3, WriteFailed = 4, ExternalChangeDeclined = 5 }`
  - `public sealed record MergeToolArguments(string Base, string Local, string Remote, string Merged, string? LabelBase, string? LabelLocal, string? LabelRemote, string? Repo, bool NoRepoContext)`
  - `public abstract record ParseOutcome` with nested `Parsed(MergeToolArguments Arguments)`, `Help(string Text)`, `Version(string Text)`, `Invalid(string Message)`
  - `public static class CommandLine { public static ParseOutcome Parse(IReadOnlyList<string> args); public const string Usage = …; }`

A hand-written parser, not System.CommandLine: one sub-command, and the parser must be pure and fully tested (spec §8.1 forms: `--opt value`, `--opt=value`, `--` ends options). `--wait` is accepted and ignored (it is the default). `--no-wait`, `--reuse-window`, `--initial`, `--readonly-sources`, `repo`, `diff` → `Invalid("… is not available yet (Phase 1)")`.

- [ ] **Step 1: Write the failing tests**

```csharp
[Fact] public void Merge_with_the_four_paths_parses()
{
    var outcome = CommandLine.Parse(["merge", "--base", "b", "--local", "l", "--remote", "r", "--merged", "m"]);
    var parsed = Assert.IsType<ParseOutcome.Parsed>(outcome).Arguments;
    Assert.Equal(("b", "l", "r", "m"), (parsed.Base, parsed.Local, parsed.Remote, parsed.Merged));
    Assert.False(parsed.NoRepoContext);
}
[Fact] public void Leading_dash_paths_parse_after_double_dash_and_with_equals()
{
    var a = Assert.IsType<ParseOutcome.Parsed>(CommandLine.Parse(["merge", "--base=-b", "--local=-l", "--remote=-r", "--merged=-m"])).Arguments;
    Assert.Equal("-m", a.Merged);
    var b = CommandLine.Parse(["merge", "--base", "-b", "--local", "l", "--remote", "r", "--merged", "m"]);
    Assert.IsType<ParseOutcome.Invalid>(b); // "-b" is an option-like value without '=' or '--'
}
[Theory]
[InlineData(new[] { "merge", "--local", "l", "--remote", "r", "--merged", "m" })]          // missing --base
[InlineData(new[] { "merge", "--base", "b", "--local", "l", "--remote", "r", "--merged" })] // no value
[InlineData(new[] { "merge", "--base", "b", "--base", "c", "--local", "l", "--remote", "r", "--merged", "m" })] // twice
[InlineData(new[] { "merge", "--bogus", "x" })]
[InlineData(new[] { "repo" })]
[InlineData(new string[0])]
public void Bad_arguments_are_invalid(string[] args) => Assert.IsType<ParseOutcome.Invalid>(CommandLine.Parse(args));
[Fact] public void Labels_repo_and_no_repo_context_are_kept() { /* --label-local x --label-remote y --label-base z --repo d --no-repo-context → fields set */ }
[Fact] public void Help_and_version() { Assert.IsType<ParseOutcome.Help>(CommandLine.Parse(["--help"])); Assert.IsType<ParseOutcome.Version>(CommandLine.Parse(["--version"])); }
[Fact] public void Exit_codes_are_the_spec_values() => Assert.Equal(new[] { 0, 1, 2, 3, 4, 5 }, Enum.GetValues<ExitCode>().Select(c => (int)c));
```

- [ ] **Step 2: Run to verify they fail** — `dotnet test resolver/CthulhuResolve.slnx --filter FullyQualifiedName~CommandLineTests` → compile errors (types missing).
- [ ] **Step 3: Implement `CommandLine.Parse` and the types in `Cli/CommandLine.cs`.** A value starting with `-` is accepted only in `--opt=value` form or after `--`.
- [ ] **Step 4: Run** the same filter → all pass.
- [ ] **Step 5: Commit** — `git commit -m "feat(resolver): merge command line and exit codes"`

---

### Task 2: Loading the inputs and the role labels

**Files:**
- Create: `resolver/src/Resolve.Core/Session/MergeInputs.cs`, `resolver/src/Resolve.Core/Session/OperationRoles.cs`
- Test: `resolver/tests/Resolve.Core.Tests/MergeInputsTests.cs`

**Interfaces:**
- Consumes: `MergeToolArguments` (Task 1); `TextCodec.Decode`, `DecodedText`, `RepoLocation.DiscoverAsync`, `RepoLocation.GitPathAsync`, `UnmergedIndex.ListAsync`, `FileFingerprint.Capture` (Phase 0).
- Produces:
  - `public enum BaseKind { Present, EmptyStage, Absent, AbsentAssumed }` (`AbsentAssumed`: empty `$BASE` and no repo context to check)
  - `public sealed record MergeInputs(string MergedPath, string BaseFile, string LeftFile, string RightFile, RepoLocation? Repo, RepoPath? RelativePath, BaseKind BaseKind, DecodedText Base, DecodedText Left, DecodedText Right, SideLabels Labels, FileFingerprint? MergedAtLoad)` — `Base` is empty (no lines) unless `BaseKind == Present`.
  - `public abstract record LoadOutcome` with `Loaded(MergeInputs Inputs)` and `Unreadable(string Reason)`.
  - `public static class MergeInputLoader { public static Task<LoadOutcome> LoadAsync(IGitRunner runner, MergeToolArguments arguments, CancellationToken ct); }`
  - `public enum OperationKind { Merge, Rebase, CherryPick, Revert, Unknown }`
  - `public sealed record SideLabels(OperationKind Operation, string LeftRole, string RightRole, string? LeftDetail, string? RightDetail, string? Banner)`
  - `public static class OperationRoles { public static Task<SideLabels> DetectAsync(IGitRunner runner, RepoLocation? repo, MergeToolArguments arguments, CancellationToken ct); }`

Repo context: `--repo` if given, else `RepoLocation.DiscoverAsync` from MERGED's directory, unless `--no-repo-context`; a discovery failure means no repo context (not an error). `RelativePath` = MERGED relative to the worktree with `/` separators.

Base: a non-empty `$BASE` → `Present`. A 0-byte `$BASE` with repo context → `UnmergedIndex.ListAsync`, find `RelativePath`: stage 1 present → `EmptyStage` (an empty file is the real base), absent → `Absent`. With no repo context → `AbsentAssumed`.

Decoding: each present input through `TextCodec.Decode`; `Binary` or `Undecodable` on any of them → `Unreadable("<which> is binary|not decodable: <reason>")` (exit 3 later). Inputs are read with `File.ReadAllBytes` (no handle kept). `MergedAtLoad = FileFingerprint.Capture(MergedPath)`.

Roles (spec §3.3, Phase 1 subset), checked in this order through `repo.GitPathAsync`:

| Evidence | Operation | LeftRole | RightRole | Banner |
|---|---|---|---|---|
| `rebase-merge/` or `rebase-apply/` exists | Rebase | `Upstream / onto + already-rebased` | `Your commit being replayed` | `Rebase: LEFT is the branch you are rebasing onto (Git's "ours"). RIGHT is your own commit (Git's "theirs").` |
| `MERGE_HEAD` | Merge | `Current branch` | `Incoming branch` | null |
| `CHERRY_PICK_HEAD` | CherryPick | `Current branch` | `Picked commit` | null |
| `REVERT_HEAD` | Revert | `Current branch` | `Inverse of commit` | null |
| none, or no repo | Unknown | `Git stage 2` | `Git stage 3` | null |

`LeftDetail`/`RightDetail` = `--label-local`/`--label-remote` when given, else null.

- [ ] **Step 1: Write the failing tests** (real repositories via `GitFixture.ConflictAsync`)

```csharp
[Fact] public async Task A_merge_conflict_loads_three_texts_with_merge_roles()
{
    using var fx = await GitFixture.CreateAsync();
    await fx.ConflictAsync(f => f.Write("a.txt", "one\nbase\nthree\n"), f => f.Write("a.txt", "one\nleft\nthree\n"), f => f.Write("a.txt", "one\nright\nthree\n"));
    var args = await MergeToolCall.WriteStageFilesAsync(fx, "a.txt"); // test helper: checkout-index --temp --stage=all, returns MergeToolArguments
    var loaded = Assert.IsType<LoadOutcome.Loaded>(await MergeInputLoader.LoadAsync(fx.Git, args, default)).Inputs;
    Assert.Equal(BaseKind.Present, loaded.BaseKind);
    Assert.Equal("left", loaded.Left.Lines[1].Content);
    Assert.Equal((OperationKind.Merge, "Current branch", "Incoming branch"), (loaded.Labels.Operation, loaded.Labels.LeftRole, loaded.Labels.RightRole));
    Assert.Equal("a.txt", loaded.RelativePath?.Value);
}
[Fact] public async Task An_empty_base_with_no_stage_1_is_absent() { /* add/add-shaped conflict where stage 1 is missing but a 0-byte $BASE is passed → BaseKind.Absent */ }
[Fact] public async Task An_empty_base_without_repo_context_is_assumed_absent() { /* --no-repo-context → BaseKind.AbsentAssumed, Labels.Operation == Unknown, roles "Git stage 2"/"Git stage 3" */ }
[Fact] public async Task A_binary_side_is_unreadable() { /* LEFT file contains a NUL in the first 8000 bytes → LoadOutcome.Unreadable mentioning "local" */ }
[Fact] public async Task A_rebase_conflict_gets_rebase_roles_and_the_banner() { /* git rebase stopping on a conflict → Operation Rebase, Banner contains "Git's \"ours\"" */ }
[Fact] public async Task Inputs_are_not_held_open() { /* after LoadAsync, File.Delete on all three temp files succeeds (Windows sharing) */ }
```

`MergeToolCall` is a new test helper in `tests/Resolve.Core.Tests/Support/MergeToolCall.cs`: runs `git checkout-index --temp --stage=all -z -- <path>` in the fixture, maps stages to the temp names, writes a 0-byte file for a missing stage, and returns `MergeToolArguments` with `Merged` = the worktree path.

- [ ] **Step 2: Run** `--filter FullyQualifiedName~MergeInputsTests` → fails to compile.
- [ ] **Step 3: Implement** `MergeInputLoader.LoadAsync` and `OperationRoles.DetectAsync` as specified above.
- [ ] **Step 4: Run** the filter → pass.
- [ ] **Step 5: Commit** — `git commit -m "feat(resolver): load merge-tool inputs with repo context and role labels"`

---

### Task 3: Planning from git's own diffs

**Files:**
- Create: `resolver/src/Resolve.Core/Merge/MergePlanning.cs`
- Test: `resolver/tests/Resolve.Core.Tests/MergePlanningTests.cs`

**Interfaces:**
- Consumes: `MergeInputs` (Task 2); `GitLineDiff.ComputeAsync`, `LineInterner`, `Diff3Planner.Plan(int[], int[], int[], IReadOnlyList<Edit>, IReadOnlyList<Edit>, DiffAlgorithm)`, `Diff3Planner.Plan(IReadOnlyList<Line>…)` (Phase 0).
- Produces: `public static class MergePlanning { public static Task<MergePlan> PlanAsync(IGitRunner runner, MergeInputs inputs, CancellationToken ct); }`

Rule (spec §10.1 amended by S2): when every input that has content uses the same encoding and that encoding is `Utf8` or `Utf8Bom`, the edit scripts come from `GitLineDiff.ComputeAsync(…, DiffAlgorithm.Histogram)` over the files git passed (base → local, base → remote); a base that is not `Present` is diffed from an empty file created under `Path.GetTempPath()/cthulhu-resolve-<random>/` and deleted in `finally`. Otherwise (UTF-16/32, or BOMs differing between sides) plan in-process with `Diff3Planner.Plan(lines…, DiffAlgorithm.Histogram)`. Interning uses one `LineInterner` for all three so ids compare across sides.

- [ ] **Step 1: Write the failing tests**

```csharp
[Fact] public async Task Criterion_1_shape_gives_one_conflict_and_two_changes()
{
    // base: 50 lines "line 1".."line 50"; LEFT changes lines 3 and 20; RIGHT changes lines 20 and 40 (1-based)
    var plan = await MergePlanning.PlanAsync(Fixtures.Git(), await Inputs.FromTextsAsync(base50, left, right), default);
    Assert.Equal(1, plan.ConflictCount);
    Assert.Equal(2, plan.ChangeCount);
    var conflict = plan.Regions.Single(r => r.Kind == RegionKind.Conflict);
    Assert.Equal(19, conflict.Left.Start); // zero-based line 20
}
[Fact] public async Task The_plan_matches_git_merge_file_conflict_count() { /* same inputs → `git merge-file -p --diff3` marker count == plan.ConflictCount */ }
[Fact] public async Task Differing_boms_plan_in_process_and_still_tile_every_side() { /* LEFT Utf8Bom, RIGHT Utf8 → regions tile left, right and base line counts */ }
[Fact] public async Task No_temp_files_are_left_behind() { /* absent base: the temp directory is gone after PlanAsync */ }
```

`Inputs.FromTextsAsync` (test helper in `Support/MergeToolCall.cs`) writes three temp files and returns `MergeInputs` with no repo context; `Fixtures.Git()` returns an `IsolatedGitRunner` (Phase 0 test support) with a temp home.

- [ ] **Step 2: Run** `--filter FullyQualifiedName~MergePlanningTests` → compile failure.
- [ ] **Step 3: Implement `MergePlanning.PlanAsync`** per the rule above.
- [ ] **Step 4: Run** → pass.
- [ ] **Step 5: Commit** — `git commit -m "feat(resolver): plan merge-tool inputs from git's histogram diffs"`

---

### Task 4: The resolution document: initial result, anchors, region states

**Files:**
- Create: `resolver/src/Resolve.Core/Resolution/TextChange.cs`, `resolver/src/Resolve.Core/Resolution/ResolutionDocument.cs`
- Test: `resolver/tests/Resolve.Core.Tests/ResolutionDocumentTests.cs`

**Interfaces:**
- Consumes: `MergePlan`, `PlanRegion`, `RegionKind`, `LineRange`, `Line` (Phase 0).
- Produces:
  - `public readonly record struct TextChange(int Offset, int RemovedLength, string InsertedText)`
  - `public enum ResolutionChoice { Unresolved, Base, Left, Right, LeftThenRight, RightThenLeft, Manual }` (spec §6.4)
  - `public sealed record RegionView(int RegionId, RegionKind Kind, int Start, int End, LineRange ResultLines, ResolutionChoice Choice, bool Acknowledged)` — `Start`/`End` are char offsets in `Text`.
  - `public readonly record struct DocumentCounts(int UnresolvedConflicts, int ManualUnacknowledged, int Changes)`
  - `public sealed partial class ResolutionDocument` with:
    - `public static ResolutionDocument Create(MergePlan plan, IReadOnlyList<Line> @base, IReadOnlyList<Line> left, IReadOnlyList<Line> right, TimeProvider clock)`
    - `public string Text { get; }`, `public int Version { get; }`, `public IReadOnlyList<RegionView> Regions { get; }`, `public DocumentCounts Counts { get; }`, `public bool IsComplete { get; }`
    - `public IReadOnlyList<TextChange> Edit(TextChange change)` — a manual edit; returns the changes to show (here: the same change)
    - `public int LineOfOffset(int offset)`

Initial result (D4, spec §0): `Equal` → base lines; `LeftOnly` → left lines, choice `Left`; `RightOnly` → right lines, choice `Right`; `BothSame` → left lines, choice `Left`; `Conflict` → base lines, choice `Unresolved`. Text is the concatenation of `Line.ToString()` (terminators kept).

Anchors (spec §10.4): each region has a start anchor that sticks left and an end anchor that sticks right; an edit replacing `[a,b)` with ℓ chars maps `x ↦ x` if `x < a`, `x + ℓ − (b − a)` if `x ≥ b`, and `a` (start) or `a + ℓ` (end) if `x ∈ [a,b)`. A region is touched by an edit when `a ≤ End` and `b ≥ Start`.

Manual detection (spec §7.2): every region stores the hash of the text its last action produced (`ExpectedText`, SHA-256 of UTF-16 code units). After an edit, each touched region whose text no longer matches becomes `Manual`; one whose text matches again regains its stored choice. Only `Conflict` regions need acknowledgement: a `Manual` conflict starts unacknowledged; a manual edit in any other region is `Manual` with `Acknowledged = true`.

Counts: `UnresolvedConflicts` = conflicts with choice `Unresolved`; `ManualUnacknowledged` = conflicts `Manual ∧ ¬Acknowledged`; `Changes` = regions of kind `LeftOnly`, `RightOnly` or `BothSame`. `IsComplete` = both of the first two are 0.

- [ ] **Step 1: Write the failing tests**

```csharp
[Fact] public void The_initial_result_applies_non_conflicting_changes_and_keeps_base_in_conflicts()
{
    var doc = Docs.Criterion1(); // base 50 lines, LEFT 3 & 20, RIGHT 20 & 40
    Assert.Equal(new DocumentCounts(1, 0, 2), doc.Counts);
    Assert.Equal("left 3", Lines(doc.Text)[2]); Assert.Equal("right 40", Lines(doc.Text)[39]); Assert.Equal("line 20", Lines(doc.Text)[19]);
    Assert.False(doc.IsComplete);
}
[Fact] public void Typing_inside_a_conflict_makes_it_manual_and_unacknowledged() { /* Edit inside the conflict → Choice Manual, Acknowledged false, Counts.ManualUnacknowledged 1 */ }
[Fact] public void An_edit_straddling_two_regions_marks_both_manual() { }
[Fact] public void Editing_back_to_the_produced_text_restores_the_choice() { }
[Fact] public void Deleting_a_conflicts_text_leaves_it_manual_not_resolved() { /* criterion 11: delete the whole region text → Manual, unacknowledged */ }
[Fact] public void Anchors_stay_ordered_under_random_edits() =>   // P3
    Gen.Int[0, 400].SelectMany(o => Gen.Int[0, 20].Select(r => (o, r))).Array[1, 40].Sample(edits =>
    {
        var doc = Docs.Criterion1();
        foreach (var (o, r) in edits) { var at = o % (doc.Text.Length + 1); doc.Edit(new TextChange(at, Math.Min(r, doc.Text.Length - at), "x\n")); }
        var views = doc.Regions;
        Assert.All(views, v => Assert.True(v.Start <= v.End));
        Assert.True(views.Zip(views.Skip(1)).All(p => p.First.End <= p.Second.Start));
    });
```

`Docs` is a test helper (`tests/Resolve.Core.Tests/Support/Docs.cs`) building documents from in-memory lines with `Diff3Planner.Plan(lines…, Histogram)` and `TimeProvider.System`: `Criterion1()` (base 50 lines, LEFT changes 3 and 20, RIGHT changes 20 and 40) and `NoFinalNewline()` (one conflict on the last line; RIGHT's last line has no terminator). `Lines(text)` splits on `\n`.

- [ ] **Step 2: Run** `--filter FullyQualifiedName~ResolutionDocumentTests` → compile failure.
- [ ] **Step 3: Implement** the types and `ResolutionDocument.Create`/`Edit`/`LineOfOffset` as specified. Text storage: a `StringBuilder`; regions as an ordered array of `(Start, End, Choice, Acknowledged, ExpectedHash, StoredChoice)`.
- [ ] **Step 4: Run** → pass.
- [ ] **Step 5: Commit** — `git commit -m "feat(resolver): resolution document with anchors and manual-edit detection"`

---

### Task 5: Region commands, typed newlines and undo

**Files:**
- Create: `resolver/src/Resolve.Core/Resolution/RegionCommands.cs`
- Modify: `resolver/src/Resolve.Core/Resolution/ResolutionDocument.cs`
- Test: `resolver/tests/Resolve.Core.Tests/RegionCommandTests.cs`

**Interfaces:**
- Consumes: Task 4's document and types.
- Produces (on `ResolutionDocument` unless stated):
  - `public abstract record RegionCommand` with `Accept(int RegionId, ResolutionChoice Choice)` (Choice ∈ Left, Right, LeftThenRight, RightThenLeft, Base), `Reset(int RegionId)`, `Acknowledge(int RegionId)`, `ApplyAllNonConflicting`
  - `public abstract record CommandOutcome` with `Changed(IReadOnlyList<TextChange> Changes, IReadOnlyList<int> SkippedManual)`, `NeedsConfirmation(int RegionId, string Prompt)`, `NoChange`
  - `public CommandOutcome Apply(RegionCommand command, bool confirmed = false)`
  - `public IReadOnlyList<TextChange>? Undo()` and `public IReadOnlyList<TextChange>? Redo()` — the changes to show, or null when there is nothing to undo/redo
  - `public string NewLine { get; }` — dominant EOL of LEFT, else RIGHT, else `"\n"` (spec §9.1); `Edit` rewrites any `"\n"` / `"\r\n"` in inserted text to `NewLine` before applying, and returns the rewritten change.

Region text per choice (zdiff3 trim, spec §10.2; `p`/`s` = `region.Trim.CommonPrefix`/`CommonSuffix`, sliced over the region's left and right lines): `Left` → left lines; `Right` → right lines; `Base` → base lines; `LeftThenRight` → `left[0..p) + left[p..L−s) + right[p..R−s) + left[L−s..L)`; `RightThenLeft` → `right[0..p) + right[p..R−s) + left[p..L−s) + right[R−s..R)`. `Reset` restores the initial (D4) text and choice. `Acknowledge` applies only to a `Manual` conflict.

`Accept` works on any non-`Equal` region, conflict or not. Accepting on a `Manual` region with `confirmed == false` returns `NeedsConfirmation(id, "Replace your manual edit in this region?")` and changes nothing (criterion 2). `ApplyAllNonConflicting` re-applies the D4 side to every non-conflict region that is not `Manual` and lists the `Manual` ones in `SkippedManual`.

Undo (spec §7.4): one log of `(TextChange[] forward, RegionTransition[])` entries; undo applies the inverse and restores region states in the same step. Consecutive single-character inserts or deletes in the same region within 1 s (measured with the injected `TimeProvider`) coalesce; commands never coalesce. A new edit after undo clears the redo stack.

- [ ] **Step 1: Write the failing tests**

```csharp
[Fact] public void Accept_left_resolves_the_conflict_and_completes_the_document()
{
    var doc = Docs.Criterion1(); var id = doc.Regions.Single(r => r.Kind == RegionKind.Conflict).RegionId;
    Assert.IsType<CommandOutcome.Changed>(doc.Apply(new RegionCommand.Accept(id, ResolutionChoice.Left)));
    Assert.Equal("left 20", Lines(doc.Text)[19]); Assert.True(doc.IsComplete);
}
[Fact] public void Both_left_then_right_duplicates_only_the_core() { /* conflict with shared first/last line → shared lines appear once, cores in L then R order */ }
[Fact] public void Criterion_2_accept_after_manual_edit_asks_and_one_undo_restores_text_and_state()
{
    var doc = Docs.Criterion1(); var c = Conflict(doc);
    doc.Edit(new TextChange(c.Start, 0, "mine "));
    var manualText = doc.Text;
    Assert.IsType<CommandOutcome.NeedsConfirmation>(doc.Apply(new RegionCommand.Accept(c.RegionId, ResolutionChoice.Left)));
    Assert.Equal(manualText, doc.Text);
    doc.Apply(new RegionCommand.Accept(c.RegionId, ResolutionChoice.Left), confirmed: true);
    Assert.Equal(ResolutionChoice.Left, Conflict(doc).Choice);
    doc.Undo();
    Assert.Equal(manualText, doc.Text); Assert.Equal(ResolutionChoice.Manual, Conflict(doc).Choice);
}
[Fact] public void Apply_all_non_conflicting_skips_manual_regions_and_lists_them() { }
[Fact] public void Acknowledge_completes_a_manual_conflict() { }
[Fact] public void Typed_newline_uses_the_dominant_eol_of_left() { /* LEFT all CRLF; Edit inserts "\n" → document text holds "\r\n" there */ }
[Fact] public void Accepting_right_on_a_last_line_without_newline_keeps_no_newline() { /* Docs.NoFinalNewline(): RIGHT's last line has no terminator → text does not end in "\n" after Accept Right */ }
[Fact] public void Typing_coalesces_within_a_second_and_commands_never_do() { /* FakeTimeProvider: 3 keystrokes 100 ms apart → one Undo removes all 3; Accept then Undo removes only the accept */ }
[Fact] public void Undo_n_after_n_operations_is_identity() =>   // P2
    Gen.Int[0, 5].Array[1, 30].Sample(ops =>
    {
        var doc = Docs.Criterion1(); var text = doc.Text; var states = doc.Regions.ToArray();
        foreach (var op in ops) Docs.RandomOp(doc, op);       // mixes Edit, Accept*, Reset, Acknowledge
        while (doc.Undo() is not null) { }
        Assert.Equal(text, doc.Text); Assert.Equal(states, doc.Regions);
    });
```

Use `Microsoft.Extensions.Time.Testing.FakeTimeProvider` (add `Microsoft.Extensions.TimeProvider.Testing` 10.11.0 to `Directory.Packages.props` and the core test project).

- [ ] **Step 2: Run** `--filter FullyQualifiedName~RegionCommandTests` → compile failure.
- [ ] **Step 3: Implement** commands, region-text building, `NewLine`, and the undo log as specified.
- [ ] **Step 4: Run** the filter, then the whole core suite → pass.
- [ ] **Step 5: Commit** — `git commit -m "feat(resolver): region commands, typed newlines and one undo history"`

---

### Task 6: Pane line maps

**Files:**
- Create: `resolver/src/Resolve.Core/Resolution/LineMap.cs`
- Test: `resolver/tests/Resolve.Core.Tests/LineMapTests.cs`

**Interfaces:**
- Consumes: `RegionView.ResultLines` (Task 4), `PlanRegion.Left`/`.Right` (Phase 0).
- Produces:
  - `public sealed class LineMap { public static LineMap Build(IReadOnlyList<(LineRange Side, LineRange Result)> blocks, int sideLines, int resultLines); public double ToResult(double sideLine); public double ToSide(double resultLine); public IReadOnlyList<(LineRange Side, LineRange Result)> Blocks { get; } }`
  - `public static class PaneMaps { public static (LineMap Left, LineMap Right) For(MergePlan plan, ResolutionDocument document); }`

Spec §10.5: monotone, piecewise linear, breakpoints at block boundaries, 1:1 inside equal runs, proportional inside a block; an empty range on one side maps the whole other range to its single position. Lines are fractional (sub-line scroll positions).

- [ ] **Step 1: Write the failing tests**

```csharp
[Fact] public void Equal_runs_map_one_to_one_and_blocks_proportionally() { /* blocks [(10,12)->(10,16)]: 5→5, 11→13, 14→18 */ }
[Fact] public void The_map_is_monotone_and_covers_both_ends() => Gen.… .Sample(blocks => { /* random tilings: ToResult non-decreasing over 0..sideLines; ToResult(0)==0; ToResult(sideLines)==resultLines; ToSide(ToResult(x)) == x on equal runs */ });
[Fact] public void An_empty_side_range_collapses_to_a_point() { }
[Fact] public void Maps_follow_the_document_after_an_edit() { /* PaneMaps.For after inserting 3 lines in the conflict: the left map's block for the conflict ends 3 result lines later */ }
```

- [ ] **Step 2: Run** → fails. **Step 3: Implement.** **Step 4: Run** → pass.
- [ ] **Step 5: Commit** — `git commit -m "feat(resolver): monotone line maps between panes"`

---

### Task 7: The merge-tool session: save, exit codes, marker scan

**Files:**
- Create: `resolver/src/Resolve.Core/Session/MergeToolSession.cs`
- Test: `resolver/tests/Resolve.Core.Tests/MergeToolSessionTests.cs`

**Interfaces:**
- Consumes: Tasks 1–6; `AtomicFileWriter`, `SaveOutcome`, `FileFingerprint`, `TextCodec.SplitLines`, `TextCodec.Encode` (Phase 0).
- Produces:
  - `public sealed record SessionOptions(string BackupRoot, TimeProvider Clock)`; `public static SessionOptions SessionOptions.Default` (BackupRoot = `LocalApplicationData/CthulhuResolve/backups`)
  - `public abstract record OpenOutcome` with `Opened(MergeToolSession Session)` and `Failed(ExitCode Code, string Message)`
  - `public sealed class MergeToolSession` with:
    - `public static Task<OpenOutcome> OpenAsync(IGitRunner runner, MergeToolArguments arguments, SessionOptions options, CancellationToken ct)` — load (Task 2; `Unreadable` → `Failed(InputUnreadable)`), plan (Task 3), `ResolutionDocument.Create` (Task 4); a `RelativePath` that is not UU in the index → `Failed(InputUnreadable, "… is not a both-modified conflict (Phase 1 handles UU only)")`
    - `public MergeInputs Inputs { get; }`, `public MergePlan Plan { get; }`, `public ResolutionDocument Document { get; }`
    - `public bool EncodingsDiffer { get; }`, `public TextEncodingKind ResultEncoding { get; set; }` (defaults to LEFT's)
    - `public bool IsSaved { get; }` — true when the last save wrote the current `Document.Version`
    - `public SaveResult Save(bool overwriteExternalChange = false)` with `public abstract record SaveResult { Saved; ExternalChange; Failed(string Message) }`
    - `public ExitCode SaveAndExit(bool overwriteExternalChange = false)` — refuses (returns `Unresolved` and writes nothing) unless `Document.IsComplete`; otherwise saves → `Resolved`, or `WriteFailed` / `ExternalChangeDeclined`
    - `public ExitCode ExitUnresolved(bool overwriteExternalChange = false)` — saves what RESULT shows → `Unresolved`, or `WriteFailed` / `ExternalChangeDeclined`
    - `public ExitCode Cancel()` — writes nothing → `Unresolved`
    - `public IReadOnlyList<string> MarkerWarnings()` — `"conflict-marker-like line at N"` for each 1-based line matching `^(<{7}|={7}|>{7}|\|{7})( |$)`

Save: encode `TextCodec.Encode(ResultEncoding, TextCodec.SplitLines(Document.Text))`; `AtomicFileWriter.Save(MergedPath, bytes, expected, backupDirectory)` where `expected` is the fingerprint at load (or of our last save), and `backupDirectory` is `BackupRoot/<first 16 hex of SHA-256(repo worktree or "no-repo")>/<RelativePath or file name with '/' → '_'>` on the first save only, null after. `ExternallyModified` with `overwriteExternalChange == false` → `SaveResult.ExternalChange`; with true → save again with `expected` = the current fingerprint.

- [ ] **Step 1: Write the failing tests**

```csharp
[Fact] public async Task Cancel_leaves_merged_bytes_and_mtime_and_the_worktree_untouched()   // criterion 5
{
    using var fx = await Fixtures.Criterion1ConflictAsync(); var args = await MergeToolCall.WriteStageFilesAsync(fx, "a.txt");
    var before = (File.ReadAllBytes(args.Merged), File.GetLastWriteTimeUtc(args.Merged)); var files = Fixtures.Listing(fx.Worktree);
    var s = Opened(await MergeToolSession.OpenAsync(fx.Git, args, Fixtures.Options(), default));
    s.Document.Edit(new TextChange(0, 0, "edit "));
    Assert.Equal(ExitCode.Unresolved, s.Cancel());
    Assert.Equal(before, (File.ReadAllBytes(args.Merged), File.GetLastWriteTimeUtc(args.Merged)));
    Assert.Equal(files, Fixtures.Listing(fx.Worktree));
}
[Fact] public async Task Save_and_exit_refuses_while_a_conflict_is_unresolved() { }
[Fact] public async Task Resolving_then_save_and_exit_writes_the_result_backs_up_once_and_returns_0() { /* backup file under BackupRoot; second Save makes no second backup */ }
[Fact] public async Task An_external_write_blocks_the_save_until_overwrite_is_confirmed() { /* File.WriteAllText(Merged, …) after open → SaveResult.ExternalChange; SaveAndExit(false) → ExternalChangeDeclined; Save(true) → Saved */ }
[Fact] public async Task A_failed_write_returns_4() { /* MERGED made read-only → ExitUnresolved → WriteFailed */ }
[Fact] public async Task Choosing_left_everywhere_saves_lefts_exact_bytes() { /* P5: base present, LEFT is UTF-8 BOM with CRLF; Accept(Left) on every non-Equal region (RightOnly included) → saved bytes == LEFT's file bytes exactly */ }
[Fact] public void Side_derived_results_are_a_splice_of_input_bytes() { /* P1 (CsCheck): random choices among Left/Right per region, no manual edits → saved bytes == concatenation of the chosen sides' line bytes */ }
[Fact] public async Task Crlf_file_round_trips_byte_for_byte_after_a_typed_line() { /* Review Focus 1 */ }
[Fact] public void Marker_like_lines_are_reported_with_their_line_numbers() { }
[Fact] public async Task A_non_uu_path_is_refused_with_3() { /* DU conflict → Failed(InputUnreadable) */ }
```

`Fixtures` (test helper, `Support/Fixtures.cs`): `Criterion1ConflictAsync()` builds the criterion 1 repository; `Options()` = `new SessionOptions(<temp backup root>, TimeProvider.System)`; `Listing(dir)` = sorted relative file names excluding `.git`.

- [ ] **Step 2: Run** `--filter FullyQualifiedName~MergeToolSessionTests` → compile failure.
- [ ] **Step 3: Implement** `MergeToolSession` as specified.
- [ ] **Step 4: Run** the filter and the whole core suite → pass.
- [ ] **Step 5: Commit** — `git commit -m "feat(resolver): merge-tool session with atomic save, backups and exit codes"`

---

### Task 8: The app project and the editor surface

**Files:**
- Create: `resolver/src/Resolve.App/Resolve.App.csproj`, `resolver/src/Resolve.App/App.cs`, `resolver/src/Resolve.App/Editor/IEditorSurface.cs`, `resolver/src/Resolve.App/Editor/AvaloniaEditSurface.cs`
- Create: `resolver/tests/Resolve.App.Tests/Resolve.App.Tests.csproj`, `resolver/tests/Resolve.App.Tests/TestApp.cs`, `resolver/tests/Resolve.App.Tests/EditorSurfaceTests.cs`
- Modify: `resolver/Directory.Packages.props` (add `xunit.v3` 3.2.2, `Avalonia.Headless.XUnit` 12.1.4), `resolver/CthulhuResolve.slnx` (add both projects)

**Interfaces:**
- Consumes: `TextChange` (Task 4), `ResolutionChoice` (Task 4).
- Produces:
  - `public enum RegionTint { Unresolved, FromLeft, FromRight, Both, Manual, Applied }`
  - `public readonly record struct Decoration(int StartLine, int EndLine, RegionTint Tint)` (end exclusive; zero-based)
  - `public interface IEditorSurface { bool IsReadOnly { get; } double LineToY(int line); int YToLine(double y); double VerticalOffset { get; } double ViewportHeight { get; } void ScrollTo(double y); void ApplyChanges(IReadOnlyList<TextChange> changes); void SetDecorations(IReadOnlyList<Decoration> decorations); event Action<TextChange>? Edited; event Action<double>? Scrolled; Control View { get; } }`
  - `public sealed class AvaloniaEditSurface : IEditorSurface { public AvaloniaEditSurface(string text, bool readOnly, string newLine); }`

Project: `OutputType` `Exe` (git must read the exit code and stderr), `AssemblyName` `cthulhu-resolve`, references Avalonia, Avalonia.Desktop, Avalonia.Themes.Fluent, Avalonia.AvaloniaEdit and `Resolve.Core`. Test project: xUnit v3 (`xunit.v3`, `xunit.runner.visualstudio`, `Microsoft.NET.Test.Sdk`), `Avalonia.Headless.XUnit`, `[assembly: AvaloniaTestApplication(typeof(TestApp))]`, tests use `[AvaloniaFact]`.

Adapter rules from the S1 report: scroll through the template's `PART_ScrollViewer.Offset` (`TextEditor.ScrollToVerticalOffset` is a no-op in 12.0.0); disable AvaloniaEdit's own undo (`Document.UndoStack.SizeLimit = 0`) because the document owns undo; `ApplyChanges` must not raise `Edited` (guard flag); Enter inserts `newLine` (handle `KeyDown` for Enter before AvaloniaEdit); decorations draw through one `IBackgroundRenderer` with cached immutable brushes (S1 finding 5), and each tint also draws a distinct left-edge glyph pattern so state is never carried by colour alone (spec §4.6).

- [ ] **Step 1: Write the failing tests**

```csharp
[AvaloniaFact] public void Typing_raises_one_edited_change() { /* type "ab" via KeyTextInput on a window hosting the surface → two Edited events (Offset, 0, "a"), (Offset+1, 0, "b") */ }
[AvaloniaFact] public void Applied_changes_do_not_echo_as_edits() { }
[AvaloniaFact] public void Enter_inserts_the_configured_newline() { /* newLine "\r\n" → Edited InsertedText == "\r\n" */ }
[AvaloniaFact] public void Read_only_surfaces_ignore_typing() { }
[AvaloniaFact] public void Scroll_to_moves_the_view_and_reports_it() { /* 500-line text, ScrollTo(LineToY(200)) → YToLine(VerticalOffset) == 200 and Scrolled fired */ }
```

- [ ] **Step 2: Run** `dotnet test resolver/CthulhuResolve.slnx --filter FullyQualifiedName~EditorSurfaceTests` → fails.
- [ ] **Step 3: Implement** the project files, `App`, `IEditorSurface`, `AvaloniaEditSurface`.
- [ ] **Step 4: Run** → pass; then `dotnet build resolver/CthulhuResolve.slnx -c Release` → 0 warnings.
- [ ] **Step 5: Commit** — `git commit -m "feat(resolver): Avalonia app project and the AvaloniaEdit surface"`

---

### Task 9: Three panes, bands and synchronised scrolling

**Files:**
- Create: `resolver/src/Resolve.App/Panes/BandCanvas.cs`, `resolver/src/Resolve.App/Panes/ThreePaneView.cs`
- Test: `resolver/tests/Resolve.App.Tests/ThreePaneViewTests.cs`

**Interfaces:**
- Consumes: `IEditorSurface`/`AvaloniaEditSurface` (Task 8), `PaneMaps.For`, `LineMap` (Task 6), `MergeToolSession` (Task 7).
- Produces: `public sealed class ThreePaneView : Grid { public ThreePaneView(MergeToolSession session); public IEditorSurface Left { get; } public IEditorSurface Result { get; } public IEditorSurface Right { get; } public bool SyncScroll { get; set; } public void Refresh(IReadOnlyList<TextChange> changes); public void RevealLine(int resultLine); }`; `BandCanvas` exposes `internal int VisibleBlockCount { get; }` for tests.

Layout: LEFT | band | RESULT | band | RIGHT, star widths 1:0.08:1:0.08:1, GridSplitters between panes. LEFT and RIGHT are read-only surfaces over `Inputs.Left`/`Right` text; RESULT is editable over `Document.Text` with `Document.NewLine`. `Refresh` applies the changes to RESULT, recomputes `PaneMaps.For`, decorations and bands. A RESULT `Edited` event goes to `Document.Edit` and then `Refresh` with the returned change. Sync (spec §10.5): the pane that raised `Scrolled` is master; the others scroll to `LineToY(map(YToLine-fraction))`; a guard flag stops feedback loops. `BandCanvas` draws one cubic-Bézier quad per non-equal block visible in either pane, using the S1 spike's binary-search-and-walk (only on-screen blocks), tinted by the block's `RegionTint`.

Tints: `Unresolved` conflict, `FromLeft` (choice Left), `FromRight` (Right), `Both` (LeftThenRight/RightThenLeft), `Manual`, `Applied` (non-conflict region in its D4 state).

- [ ] **Step 1: Write the failing tests**

```csharp
[AvaloniaFact] public void Scrolling_left_in_an_equal_run_scrolls_result_one_to_one() { }
[AvaloniaFact] public void A_typed_line_in_result_shifts_the_maps_and_keeps_bands_attached() { }
[AvaloniaFact] public void Bands_are_drawn_only_for_visible_non_equal_blocks() { /* BandCanvas.VisibleBlockCount after scrolling to the top of a 2,000-line criterion-1-shaped file */ }
[AvaloniaFact] public void Sync_off_scrolls_panes_independently() { }
```

- [ ] **Step 2: Run** → fails. **Step 3: Implement.** **Step 4: Run** → pass.
- [ ] **Step 5: Commit** — `git commit -m "feat(resolver): three panes with bands and synchronised scrolling"`

---

### Task 10: The accessible conflict navigator (decision O1-A)

**Files:**
- Create: `resolver/src/Resolve.App/Shell/ConflictNavigator.cs`
- Test: `resolver/tests/Resolve.App.Tests/ConflictNavigatorTests.cs`

**Interfaces:**
- Consumes: `ResolutionDocument.Regions`, `RegionView`, `SideLabels` (Tasks 2, 4).
- Produces: `public sealed class ConflictNavigator : UserControl { public ConflictNavigator(MergeToolSession session); public int? CurrentRegionId { get; } public event Action<int>? RegionChosen; public void Select(int regionId); public void Announce(string text); public void Refresh(); public TextBlock Status { get; } internal IReadOnlyList<Control> Items(); }`

A `ListBox` with one item per non-`Equal` region, conflicts first in file order then changes. Each item's `AutomationProperties.Name` is exact copy: `"Conflict {n} of {N}, lines {a}–{b}, {state}"` or `"Change {n} of {M}, lines {a}–{b}, {state}"`, where lines are 1-based result lines and `state` ∈ `unresolved`, `taken from {LeftRole}`, `taken from {RightRole}`, `both, {LeftRole} first`, `both, {RightRole} first`, `kept the base`, `edited by hand, not yet marked resolved`, `edited by hand`. `Status` has `AutomationProperties.LiveSetting = AutomationLiveSetting.Polite` and holds the last announcement. Each item is focusable; Enter or a click raises `RegionChosen`.

- [ ] **Step 1: Write the failing tests**

```csharp
[AvaloniaFact] public void Items_name_the_region_its_lines_and_its_state()
{
    var nav = new ConflictNavigator(Sessions.Criterion1());
    Assert.Equal("Conflict 1 of 1, lines 20–20, unresolved", AutomationProperties.GetName(nav.Items()[0]));
    Assert.Equal("Change 1 of 2, lines 3–3, taken from Current branch", AutomationProperties.GetName(nav.Items()[1]));
}
[AvaloniaFact] public void The_status_line_is_a_polite_live_region() => Assert.Equal(AutomationLiveSetting.Polite, AutomationProperties.GetLiveSetting(new ConflictNavigator(Sessions.Criterion1()).Status));
[AvaloniaFact] public void Names_follow_the_document_after_a_resolution() { }
[AvaloniaFact] public void Enter_on_an_item_chooses_that_region() { }
```

`Sessions.Criterion1()` (test helper) (`tests/Resolve.App.Tests/Support/Sessions.cs`) opens a `MergeToolSession` on a real merge conflict built with the criterion-1 shape, so repo context gives merge roles (`Current branch` / `Incoming branch`).

- [ ] **Step 2: Run** → fails. **Step 3: Implement.** **Step 4: Run** → pass.
- [ ] **Step 5: Commit** — `git commit -m "feat(resolver): accessible conflict navigator with live announcements (O1-A)"`

---

### Task 11: The merge-tool window: header, actions, keys, dialogs

**Files:**
- Create: `resolver/src/Resolve.App/Shell/MergeToolWindow.cs`, `resolver/src/Resolve.App/Shell/Keymap.cs`
- Test: `resolver/tests/Resolve.App.Tests/MergeToolWindowTests.cs`

**Interfaces:**
- Consumes: Tasks 7, 9, 10.
- Produces:
  - `public enum WindowCommand { NextConflict, PreviousConflict, NextChange, PreviousChange, AcceptLeft, AcceptRight, BothLeftFirst, BothRightFirst, KeepBase, ResetRegion, AcknowledgeRegion, Undo, Redo, Save, ApplyAllNonConflicting }`
  - `public static class Keymap { public static WindowCommand? For(Key key, KeyModifiers modifiers); }`
  - `public sealed class MergeToolWindow : Window { public MergeToolWindow(MergeToolSession session, Func<string, Task<bool>> confirm); public ExitCode? Result { get; } public void Run(WindowCommand command); public string CounterText { get; } internal Button SaveButton { get; } internal Button SaveAndExitButton { get; } internal int ResultCaretLine(); }` (`InternalsVisibleTo("Resolve.App.Tests")` in the app project)
  - Test helper `Windows.Criterion1(Func<string, Task<bool>> confirm)` in `tests/Resolve.App.Tests/Support/Windows.cs`: opens the criterion-1 session from a real merge-conflict fixture and returns the shown window.

Keys (spec §4.6, Phase 1 subset): F8 / Shift+F8 → Next/PreviousConflict; F7 / Shift+F7 → Next/PreviousChange; Ctrl+Alt+← AcceptLeft; Ctrl+Alt+→ AcceptRight; Ctrl+Alt+↓ BothLeftFirst; Ctrl+Alt+↑ BothRightFirst; Ctrl+Alt+Backspace KeepBase; Ctrl+Alt+R ResetRegion; Ctrl+Enter AcknowledgeRegion; Ctrl+Z Undo; Ctrl+Y and Ctrl+Shift+Z Redo; Ctrl+S Save. Region commands act on the navigator's current region (navigation keys move it and reveal its first result line).

Layout (spec §4.2): one-line header `Merging {RelativePath or file name} · LEFT {LeftRole}[ · {LeftDetail}] · RIGHT {RightRole}[ · {RightDetail}]`, the role banner under it when non-null, a "No common ancestor — every difference is a conflict" banner when `BaseKind` is not `Present`, an encoding choice (`Save as LEFT's encoding` / `RIGHT's`) only when `EncodingsDiffer`; then `ThreePaneView`, the navigator to its right, the action bar (`Accept LEFT · {LeftRole}`, `Accept RIGHT · {RightRole}`, `Both, LEFT first`, `Both, RIGHT first`, `Keep base`, `Mark resolved`, `Apply all non-conflicting`), and the footer `[Save & exit (0)]` `[Exit unresolved (1)]` `[Cancel — leave file untouched (1)]`. `CounterText` = `"{c} conflict(s) · {n} change(s)"` plus `" · {m} manual"` when m > 0, singular for 1 (criterion 1 reads `"1 conflict · 2 changes"`). Tab order: LEFT → RESULT → RIGHT → navigator → action bar → footer.

Behaviour: `Save & exit` is enabled only when `Document.IsComplete`; it calls `SaveAndExit`, prints `MarkerWarnings()` to stderr, sets `Result`, closes. `Exit unresolved` saves through `ExitUnresolved`, and Ctrl+S saves through `Save`; both first `confirm("Saves exactly what RESULT shows; {k} regions still unresolved.")` when `Document.IsComplete` is false (spec §4.8). `Cancel` and the title-bar ✕ both call `Cancel()`; when the document has changes, `confirm("Discard your changes and leave the file untouched?")` first. `NeedsConfirmation` from a command → `confirm(prompt)` → re-apply with `confirmed: true`. `SaveResult.ExternalChange` → `confirm("The file changed on disk since it was opened. Overwrite it?")` → retry with overwrite, or `Result = ExternalChangeDeclined` on exit paths. Every command outcome is announced through `ConflictNavigator.Announce`.

- [ ] **Step 1: Write the failing tests**

```csharp
[AvaloniaFact] public void Criterion_1_counter_navigation_and_gates()
{
    var w = Windows.Criterion1(confirm: _ => Task.FromResult(true));
    Assert.Equal("1 conflict · 2 changes", w.CounterText);
    w.Run(WindowCommand.NextConflict);
    Assert.Equal(19, w.ResultCaretLine());                  // zero-based line 20
    Assert.True(w.SaveButton.IsEnabled); Assert.False(w.SaveAndExitButton.IsEnabled);
}
[AvaloniaFact] public void Criterion_2_in_the_window_asks_before_replacing_a_manual_edit() { /* type in the conflict, Ctrl+Alt+← → confirm called with "Replace your manual edit in this region?"; Ctrl+Z restores text and Manual */ }
[AvaloniaFact] public void Resolving_then_save_and_exit_closes_with_0() { }
[AvaloniaFact] public void Closing_the_window_after_edits_asks_then_cancels() { /* Review Focus 3: Close() → confirm asked; Result == Unresolved; MERGED bytes unchanged */ }
[AvaloniaFact] public void Keymap_matches_the_spec_table() { /* every row above: Keymap.For(key, mods) == command */ }
[AvaloniaFact] public void Tab_order_runs_left_result_right_navigator_actions_footer() { }
[AvaloniaFact] public void No_button_reads_ours_or_theirs() { /* walk the visual tree: no Button content or automation name contains "ours" or "theirs" (case-insensitive) */ }
```

- [ ] **Step 2: Run** → fails. **Step 3: Implement.** **Step 4: Run** → pass.
- [ ] **Step 5: Commit** — `git commit -m "feat(resolver): merge-tool window with keys, gates and confirmations"`

---

### Task 12: Program entry: command line to window to exit code

**Files:**
- Create: `resolver/src/Resolve.App/Program.cs`
- Test: `resolver/tests/Resolve.App.Tests/ProgramTests.cs`

**Interfaces:**
- Consumes: `CommandLine.Parse`, `ExitCode` (Task 1), `MergeToolSession.OpenAsync`, `SessionOptions.Default` (Task 7), `MergeToolWindow` (Task 11).
- Produces: `public static class Program { public static int Main(string[] args); internal static Task<int> RunAsync(string[] args, TextWriter stdout, TextWriter stderr, Func<MergeToolSession, Task<ExitCode>> showWindow); }`

`Main` = `RunAsync(args, Console.Out, Console.Error, ShowWindowAsync).GetAwaiter().GetResult()`. Order: parse (`Invalid` → message + `CommandLine.Usage` to stderr, return 2; `Help`/`Version` → stdout, return 0) → probe `GitVersion.ProbeAsync` (below `Minimum` → stderr "git 2.38 or later is required", return 2) → `OpenAsync` (`Failed` → message to stderr, return its code) → `showWindow` (starts Avalonia classic desktop lifetime with `MergeToolWindow`, waits until it closes, returns `Result ?? ExitCode.Unresolved`). `AtomicFileWriter.CleanOrphans(Path.GetDirectoryName(Merged), TimeSpan.FromHours(1))` runs once before opening.

- [ ] **Step 1: Write the failing tests** (no window: `showWindow` is a fake)

```csharp
[Fact] public async Task Usage_errors_exit_2_with_the_reason_on_stderr() { /* RunAsync(["merge"], …) == 2; stderr contains "--base" */ }
[Fact] public async Task An_unreadable_input_exits_3() { /* binary LOCAL → 3, showWindow never called */ }
[Fact] public async Task The_window_result_is_the_exit_code() { /* showWindow returns Resolved → 0 */ }
[Fact] public async Task Help_and_version_exit_0() { }
```

- [ ] **Step 2: Run** → fails. **Step 3: Implement.** **Step 4: Run** → pass; `dotnet run --project resolver/src/Resolve.App -- --version` prints the version and exits 0.
- [ ] **Step 5: Commit** — `git commit -m "feat(resolver): cthulhu-resolve entry point and exit-code plumbing"`

---

### Task 13: `git mergetool` end to end, docs, CI

**Files:**
- Create: `resolver/tests/Resolve.MergeToolProbe/Resolve.MergeToolProbe.csproj`, `resolver/tests/Resolve.MergeToolProbe/Program.cs`
- Create: `resolver/tests/Resolve.Core.Tests/MergeToolAcceptanceTests.cs`
- Modify: `resolver/CthulhuResolve.slnx` (add the probe), `resolver/README.md` (Phase 1 status table, merge-tool setup with the §8.4 config and `trustExitCode = true`), `.github/workflows/resolver.yml` (no change expected: it already runs `dotnet test CthulhuResolve.slnx` on Windows, macOS and Linux; verify the new projects run there)

**Interfaces:**
- Consumes: `CommandLine.Parse`, `MergeToolSession` (Tasks 1, 7).
- Produces: a test-only console `resolve-probe` that takes the same `merge …` arguments plus `PROBE_ACTION` from the environment (`accept-left-all`, `cancel`, `exit-unresolved`), drives the session exactly as the window would, and returns the session's exit code. It exists so acceptance 8 runs git's real `git mergetool` contract without a display; the window's own wiring is covered by Tasks 11–12.

- [ ] **Step 1: Write the failing tests**

```csharp
[Fact] public async Task Criterion_8_git_mergetool_resolves_and_git_stages_the_file()
{
    using var fx = await Fixtures.Criterion1ConflictAsync(path: "dir with space/-ünïcode.txt");
    await fx.RunAsync("config", "mergetool.probe.cmd", Probe.Command());   // dotnet "<probe.dll>" merge --base "$BASE" --local "$LOCAL" --remote "$REMOTE" --merged "$MERGED"
    await fx.RunAsync("config", "mergetool.probe.trustExitCode", "true");
    var run = await fx.TryRunWithEnvAsync(new() { ["PROBE_ACTION"] = "accept-left-all" }, "mergetool", "--no-prompt", "--tool", "probe", "--", "dir with space/-ünïcode.txt");
    Assert.Equal(0, run.ExitCode);
    var stages = await fx.RunAsync("ls-files", "-s", "--", "dir with space/-ünïcode.txt");
    Assert.Matches(@"^\d{6} [0-9a-f]+ 0\t", Encoding.UTF8.GetString(stages.StandardOutput));   // stage 0 only
    Assert.Empty(Fixtures.Listing(fx.Worktree).Where(f => f.Contains("_BASE_") || f.Contains("_LOCAL_") || f.Contains("_REMOTE_")));
}
[Fact] public async Task Cancel_through_git_mergetool_keeps_the_conflict() { /* PROBE_ACTION=cancel → git exit != 0; ls-files -u still lists stages 1–3 */ }
```

`Probe.Command()` resolves the probe's built `.dll` next to the test assembly (the core test project gets a `ProjectReference` to the probe with `ReferenceOutputAssembly="false"` and `OutputItemType="Content"` copying its output); `TryRunWithEnvAsync` is added to `GitFixture`.

- [ ] **Step 2: Run** `--filter FullyQualifiedName~MergeToolAcceptanceTests` → fails.
- [ ] **Step 3: Implement** the probe and the fixture helpers; update the README (status: Phase 1 ✅ items; "Use as git's merge tool" with the §8.4 snippet pointing at the built `cthulhu-resolve`).
- [ ] **Step 4: Run** `dotnet test resolver/CthulhuResolve.slnx -c Release` → every project passes with 0 warnings. Then by hand on a real conflict: set `mergetool.cthulhu.cmd` to the built `cthulhu-resolve.exe`, run `git mergetool --tool cthulhu -- <file>`, resolve with the keyboard only, Save & exit → git reports the merge as successful.
- [ ] **Step 5: Commit** — `git commit -m "test(resolver): git mergetool acceptance; docs: Phase 1 merge-tool setup"`; push and confirm the `resolver` workflow is green on all three OSes.
