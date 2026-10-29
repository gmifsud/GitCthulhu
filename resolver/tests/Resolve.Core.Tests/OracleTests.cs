using System.Text;
using Resolve.Core.Diff;
using Resolve.Core.Git;
using Resolve.Core.Merge;
using Resolve.Core.Tests.Support;
using Resolve.Core.Text;
using Xunit.Abstractions;

namespace Resolve.Core.Tests;

/// <summary>
/// Spike S2: the planner against <c>git merge-file -p --diff3</c>. Fixed seeds, so the figures in
/// docs/spikes/S2-planner-oracle.md reproduce exactly.
/// </summary>
public class OracleTests(ITestOutputHelper output)
{
    private static readonly MarkerLabels Labels = new("L", "B", "R");

    /// <summary>Unambiguous inputs (every line unique): the LCS is unique, so any correct planner must agree with git exactly.</summary>
    [Theory]
    [InlineData(DiffAlgorithm.Myers)]
    [InlineData(DiffAlgorithm.Histogram)]
    public async Task Unique_lines_match_git_exactly(DiffAlgorithm algorithm)
    {
        var random = new Random(20261027);
        var mismatches = new List<string>();
        for (var trial = 0; trial < 150; trial++)
        {
            var fresh = 1000;
            var baseText = Enumerable.Range(0, random.Next(1, 30)).Select(index => $"line {index}").ToArray();
            var left = Mutate(baseText, random, ref fresh);
            var right = Mutate(baseText, random, ref fresh);
            var (ours, git, _, _) = await CompareAsync(baseText, left, right, algorithm);
            if (ours != git)
            {
                mismatches.Add($"trial {trial}:\n--- ours\n{ours}\n--- git\n{git}");
            }
        }
        Assert.True(mismatches.Count == 0, string.Join("\n\n", mismatches.Take(2)));
    }

    /// <summary>
    /// Ambiguous inputs (4-letter alphabet, many repeats) with the in-process diffs. Equal-cost alignments differ
    /// from xdiff's, so agreement is partial by nature: measured on git 2.43 at Myers 76.5 % / 49.0 % and
    /// Histogram 67.0 % / 41.2 % (count / byte-exact). This is why plans are built from git's own diffs
    /// (<see cref="GitLineDiff"/>) and the in-process diffs are display-only. Floors sit a little under the
    /// measured rates to catch regressions while tolerating other git versions' oracle output.
    /// </summary>
    [Theory]
    [InlineData(DiffAlgorithm.Myers, 0.70, 0.45)]
    [InlineData(DiffAlgorithm.Histogram, 0.60, 0.38)]
    public async Task Ambiguous_inputs_mostly_agree_with_git(DiffAlgorithm algorithm, double countFloor, double exactFloor)
    {
        var random = new Random(42);
        int trials = 400, countAgree = 0, exactAgree = 0;
        for (var trial = 0; trial < trials; trial++)
        {
            var baseText = Enumerable.Range(0, random.Next(1, 25)).Select(_ => ((char)('a' + random.Next(4))).ToString()).ToArray();
            var fresh = 0;
            var left = MutateAlphabet(baseText, random, ref fresh);
            var right = MutateAlphabet(baseText, random, ref fresh);
            var (ours, git, ourCount, gitCount) = await CompareAsync(baseText, left, right, algorithm);
            countAgree += ourCount == gitCount ? 1 : 0;
            exactAgree += ours == git ? 1 : 0;
        }
        var countRate = (double)countAgree / trials;
        var exactRate = (double)exactAgree / trials;
        output.WriteLine($"S2 {algorithm}: conflict-count agreement {countRate:P1}, byte-exact agreement {exactRate:P1} over {trials} trials");
        Assert.True(countRate >= countFloor, $"count agreement {countRate:P1} < {countFloor:P0}");
        Assert.True(exactRate >= exactFloor, $"exact agreement {exactRate:P1} < {exactFloor:P0}");
    }

    private static async Task<(string Ours, string Git, int OurCount, int GitCount)> CompareAsync(
        string[] baseText, string[] left, string[] right, DiffAlgorithm algorithm, bool useGitDiffs = false)
    {
        var baseLines = ToLines(baseText);
        var leftLines = ToLines(left);
        var rightLines = ToLines(right);
        var directory = Directory.CreateTempSubdirectory("cthulhu-oracle-").FullName;
        try
        {
            var paths = new[] { "l", "b", "r" }.Select(name => Path.Join(directory, name)).ToArray();
            await File.WriteAllTextAsync(paths[0], Join(left));
            await File.WriteAllTextAsync(paths[1], Join(baseText));
            await File.WriteAllTextAsync(paths[2], Join(right));
            var plan = useGitDiffs
                ? await PlanFromGitDiffsAsync(directory, paths[1], paths[0], paths[2], baseLines, leftLines, rightLines, algorithm)
                : Diff3Planner.Plan(baseLines, leftLines, rightLines, algorithm);
            var ours = MarkerRenderer.RenderDiff3(plan, baseLines, leftLines, rightLines, Labels);
            var result = await new GitRunner().RunAsync(
                new GitInvocation(directory, ["merge-file", "-p", "--diff3", "-L", "L", "-L", "B", "-L", "R", paths[0], paths[1], paths[2]]),
                CancellationToken.None);
            Assert.True(result.ExitCode >= 0, result.ErrorText);
            return (ours, Encoding.UTF8.GetString(result.StandardOutput), plan.ConflictCount, result.ExitCode);
        }
        finally
        {
            Directory.Delete(directory, recursive: true);
        }
    }

