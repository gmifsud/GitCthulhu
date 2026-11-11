using System.Text;
using Resolve.Core.Cli;
using Resolve.Core.Diff;
using Resolve.Core.Git;
using Resolve.Core.Merge;
using Resolve.Core.Session;
using Resolve.Core.Tests.Support;

namespace Resolve.Core.Tests;

/// <summary>A git that cannot read an input is an error, never "no changes": a silent all-Equal plan would save BASE over both sides.</summary>
public sealed class PlanningFailureTests
{
    /// <summary>Lets everything through except <c>git diff</c>, which fails the way git does for an unreadable path.</summary>
    private sealed class UnreadableDiffRunner(IGitRunner inner) : IGitRunner
    {
        public Task<GitResult> RunAsync(GitInvocation invocation, CancellationToken cancellationToken) =>
            invocation.Arguments.Contains("diff")
                ? Task.FromResult(new GitResult(1, [], Encoding.UTF8.GetBytes("error: Could not access 'x_BASE_1.txt'\n")))
                : inner.RunAsync(invocation, cancellationToken);
    }

    /// <summary>Answers every <c>git diff</c> with "identical", as git does when an attribute converts away the only differences.</summary>
    private sealed class IdenticalDiffRunner(IGitRunner inner) : IGitRunner
    {
        public Task<GitResult> RunAsync(GitInvocation invocation, CancellationToken cancellationToken) =>
            invocation.Arguments.Contains("diff")
                ? Task.FromResult(new GitResult(0, [], []))
                : inner.RunAsync(invocation, cancellationToken);
    }

    [Fact]
    public async Task A_diff_over_a_missing_path_throws()
    {
        var directory = Directory.CreateTempSubdirectory("cthulhu-missing-").FullName;
        var present = Path.Join(directory, "present");
        await File.WriteAllTextAsync(present, "a\n");
        await Assert.ThrowsAsync<GitCommandException>(() =>
            GitLineDiff.ComputeAsync(Fixtures.Git(), directory, Path.Join(directory, "missing"), present, DiffAlgorithm.Histogram, default));
    }

    [Fact]
    public async Task Opening_when_git_cannot_diff_an_input_fails_as_input_unreadable()
    {
        using var fx = await Fixtures.Criterion1ConflictAsync();
        var arguments = await MergeToolCall.WriteStageFilesAsync(fx, "a.txt");

        var outcome = await MergeToolSession.OpenAsync(new UnreadableDiffRunner(fx.Git), arguments, Fixtures.Options(), default);

        var failed = Assert.IsType<OpenOutcome.Failed>(outcome);
        Assert.Equal(ExitCode.InputUnreadable, failed.Code);
    }

    [Fact]
    public void An_edit_script_that_does_not_match_its_inputs_never_becomes_a_plan()
    {
        int[] baseIds = [1, 2, 3];
        int[] left = [1, 9, 3];
        int[] right = [1, 2, 3];
        // Wrong net length, unsorted, out of bounds: each throws from Plan and is null from TryPlan.
        Assert.Throws<ArgumentException>(() => Diff3Planner.Plan(baseIds, [1, 2, 3, 4], right, [], [], DiffAlgorithm.Histogram));
        Assert.Throws<ArgumentException>(() => Diff3Planner.Plan(baseIds, left, right, [new Edit(1, 2, 1, 2), new Edit(0, 1, 0, 1)], [], DiffAlgorithm.Histogram));
        Assert.Throws<ArgumentException>(() => Diff3Planner.Plan(baseIds, left, right, [new Edit(2, 9, 1, 2)], [], DiffAlgorithm.Histogram));
        Assert.Null(Diff3Planner.TryPlan(baseIds, left, right, [new Edit(2, 9, 1, 2)], [], DiffAlgorithm.Histogram));
        Assert.NotNull(Diff3Planner.Plan(baseIds, left, right, [new Edit(1, 2, 1, 2)], [], DiffAlgorithm.Histogram));
    }

    [Fact]
    public void A_script_whose_equal_stretches_differ_is_not_valid()   // ruling R28
    {
        int[] baseIds = [1, 2, 3, 4];
        int[] left = [1, 9, 3, 4];
        // The counts match, but the stretch the script calls equal holds 2 on BASE and 9 on LEFT.
        Assert.False(Diff3Planner.IsValidScript([], baseIds, left));
        Assert.False(Diff3Planner.IsValidScript([new Edit(3, 4, 3, 4)], baseIds, left));
        // An edit whose side position disagrees with the stretch before it.
        Assert.False(Diff3Planner.IsValidScript([new Edit(1, 2, 2, 3)], baseIds, [1, 2, 9, 4]));
        Assert.Null(Diff3Planner.TryPlan(baseIds, left, baseIds, [], [], DiffAlgorithm.Histogram));
        Assert.True(Diff3Planner.IsValidScript([new Edit(1, 2, 1, 2)], baseIds, left));
    }

    [Fact]
    public async Task A_git_script_that_hides_a_change_falls_back_to_the_in_process_plan()   // ruling R28
    {
        var inputs = await Inputs.FromTextsAsync("a\nb\nc\nd\ne\n", "a\nLEFT\nc\nd\ne\n", "a\nb\nc\nd\nRIGHT\n");

        var plan = await MergePlanning.PlanAsync(new IdenticalDiffRunner(Fixtures.Git()), inputs, default);

        var expected = Diff3Planner.Plan(inputs.Base.Lines, inputs.Left.Lines, inputs.Right.Lines, DiffAlgorithm.Histogram);
        Assert.Equal(expected.Digest, plan.Digest);
        Assert.Equal(expected.Regions.Select(region => (region.Kind, region.Base, region.Left, region.Right)), plan.Regions.Select(region => (region.Kind, region.Base, region.Left, region.Right)));
        Assert.Equal(2, plan.ChangeCount);
    }

    [Fact]
    public async Task Relative_input_paths_plan_from_another_current_directory()
    {
        var inputs = await Inputs.FromTextsAsync("a\nb\nc\nd\ne\n", "a\nLEFT\nc\nd\ne\n", "a\nb\nc\nd\nRIGHT\n");
        var parent = Path.GetDirectoryName(inputs.LeftFile)!;
        var folder = Path.GetFileName(parent);
        var relative = inputs with
        {
            BaseFile = Path.Join(folder, "BASE"),
            LeftFile = Path.Join(folder, "LOCAL"),
            RightFile = Path.Join(folder, "REMOTE"),
        };
        var previous = Environment.CurrentDirectory;
        Environment.CurrentDirectory = Path.GetDirectoryName(parent)!;
        try
        {
            var plan = await MergePlanning.PlanAsync(Fixtures.Git(), relative, default);
            Assert.Equal(2, plan.ChangeCount);
            Assert.Contains(plan.Regions, region => region.Kind == RegionKind.LeftOnly);
        }
        finally
        {
            Environment.CurrentDirectory = previous;
        }
    }
}
