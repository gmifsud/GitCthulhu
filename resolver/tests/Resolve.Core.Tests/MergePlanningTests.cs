using System.Text;
using Resolve.Core.Git;
using Resolve.Core.Merge;
using Resolve.Core.Text;
using Resolve.Core.Tests.Support;

namespace Resolve.Core.Tests;

public sealed class MergePlanningTests
{
    private static string Lines(IEnumerable<string> lines) => string.Concat(lines.Select(line => line + "\n"));

    private static string[] Fifty() => Enumerable.Range(1, 50).Select(n => $"line {n}").ToArray();

    private static (string Base, string Left, string Right) Criterion1()
    {
        var left = Fifty();
        left[2] = "left 3";
        left[19] = "left 20";
        var right = Fifty();
        right[19] = "right 20";
        right[39] = "right 40";
        return (Lines(Fifty()), Lines(left), Lines(right));
    }

    [Fact]
    public async Task Criterion_1_shape_gives_one_conflict_and_two_changes()
    {
        var (b, l, r) = Criterion1();
        var plan = await MergePlanning.PlanAsync(Fixtures.Git(), await Inputs.FromTextsAsync(b, l, r), default);
        Assert.Equal(1, plan.ConflictCount);
        Assert.Equal(2, plan.ChangeCount);
        var conflict = plan.Regions.Single(region => region.Kind == RegionKind.Conflict);
        Assert.Equal(19, conflict.Left.Start);
    }

    [Fact]
    public async Task The_plan_matches_git_merge_file_conflict_count()
    {
        var (b, l, r) = Criterion1();
        var git = Fixtures.Git();
        var inputs = await Inputs.FromTextsAsync(b, l, r);
        var plan = await MergePlanning.PlanAsync(git, inputs, default);

        var directory = Directory.CreateTempSubdirectory("cthulhu-oracle-").FullName;
        try
        {
            File.Copy(inputs.LeftFile, Path.Join(directory, "left"));
            File.Copy(inputs.BaseFile, Path.Join(directory, "base"));
            File.Copy(inputs.RightFile, Path.Join(directory, "right"));
            var result = await git.RunAsync(new GitInvocation(directory, ["merge-file", "-p", "--diff3", "left", "base", "right"]), default);
            Assert.True(result.ExitCode >= 0, result.ErrorText);
            var markers = Encoding.UTF8.GetString(result.StandardOutput).Split('\n')
                .Count(line => line.StartsWith("<<<<<<<", StringComparison.Ordinal) && (line.Length == 7 || line[7] == ' '));
            Assert.Equal(markers, plan.ConflictCount);
            Assert.Equal(1, markers);
        }
        finally
        {
            Directory.Delete(directory, recursive: true);
        }
    }

    [Fact]
    public async Task Differing_boms_plan_in_process_and_still_tile_every_side()
    {
        var (b, l, r) = Criterion1();
        var inputs = await Inputs.FromTextsAsync(b, l, r, TextEncodingKind.Utf8Bom, TextEncodingKind.Utf8);
        var plan = await MergePlanning.PlanAsync(Fixtures.Git(), inputs, default);
        Assert.Equal(1, plan.ConflictCount);
        Assert.Equal(inputs.Left.Lines.Length, plan.Regions.Sum(region => region.Left.Length));
        Assert.Equal(inputs.Right.Lines.Length, plan.Regions.Sum(region => region.Right.Length));
        Assert.Equal(inputs.Base.Lines.Length, plan.Regions.Sum(region => region.Base.Length));
    }

    [Fact]
    public async Task An_absent_base_plans_both_sides_as_additions_and_leaves_no_temp_files()
    {
        var before = TempDirectories();
        var inputs = await Inputs.FromTextsAsync(null, "same\nleft\n", "same\nright\n");
        var plan = await MergePlanning.PlanAsync(Fixtures.Git(), inputs, default);
        Assert.Equal(1, plan.ConflictCount);
        Assert.Equal(before, TempDirectories());
    }

    [Fact]
    public async Task No_temp_files_are_left_behind_when_planning_is_cancelled()
    {
        var before = TempDirectories();
        var inputs = await Inputs.FromTextsAsync(null, "a\n", "b\n");
        using var cancelled = new CancellationTokenSource();
        await cancelled.CancelAsync();
        await Assert.ThrowsAnyAsync<OperationCanceledException>(() => MergePlanning.PlanAsync(Fixtures.Git(), inputs, cancelled.Token));
        Assert.Equal(before, TempDirectories());
    }

    private static string[] TempDirectories() =>
        Directory.GetDirectories(Path.GetTempPath(), "cthulhu-resolve-*").Order(StringComparer.Ordinal).ToArray();
}
