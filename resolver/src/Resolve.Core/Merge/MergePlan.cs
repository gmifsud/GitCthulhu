using System.Collections.Immutable;
using System.Security.Cryptography;
using System.Text;
using Resolve.Core.Diff;
using Resolve.Core.Text;

namespace Resolve.Core.Merge;

/// <summary>Half-open line range [Start, End).</summary>
public readonly record struct LineRange(int Start, int End)
{
    public int Length => End - Start;

    public bool IsEmpty => Start == End;

    public override string ToString() => $"[{Start},{End})";
}

public enum RegionKind
{
    /// <summary>Unchanged on both sides.</summary>
    Equal,
    LeftOnly,
    RightOnly,
    /// <summary>Both sides made the same change: not a conflict (git's EAGER level).</summary>
    BothSame,
    Conflict,
}

/// <summary>
/// Lines that left and right share at the edges of a conflict (zdiff3's trim). Metadata only: the region keeps
/// its full ranges so the base stays meaningful; "accept both" uses it to avoid duplicating shared lines.
/// </summary>
public readonly record struct ConflictTrim(int CommonPrefix, int CommonSuffix);

public sealed record PlanRegion(int Id, RegionKind Kind, LineRange Base, LineRange Left, LineRange Right)
{
    public ConflictTrim Trim { get; init; }
}

/// <summary>
/// The frozen partition of a merge (ADR 0006). Computed once from immutable stages; region ids never change
/// for the life of a session. The regions tile all three inputs in order with no gaps.
/// </summary>
public sealed record MergePlan(ImmutableArray<PlanRegion> Regions, DiffAlgorithm Algorithm, string Digest)
{
    public int ConflictCount => Regions.Count(region => region.Kind == RegionKind.Conflict);

    public int ChangeCount => Regions.Count(region => region.Kind is RegionKind.LeftOnly or RegionKind.RightOnly or RegionKind.BothSame);
}

/// <summary>
/// Three-way planner (diff3; Khanna, Kunal &amp; Pierce, FSTTCS 2007). Base→left and base→right edits are walked
/// in base order; edits whose base ranges overlap or touch — including insertions at the same base position —
/// form one chunk, as in git's xdl_do_merge. A chunk changed on both sides is BothSame when the two results
/// are identical, otherwise a Conflict. No zealous splitting: git itself caps refinement at EAGER for diff3
/// output because a split conflict has no meaningful base, and this resolver always shows the base.
/// </summary>
public static class Diff3Planner
{
    /// <summary>Bump when planning rules change, so stored sessions detect drift (spec §6.7).</summary>
    public const int PlannerVersion = 1;

    public static MergePlan Plan(IReadOnlyList<Line> baseLines, IReadOnlyList<Line> left, IReadOnlyList<Line> right, DiffAlgorithm algorithm)
    {
        var interner = new LineInterner();
        return Plan(interner.Intern(baseLines), interner.Intern(left), interner.Intern(right), algorithm);
    }

    public static MergePlan Plan(int[] baseIds, int[] left, int[] right, DiffAlgorithm algorithm)
    {
        ArgumentNullException.ThrowIfNull(baseIds);
        ArgumentNullException.ThrowIfNull(left);
        ArgumentNullException.ThrowIfNull(right);
        return Plan(baseIds, left, right, LineDiff.Compute(baseIds, left, algorithm), LineDiff.Compute(baseIds, right, algorithm), algorithm);
    }

