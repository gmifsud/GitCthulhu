using System.Text;
using CsCheck;
using Resolve.Core.Cli;
using Resolve.Core.Diff;
using Resolve.Core.Merge;
using Resolve.Core.Persistence;
using Resolve.Core.Resolution;
using Resolve.Core.Session;
using Resolve.Core.Tests.Support;
using Resolve.Core.Text;

namespace Resolve.Core.Tests;

/// <summary>Skipped off Windows: only there can a read-only file deterministically refuse the replace (a root user ignores Unix modes).</summary>
public sealed class WindowsOnlyFactAttribute : FactAttribute
{
    public WindowsOnlyFactAttribute()
    {
        if (!OperatingSystem.IsWindows())
        {
            Skip = "A read-only target blocks the atomic replace only on Windows; Unix permission bits are not a reliable failure under root.";
        }
    }
}

public sealed class MergeToolSessionTests
{
    private static MergeToolSession Opened(OpenOutcome outcome) => Assert.IsType<OpenOutcome.Opened>(outcome).Session;

    private static void AcceptEverywhere(ResolutionDocument doc, ResolutionChoice choice)
    {
        foreach (var region in doc.Regions.Where(view => view.Kind != RegionKind.Equal).ToArray())
        {
            Assert.IsType<CommandOutcome.Changed>(doc.Apply(new RegionCommand.Accept(region.RegionId, choice), confirmed: true));
        }
    }

    private static MergeToolSession FromInputs(MergeInputs inputs, SessionOptions? options = null)
    {
        inputs = inputs with { MergedAtLoad = FileFingerprint.Capture(inputs.MergedPath) };
        var plan = Diff3Planner.Plan(inputs.Base.Lines, inputs.Left.Lines, inputs.Right.Lines, DiffAlgorithm.Histogram);
        var document = ResolutionDocument.Create(plan, inputs.Base.Lines, inputs.Left.Lines, inputs.Right.Lines, TimeProvider.System);
        return new MergeToolSession(inputs, plan, document, options ?? Fixtures.Options());
    }

    private static string Join(Line[] lines) => string.Concat(lines.Select(line => line.ToString()));

    [Fact]
    public async Task Cancel_leaves_merged_bytes_and_mtime_and_the_worktree_untouched()   // criterion 5
    {
        using var fx = await Fixtures.Criterion1ConflictAsync();
        var args = await MergeToolCall.WriteStageFilesAsync(fx, "a.txt");
        var before = (File.ReadAllBytes(args.Merged), File.GetLastWriteTimeUtc(args.Merged));
        var files = Fixtures.Listing(fx.Worktree);
        var s = Opened(await MergeToolSession.OpenAsync(fx.Git, args, Fixtures.Options(), default));
        s.Document.Edit(new TextChange(0, 0, "edit "));
        Assert.Equal(ExitCode.Unresolved, s.Cancel());
        Assert.Equal(before.Item1, File.ReadAllBytes(args.Merged));
        Assert.Equal(before.Item2, File.GetLastWriteTimeUtc(args.Merged));
        Assert.Equal(files, Fixtures.Listing(fx.Worktree));
    }

    [Fact]
    public async Task Save_and_exit_refuses_while_a_conflict_is_unresolved()
    {
        using var fx = await Fixtures.Criterion1ConflictAsync();
        var args = await MergeToolCall.WriteStageFilesAsync(fx, "a.txt");
        var before = (File.ReadAllBytes(args.Merged), File.GetLastWriteTimeUtc(args.Merged));
        var options = Fixtures.Options();
        var s = Opened(await MergeToolSession.OpenAsync(fx.Git, args, options, default));
        Assert.False(s.Document.IsComplete);
        Assert.Equal(ExitCode.Unresolved, s.SaveAndExit());
        Assert.Equal(before.Item1, File.ReadAllBytes(args.Merged));
        Assert.Equal(before.Item2, File.GetLastWriteTimeUtc(args.Merged));
        Assert.Empty(Directory.GetFiles(options.BackupRoot, "*", SearchOption.AllDirectories));
        Assert.False(s.IsSaved);
    }

