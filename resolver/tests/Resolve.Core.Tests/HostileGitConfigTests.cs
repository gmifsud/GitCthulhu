using Resolve.Core.Cli;
using Resolve.Core.Diff;
using Resolve.Core.Merge;
using Resolve.Core.Persistence;
using Resolve.Core.Resolution;
using Resolve.Core.Session;
using Resolve.Core.Tests.Support;
using Resolve.Core.Text;

namespace Resolve.Core.Tests;

/// <summary>
/// Final review C2 / ruling R28: the plan must not depend on the user's git config. The fixture's global config
/// turns on line-ending conversion and hunk fusing, and its worktree normalises line endings by attribute.
/// </summary>
public sealed class HostileGitConfigTests
{
    private const string HostileGlobalConfig = "[core]\n\tautocrlf = true\n[diff]\n\tinterHunkContext = 5\n";

    private static async Task<GitFixture> HostileAsync(bool textAutoAttributes)
    {
        var fixture = await GitFixture.CreateAsync();
        await File.WriteAllTextAsync(Path.Join(fixture.Root, ".gitconfig"), HostileGlobalConfig);
        // GitFixture pins autocrlf off in the repository; drop that so the global setting applies.
        await fixture.RunAsync("config", "--unset", "core.autocrlf");
        if (textAutoAttributes)
        {
            fixture.Write(".gitattributes", "* text=auto\n");
        }
        return fixture;
    }

    private static string Join(IEnumerable<Line> lines) => string.Concat(lines.Select(line => line.ToString()));

    [Fact]
    public async Task Git_line_diff_keeps_end_of_line_changes_and_separate_hunks_under_hostile_config()
    {
        using var fixture = await HostileAsync(textAutoAttributes: false);
        fixture.Write("base", "a\nb\nc\nd\ne\nf\n");
        // An end-of-line-only change on line 1, then changes on lines 3 and 5 with one equal line between them.
        fixture.Write("side", "a\r\nb\nC\nd\nE\nf\n");

        var edits = await GitLineDiff.ComputeAsync(
            fixture.Git, fixture.Worktree, Path.Join(fixture.Worktree, "base"), Path.Join(fixture.Worktree, "side"), DiffAlgorithm.Histogram, default);

        Assert.Equal([new Edit(0, 1, 0, 1), new Edit(2, 3, 2, 3), new Edit(4, 5, 4, 5)], edits);
    }

    [Fact]
    public async Task Choosing_left_everywhere_saves_lefts_exact_bytes_under_hostile_config()   // P5
    {
        using var fixture = await HostileAsync(textAutoAttributes: true);
        var baseLines = Enumerable.Range(1, 12).Select(number => new Line($"line {number}", LineEnding.Lf)).ToArray();
        var left = baseLines.ToArray();
        left[1] = new Line("line 2", LineEnding.CrLf);   // end of line only: "* text=auto" hides it from git
        left[4] = new Line("left 5", LineEnding.CrLf);
        left[6] = new Line("left 7", LineEnding.Lf);
        var right = baseLines.ToArray();
        right[4] = new Line("right 5", LineEnding.Lf);
        right[8] = new Line("right 9", LineEnding.Lf);
        var inputs = await Inputs.FromTextsAsync(Join(baseLines), Join(left), Join(right), directory: Path.Join(fixture.Worktree, "stages"));
        inputs = inputs with { MergedAtLoad = FileFingerprint.Capture(inputs.MergedPath) };

        var plan = await MergePlanning.PlanAsync(fixture.Git, inputs, default);
        var document = ResolutionDocument.Create(plan, inputs.Base.Lines, inputs.Left.Lines, inputs.Right.Lines, TimeProvider.System);
        var session = new MergeToolSession(inputs, plan, document, Fixtures.Options());
        foreach (var region in document.Regions.Where(view => view.Kind != RegionKind.Equal).ToArray())
        {
            document.Apply(new RegionCommand.Accept(region.RegionId, ResolutionChoice.Left), confirmed: true);
        }

        Assert.Equal(ExitCode.Resolved, session.SaveAndExit());
        Assert.Equal(File.ReadAllBytes(inputs.LeftFile), File.ReadAllBytes(inputs.MergedPath));
    }
}