    /// <summary>
    /// Plans from given edit scripts, e.g. git's own (<see cref="GitLineDiff"/>), so the regions are exactly the
    /// conflicts git reports. Edits must be sorted, maximal and valid for these inputs.
    /// </summary>
    public static MergePlan Plan(int[] baseIds, int[] left, int[] right, IReadOnlyList<Edit> leftEdits, IReadOnlyList<Edit> rightEdits, DiffAlgorithm algorithm)
    {
        ArgumentNullException.ThrowIfNull(baseIds);
        ArgumentNullException.ThrowIfNull(left);
        ArgumentNullException.ThrowIfNull(right);
        ArgumentNullException.ThrowIfNull(leftEdits);
        ArgumentNullException.ThrowIfNull(rightEdits);
        var regions = ImmutableArray.CreateBuilder<PlanRegion>();
        int i = 0, j = 0, basePosition = 0, leftDelta = 0, rightDelta = 0;

        void AddEqual(int end)
        {
            if (end > basePosition)
            {
                regions.Add(new PlanRegion(regions.Count, RegionKind.Equal,
                    new LineRange(basePosition, end),
                    new LineRange(basePosition + leftDelta, end + leftDelta),
                    new LineRange(basePosition + rightDelta, end + rightDelta)));
            }
        }

        while (i < leftEdits.Count || j < rightEdits.Count)
        {
            int leftFirst = i, rightFirst = j;
            var startLeft = j >= rightEdits.Count || (i < leftEdits.Count && leftEdits[i].AStart <= rightEdits[j].AStart);
            var first = startLeft ? leftEdits[i++] : rightEdits[j++];
            var groupStart = first.AStart;
            var groupEnd = first.AEnd;
            bool progressed;
            do
            {
                // "<=" makes touching edits and insertions at the same base position one chunk,
                // matching xdl_do_merge's overlap test (next.start <= current.end).
                progressed = false;
                if (i < leftEdits.Count && leftEdits[i].AStart <= groupEnd)
                {
                    groupEnd = Math.Max(groupEnd, leftEdits[i++].AEnd);
                    progressed = true;
                }
                if (j < rightEdits.Count && rightEdits[j].AStart <= groupEnd)
                {
                    groupEnd = Math.Max(groupEnd, rightEdits[j++].AEnd);
                    progressed = true;
                }
            }
            while (progressed);

            AddEqual(groupStart);
            var leftChanged = i > leftFirst;
            var rightChanged = j > rightFirst;
            var leftGroupDelta = Sum(leftEdits, leftFirst, i);
            var rightGroupDelta = Sum(rightEdits, rightFirst, j);
            var baseRange = new LineRange(groupStart, groupEnd);
            var leftRange = new LineRange(groupStart + leftDelta, groupEnd + leftDelta + leftGroupDelta);
            var rightRange = new LineRange(groupStart + rightDelta, groupEnd + rightDelta + rightGroupDelta);
            var kind = (leftChanged, rightChanged) switch
            {
                (true, false) => RegionKind.LeftOnly,
                (false, true) => RegionKind.RightOnly,
                _ => SameLines(left, leftRange, right, rightRange) ? RegionKind.BothSame : RegionKind.Conflict,
            };
            var region = new PlanRegion(regions.Count, kind, baseRange, leftRange, rightRange);
            if (kind == RegionKind.Conflict)
            {
                region = region with { Trim = TrimOf(left, leftRange, right, rightRange) };
            }
            regions.Add(region);
            leftDelta += leftGroupDelta;
            rightDelta += rightGroupDelta;
            basePosition = groupEnd;
        }
        AddEqual(baseIds.Length);
        var plan = regions.ToImmutable();
        return new MergePlan(plan, algorithm, DigestOf(plan, algorithm));
    }

    private static int Sum(IReadOnlyList<Edit> edits, int from, int to)
    {
        var total = 0;
        for (var index = from; index < to; index++)
        {
            total += edits[index].Delta;
        }
        return total;
    }

    private static bool SameLines(int[] left, LineRange leftRange, int[] right, LineRange rightRange) =>
        left.AsSpan(leftRange.Start, leftRange.Length).SequenceEqual(right.AsSpan(rightRange.Start, rightRange.Length));

    private static ConflictTrim TrimOf(int[] left, LineRange leftRange, int[] right, LineRange rightRange)
    {
        var shorter = Math.Min(leftRange.Length, rightRange.Length);
        var prefix = 0;
        while (prefix < shorter && left[leftRange.Start + prefix] == right[rightRange.Start + prefix])
        {
            prefix++;
        }
        var suffix = 0;
        while (suffix < shorter - prefix && left[leftRange.End - 1 - suffix] == right[rightRange.End - 1 - suffix])
        {
            suffix++;
        }
        return new ConflictTrim(prefix, suffix);
    }

    private static string DigestOf(ImmutableArray<PlanRegion> regions, DiffAlgorithm algorithm)
    {
        var text = new StringBuilder().Append(PlannerVersion).Append('|').Append(algorithm);
        foreach (var region in regions)
        {
            text.Append('|').Append((int)region.Kind)
                .Append(':').Append(region.Base).Append(region.Left).Append(region.Right);
        }
        return "sha256:" + Convert.ToHexStringLower(SHA256.HashData(Encoding.UTF8.GetBytes(text.ToString())));
    }
}
