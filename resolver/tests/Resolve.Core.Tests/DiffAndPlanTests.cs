using CsCheck;
using Resolve.Core.Diff;
using Resolve.Core.Merge;

namespace Resolve.Core.Tests;

public class DiffAndPlanTests
{
    private static readonly Gen<int[]> SmallAlphabet = Gen.Int[0, 4].Array[0, 40];

    public static TheoryData<DiffAlgorithm> Algorithms => [DiffAlgorithm.Myers, DiffAlgorithm.Histogram];

    [Theory]
    [MemberData(nameof(Algorithms))]
    public void A_diff_transforms_a_into_b(DiffAlgorithm algorithm) =>
        Gen.Select(SmallAlphabet, SmallAlphabet).Sample((a, b) =>
            LineDiff.Apply(a, b, LineDiff.Compute(a, b, algorithm)).SequenceEqual(b));

    [Theory]
    [MemberData(nameof(Algorithms))]
    public void Edits_are_sorted_maximal_and_separated_by_equal_lines(DiffAlgorithm algorithm) =>
        Gen.Select(SmallAlphabet, SmallAlphabet).Sample((a, b) =>
        {
            var edits = LineDiff.Compute(a, b, algorithm);
            for (var index = 1; index < edits.Count; index++)
            {
                var gapA = edits[index].AStart - edits[index - 1].AEnd;
                var gapB = edits[index].BStart - edits[index - 1].BEnd;
                if (gapA < 1 || gapA != gapB)
                {
                    return false;
                }
            }
            return true;
        });

    [Fact]
    public void Myers_is_minimal_on_a_textbook_case()
    {
        // ABCABBA → CBABAC: shortest edit script has 5 operations (Myers 1986, fig. 1).
        int[] a = [0, 1, 2, 0, 1, 1, 0];
        int[] b = [2, 1, 0, 1, 0, 2];
        var edits = LineDiff.Compute(a, b, DiffAlgorithm.Myers);
        Assert.Equal(5, edits.Sum(edit => (edit.AEnd - edit.AStart) + (edit.BEnd - edit.BStart)));
    }

    [Theory]
    [MemberData(nameof(Algorithms))]
    public void Regions_tile_all_three_inputs_and_rebuild_each_side(DiffAlgorithm algorithm) =>
        Gen.Select(SmallAlphabet, SmallAlphabet, SmallAlphabet).Sample((baseIds, left, right) =>
        {
            var plan = Diff3Planner.Plan(baseIds, left, right, algorithm);
            int b = 0, l = 0, r = 0;
            var rebuiltLeft = new List<int>();
            var rebuiltRight = new List<int>();
            foreach (var region in plan.Regions)
            {
                if (region.Base.Start != b || region.Left.Start != l || region.Right.Start != r)
                {
                    return false;
                }
                (b, l, r) = (region.Base.End, region.Left.End, region.Right.End);
                rebuiltLeft.AddRange(left[region.Left.Start..region.Left.End]);
                rebuiltRight.AddRange(right[region.Right.Start..region.Right.End]);
                var baseSlice = baseIds[region.Base.Start..region.Base.End];
                var ok = region.Kind switch
                {
                    RegionKind.Equal => baseSlice.SequenceEqual(left[region.Left.Start..region.Left.End])
                        && baseSlice.SequenceEqual(right[region.Right.Start..region.Right.End]),
                    RegionKind.LeftOnly => baseSlice.SequenceEqual(right[region.Right.Start..region.Right.End]),
                    RegionKind.RightOnly => baseSlice.SequenceEqual(left[region.Left.Start..region.Left.End]),
                    RegionKind.BothSame => left[region.Left.Start..region.Left.End].SequenceEqual(right[region.Right.Start..region.Right.End]),
                    RegionKind.Conflict => !left[region.Left.Start..region.Left.End].SequenceEqual(right[region.Right.Start..region.Right.End]),
                    _ => false,
                };
                if (!ok)
                {
                    return false;
                }
            }
            return b == baseIds.Length && l == left.Length && r == right.Length
                && rebuiltLeft.SequenceEqual(left) && rebuiltRight.SequenceEqual(right);
        });