    [Fact]
    public async Task Resolving_then_save_and_exit_writes_the_result_backs_up_once_and_returns_0()
    {
        using var fx = await Fixtures.Criterion1ConflictAsync();
        var args = await MergeToolCall.WriteStageFilesAsync(fx, "a.txt");
        var markerBytes = File.ReadAllBytes(args.Merged);
        var options = Fixtures.Options();
        var s = Opened(await MergeToolSession.OpenAsync(fx.Git, args, options, default));
        var conflict = s.Document.Regions.Single(view => view.Kind == RegionKind.Conflict);
        s.Document.Apply(new RegionCommand.Accept(conflict.RegionId, ResolutionChoice.Left));
        Assert.False(s.IsSaved);

        Assert.Equal(ExitCode.Resolved, s.SaveAndExit());

        Assert.True(s.IsSaved);
        Assert.Equal(s.Document.Text, File.ReadAllText(args.Merged));
        var lines = File.ReadAllLines(args.Merged);
        Assert.Equal(["left 3", "left 20", "right 40"], new[] { lines[2], lines[19], lines[39] });
        var backup = Assert.Single(Directory.GetFiles(options.BackupRoot, "*", SearchOption.AllDirectories));
        Assert.Equal(markerBytes, File.ReadAllBytes(backup));
        var backupDirectory = new DirectoryInfo(Path.GetDirectoryName(backup)!);
        Assert.Equal("a.txt", backupDirectory.Name);
        Assert.Matches("^[0-9a-f]{16}$", backupDirectory.Parent!.Name);
        Assert.Equal(options.BackupRoot, backupDirectory.Parent.Parent!.FullName.TrimEnd(Path.DirectorySeparatorChar));

        // A second save of a changed document is not backed up again.
        s.Document.Edit(new TextChange(0, 0, "more\n"));
        Assert.False(s.IsSaved);
        Assert.IsType<SaveResult.Saved>(s.Save());
        Assert.True(s.IsSaved);
        Assert.StartsWith("more\n", File.ReadAllText(args.Merged), StringComparison.Ordinal);
        Assert.Single(Directory.GetFiles(options.BackupRoot, "*", SearchOption.AllDirectories));
    }

    [Fact]
    public async Task An_external_write_blocks_the_save_until_overwrite_is_confirmed()
    {
        using var fx = await Fixtures.Criterion1ConflictAsync();
        var args = await MergeToolCall.WriteStageFilesAsync(fx, "a.txt");
        var s = Opened(await MergeToolSession.OpenAsync(fx.Git, args, Fixtures.Options(), default));
        var conflict = s.Document.Regions.Single(view => view.Kind == RegionKind.Conflict);
        s.Document.Apply(new RegionCommand.Accept(conflict.RegionId, ResolutionChoice.Right));
        File.WriteAllText(args.Merged, "changed elsewhere\n");

        Assert.IsType<SaveResult.ExternalChange>(s.Save());
        Assert.Equal(ExitCode.ExternalChangeDeclined, s.SaveAndExit(overwriteExternalChange: false));
        Assert.Equal(ExitCode.ExternalChangeDeclined, s.ExitUnresolved(overwriteExternalChange: false));
        Assert.Equal("changed elsewhere\n", File.ReadAllText(args.Merged));
        Assert.False(s.IsSaved);

        Assert.IsType<SaveResult.Saved>(s.Save(overwriteExternalChange: true));
        Assert.Equal(s.Document.Text, File.ReadAllText(args.Merged));
        Assert.True(s.IsSaved);
    }

    [WindowsOnlyFact]
    public async Task A_failed_write_returns_4()
    {
        using var fx = await Fixtures.Criterion1ConflictAsync();
        var args = await MergeToolCall.WriteStageFilesAsync(fx, "a.txt");
        var s = Opened(await MergeToolSession.OpenAsync(fx.Git, args, Fixtures.Options(), default));
        var before = File.ReadAllBytes(args.Merged);
        File.SetAttributes(args.Merged, FileAttributes.ReadOnly);

        Assert.Equal(ExitCode.WriteFailed, s.ExitUnresolved());

        Assert.Equal(before, File.ReadAllBytes(args.Merged));
        Assert.False(s.IsSaved);
        Assert.IsType<SaveResult.Failed>(s.Save());
    }