    private static async Task<MergePlan> PlanFromGitDiffsAsync(
        string directory, string basePath, string leftPath, string rightPath, Line[] baseLines, Line[] leftLines, Line[] rightLines, DiffAlgorithm algorithm)
    {
        var git = new GitRunner();
        var leftEdits = await GitLineDiff.ComputeAsync(git, directory, basePath, leftPath, algorithm, CancellationToken.None);
        var rightEdits = await GitLineDiff.ComputeAsync(git, directory, basePath, rightPath, algorithm, CancellationToken.None);
        var interner = new LineInterner();
        return Diff3Planner.Plan(interner.Intern(baseLines), interner.Intern(leftLines), interner.Intern(rightLines), leftEdits, rightEdits, algorithm);
    }

    /// <summary>
    /// Separates planner semantics from diff alignment: fed git's own edit scripts, the planner must reproduce
    /// <c>git merge-file --diff3</c> byte for byte on the same ambiguous inputs.
    /// </summary>
    [Fact]
    public async Task Given_gits_diffs_the_planner_matches_merge_file_exactly()
    {
        var random = new Random(42);
        var mismatches = new List<string>();
        for (var trial = 0; trial < 400; trial++)
        {
            var baseText = Enumerable.Range(0, random.Next(1, 25)).Select(_ => ((char)('a' + random.Next(4))).ToString()).ToArray();
            var fresh = 0;
            var left = MutateAlphabet(baseText, random, ref fresh);
            var right = MutateAlphabet(baseText, random, ref fresh);
            var (ours, git, _, _) = await CompareAsync(baseText, left, right, DiffAlgorithm.Myers, useGitDiffs: true);
            if (ours != git)
            {
                mismatches.Add($"#{trial} base={string.Concat(baseText)} left={string.Concat(left)} right={string.Concat(right)}\n{ours}---git---\n{git}");
            }
        }
        Assert.True(mismatches.Count == 0, $"{mismatches.Count} mismatches; first:\n{mismatches.FirstOrDefault()}");
    }

    /// <summary>
    /// Which diff does a real <c>git merge</c> (ort) use for file content? Measured against plans built from
    /// git's Myers and Histogram diffs, comparing marker structure (labels differ: HEAD/branch vs L/B/R).
    /// </summary>
    [Fact]
    public async Task Git_merge_content_alignment_survey()
    {
        var random = new Random(7);
        int trials = 120, conflicted = 0, myersAgree = 0, histogramAgree = 0;
        for (var trial = 0; trial < trials; trial++)
        {
            var baseText = Enumerable.Range(0, random.Next(3, 25)).Select(_ => ((char)('a' + random.Next(4))).ToString()).ToArray();
            var fresh = 0;
            var left = MutateAlphabet(baseText, random, ref fresh);
            var right = MutateAlphabet(baseText, random, ref fresh);
            using var fixture = await GitFixture.CreateAsync();
            await fixture.RunAsync("config", "merge.conflictStyle", "diff3");
            var exit = await fixture.ConflictAsync(
                f => f.Write("f.txt", Join(baseText)), f => f.Write("f.txt", Join(left)), f => f.Write("f.txt", Join(right)));
            if (exit == 0)
            {
                continue; // merged cleanly: no markers to compare
            }
            conflicted++;
            var merged = Normalise(await File.ReadAllTextAsync(Path.Join(fixture.Worktree, "f.txt")));
            var (viaMyers, _, _, _) = await CompareAsync(baseText, left, right, DiffAlgorithm.Myers, useGitDiffs: true);
            var (viaHistogram, _, _, _) = await CompareAsync(baseText, left, right, DiffAlgorithm.Histogram, useGitDiffs: true);
            myersAgree += Normalise(viaMyers) == merged ? 1 : 0;
            histogramAgree += Normalise(viaHistogram) == merged ? 1 : 0;
        }
        var git = await GitVersion.ProbeAsync(new GitRunner(), CancellationToken.None);
        output.WriteLine($"S2 git merge {git}: {conflicted} conflicted merges; matches Myers plan {myersAgree}, Histogram plan {histogramAgree}");
        Assert.True(conflicted > 20);
        Assert.True(Math.Max(myersAgree, histogramAgree) == conflicted, "neither algorithm reproduces git merge exactly");
    }

    /// <summary>Marker lines keep only their seven marker characters; labels vary by tool.</summary>
    private static string Normalise(string text) => string.Join('\n', text.Split('\n').Select(line =>
        line.Length >= 7 && (line.StartsWith("<<<<<<<", StringComparison.Ordinal) || line.StartsWith("|||||||", StringComparison.Ordinal) || line.StartsWith(">>>>>>>", StringComparison.Ordinal))
            ? line[..7]
            : line));

    private static Line[] ToLines(string[] text) => text.Select(line => new Line(line, LineEnding.Lf)).ToArray();

    private static string Join(string[] lines) => string.Concat(lines.Select(line => line + "\n"));

    private static string[] Mutate(string[] source, Random random, ref int fresh)
    {
        var result = new List<string>();
        foreach (var line in source)
        {
            switch (random.Next(10))
            {
                case 0:
                    break; // delete
                case 1:
                    result.Add($"new {fresh++}"); // replace
                    break;
                case 2:
                    result.Add(line);
                    result.Add($"new {fresh++}"); // insert after
                    break;
                default:
                    result.Add(line);
                    break;
            }
        }
        return [.. result];
    }

    private static string[] MutateAlphabet(string[] source, Random random, ref int fresh)
    {
        var result = new List<string>();
        foreach (var line in source)
        {
            switch (random.Next(8))
            {
                case 0:
                    break;
                case 1:
                    result.Add(((char)('a' + random.Next(4))).ToString());
                    break;
                case 2:
                    result.Add(line);
                    result.Add(((char)('a' + random.Next(4))).ToString());
                    break;
                default:
                    result.Add(line);
                    break;
            }
        }
        fresh++;
        return [.. result];
    }
}