    [Fact]
    public void Planning_is_deterministic() =>
        Gen.Select(SmallAlphabet, SmallAlphabet, SmallAlphabet).Sample((baseIds, left, right) =>
            Diff3Planner.Plan(baseIds, left, right, DiffAlgorithm.Histogram).Digest
                == Diff3Planner.Plan(baseIds, left, right, DiffAlgorithm.Histogram).Digest);

    [Fact]
    public void Insertions_at_the_same_base_position_conflict()
    {
        var plan = Diff3Planner.Plan([1, 2], [1, 10, 2], [1, 20, 2], DiffAlgorithm.Myers);
        var conflict = Assert.Single(plan.Regions, region => region.Kind == RegionKind.Conflict);
        Assert.True(conflict.Base.IsEmpty);
    }

    [Fact]
    public void Touching_changes_conflict_like_git()
    {
        // Left rewrites line 1; right rewrites line 2: adjacent, no stable line between them.
        var plan = Diff3Planner.Plan([0, 1, 2, 3], [0, 10, 2, 3], [0, 1, 20, 3], DiffAlgorithm.Myers);
        Assert.Equal(1, plan.ConflictCount);
    }

    [Fact]
    public void Identical_changes_are_not_a_conflict()
    {
        var plan = Diff3Planner.Plan([0, 1, 2], [0, 9, 2], [0, 9, 2], DiffAlgorithm.Myers);
        Assert.Equal(0, plan.ConflictCount);
        Assert.Contains(plan.Regions, region => region.Kind == RegionKind.BothSame);
    }

    [Fact]
    public void Shared_edges_of_a_conflict_are_recorded_as_trim()
    {
        // Both sides add [7, 8] around different middles.
        var plan = Diff3Planner.Plan([0, 1, 2], [0, 7, 30, 8, 2], [0, 7, 40, 8, 2], DiffAlgorithm.Myers);
        var conflict = Assert.Single(plan.Regions, region => region.Kind == RegionKind.Conflict);
        Assert.Equal(new ConflictTrim(1, 1), conflict.Trim);
    }

    [Fact]
    public void Histogram_handles_a_large_file_quickly()
    {
        const int lines = 100_000;
        var random = new Random(7);
        var baseIds = Enumerable.Range(0, lines).ToArray();
        var left = (int[])baseIds.Clone();
        var right = (int[])baseIds.Clone();
        for (var index = 0; index < 300; index++)
        {
            left[random.Next(lines)] = lines + index;
            right[random.Next(lines)] = 2 * lines + index;
        }
        var clock = System.Diagnostics.Stopwatch.StartNew();
        var plan = Diff3Planner.Plan(baseIds, left, right, DiffAlgorithm.Histogram);
        clock.Stop();
        Assert.True(plan.ChangeCount > 0);
        // Spec §10.9 target is ≤ 600 ms on a worker; this guard is deliberately loose for shared CI machines.
        Assert.True(clock.ElapsedMilliseconds < 3_000, $"planning took {clock.ElapsedMilliseconds} ms");
    }
}

public class GitLineDiffTests
{
    [Fact]
    public void Hunk_headers_map_to_zero_based_edits()
    {
        var diff = "diff --git a/b b/b\n@@ -2 +2 @@\n-x\n+y\n@@ -5,0 +6,2 @@\n+p\n+q\n@@ -9,3 +11,0 @@\n-a\n-b\n-c\n"u8;
        Assert.Equal(
            [new Edit(1, 2, 1, 2), new Edit(5, 5, 5, 7), new Edit(8, 11, 11, 11)],
            GitLineDiff.ParseHunkHeaders(diff));
    }
}