    [Fact]
    public async Task Choosing_left_everywhere_saves_lefts_exact_bytes()   // P5
    {
        var baseLines = Enumerable.Range(1, 12).Select(number => new Line($"line {number}", LineEnding.CrLf)).ToArray();
        var left = baseLines.ToArray();
        left[1] = new Line("left 2", LineEnding.CrLf);
        left[4] = new Line("left 5", LineEnding.CrLf);
        var right = baseLines.ToArray();
        right[4] = new Line("right 5", LineEnding.CrLf);
        right[8] = new Line("right 9", LineEnding.CrLf);
        var inputs = await Inputs.FromTextsAsync(Join(baseLines), Join(left), Join(right), TextEncodingKind.Utf8Bom, TextEncodingKind.Utf8Bom);
        var s = FromInputs(inputs);
        Assert.Contains(s.Document.Regions, view => view.Kind == RegionKind.RightOnly);
        AcceptEverywhere(s.Document, ResolutionChoice.Left);

        Assert.Equal(ExitCode.Resolved, s.SaveAndExit());

        var saved = File.ReadAllBytes(inputs.MergedPath);
        Assert.Equal(File.ReadAllBytes(inputs.LeftFile), saved);
        Assert.Equal([0xEF, 0xBB, 0xBF], saved[..3]);
    }

    [Fact]
    public async Task Side_derived_results_are_a_splice_of_input_bytes()   // P1
    {
        var baseLines = Enumerable.Range(1, 12).Select(number => new Line($"line {number}", LineEnding.Lf)).ToArray();
        var left = baseLines.ToArray();
        (left[1], left[4], left[8]) = (new Line("left 2", LineEnding.Lf), new Line("left 5", LineEnding.Lf), new Line("left 9", LineEnding.Lf));
        var right = baseLines.ToArray();
        (right[4], right[6], right[8]) = (new Line("right 5", LineEnding.Lf), new Line("right 7", LineEnding.Lf), new Line("right 9", LineEnding.Lf));
        var inputs = await Inputs.FromTextsAsync(Join(baseLines), Join(left), Join(right));
        var emptyMerged = File.ReadAllBytes(inputs.MergedPath);

        Gen.Bool.Array[4].Sample(useLeft =>
        {
            File.WriteAllBytes(inputs.MergedPath, emptyMerged);
            var s = FromInputs(inputs);
            var chosen = new List<Line>();
            var next = 0;
            foreach (var region in s.Plan.Regions)
            {
                if (region.Kind == RegionKind.Equal)
                {
                    chosen.AddRange(left[region.Left.Start..region.Left.End]);
                    continue;
                }
                var takeLeft = useLeft[next++];
                s.Document.Apply(new RegionCommand.Accept(region.Id, takeLeft ? ResolutionChoice.Left : ResolutionChoice.Right), confirmed: true);
                chosen.AddRange(takeLeft ? left[region.Left.Start..region.Left.End] : right[region.Right.Start..region.Right.End]);
            }
            Assert.Equal(4, next);
            Assert.Equal(ExitCode.Resolved, s.SaveAndExit());
            Assert.Equal(TextCodec.Encode(TextEncodingKind.Utf8, chosen), File.ReadAllBytes(inputs.MergedPath));
        }, threads: 1);
    }

    [Fact]
    public async Task Crlf_file_round_trips_byte_for_byte_after_a_typed_line()
    {
        var inputs = await Inputs.FromTextsAsync("a\r\nb\r\nc\r\n", "a\r\nL\r\nc\r\n", "a\r\nb\r\nc\r\n");
        var s = FromInputs(inputs);

        Assert.Equal(ExitCode.Resolved, s.SaveAndExit());
        Assert.Equal(File.ReadAllBytes(inputs.LeftFile), File.ReadAllBytes(inputs.MergedPath));

        s.Document.Edit(new TextChange(0, 0, "typed\n"));
        Assert.Equal(ExitCode.Resolved, s.SaveAndExit());
        Assert.Equal(Encoding.UTF8.GetBytes("typed\r\na\r\nL\r\nc\r\n"), File.ReadAllBytes(inputs.MergedPath));
    }

    [Fact]
    public async Task Changing_the_result_encoding_marks_the_session_unsaved_and_is_written()
    {
        var inputs = await Inputs.FromTextsAsync("a\n", "b\n", "a\n", TextEncodingKind.Utf8, TextEncodingKind.Utf16LeBom);
        var s = FromInputs(inputs);
        Assert.True(s.EncodingsDiffer);
        Assert.Equal(TextEncodingKind.Utf8, s.ResultEncoding);
        Assert.Equal(ExitCode.Resolved, s.SaveAndExit());
        Assert.True(s.IsSaved);

        s.ResultEncoding = TextEncodingKind.Utf16LeBom;
        Assert.False(s.IsSaved);
        Assert.IsType<SaveResult.Saved>(s.Save());
        Assert.True(s.IsSaved);
        Assert.Equal(TextCodec.Encode(TextEncodingKind.Utf16LeBom, TextCodec.SplitLines("b\n")), File.ReadAllBytes(inputs.MergedPath));
    }

    [Fact]
    public async Task Marker_like_lines_are_reported_with_their_line_numbers()
    {
        var left = "a\n<<<<<<< HEAD\n=======\n>>>>>>> x\n|||||||\n======== no\n=======x\n<<<<<<<<\n";
        var s = FromInputs(await Inputs.FromTextsAsync("x\n", left, "x\n"));
        Assert.Equal(
            ["conflict-marker-like line at 2", "conflict-marker-like line at 3", "conflict-marker-like line at 4", "conflict-marker-like line at 5"],
            s.MarkerWarnings());
    }

    [Fact]
    public async Task A_document_without_markers_has_no_warnings()
    {
        var s = FromInputs(await Inputs.FromTextsAsync("x\n", "left\n", "x\n"));
        Assert.Empty(s.MarkerWarnings());
    }

    [Fact]
    public async Task A_non_uu_path_is_refused_with_3()
    {
        using var fx = await GitFixture.CreateAsync();
        await fx.ConflictAsync(
            f => f.Write("a.txt", "base\n"),
            f => f.Delete("a.txt"),
            f => f.Write("a.txt", "right\n"));
        var args = await MergeToolCall.WriteStageFilesAsync(fx, "a.txt");
        var before = File.ReadAllBytes(args.Merged);

        var failed = Assert.IsType<OpenOutcome.Failed>(await MergeToolSession.OpenAsync(fx.Git, args, Fixtures.Options(), default));

        Assert.Equal(ExitCode.InputUnreadable, failed.Code);
        Assert.Contains("is not a both-modified conflict (Phase 1 handles UU only)", failed.Message, StringComparison.Ordinal);
        Assert.Equal(before, File.ReadAllBytes(args.Merged));
    }

    [Fact]
    public async Task An_already_resolved_path_is_refused_with_3()
    {
        using var fx = await Fixtures.Criterion1ConflictAsync();
        var args = await MergeToolCall.WriteStageFilesAsync(fx, "a.txt");
        File.WriteAllText(args.Merged, "resolved\n");
        await fx.RunAsync("add", "a.txt");

        var failed = Assert.IsType<OpenOutcome.Failed>(await MergeToolSession.OpenAsync(fx.Git, args, Fixtures.Options(), default));

        Assert.Equal(ExitCode.InputUnreadable, failed.Code);
    }

    [Fact]
    public async Task Unreadable_inputs_are_refused_with_3()
    {
        using var fx = await Fixtures.Criterion1ConflictAsync();
        var args = await MergeToolCall.WriteStageFilesAsync(fx, "a.txt");
        var missing = args with { Local = Path.Join(Path.GetDirectoryName(args.Local)!, "missing") };
        var failed = Assert.IsType<OpenOutcome.Failed>(await MergeToolSession.OpenAsync(fx.Git, missing, Fixtures.Options(), default));
        Assert.Equal(ExitCode.InputUnreadable, failed.Code);
    }
}
