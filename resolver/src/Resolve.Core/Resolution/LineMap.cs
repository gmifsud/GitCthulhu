using Resolve.Core.Merge;

namespace Resolve.Core.Resolution;

/// <summary>
/// A monotone piecewise-linear map between a side pane's line space and the result's (spec §10.5): one to one across
/// equal runs, proportional across a block, and a block with an empty range on one side maps its whole range on the
/// other side to that single position. Lines are fractional so sub-line scroll positions follow. Values outside
/// the pane clamp to its ends; the ends are pinned: <c>ToResult(0)</c> is 0 and <c>ToResult(sideLines)</c> is the result length.
/// </summary>
public sealed class LineMap
{
    private readonly (LineRange Side, LineRange Result)[] _blocks;

    // Segments tile each space; those with a zero-length range on the queried side are left out of that side's search.
    private readonly (LineRange Side, LineRange Result)[] _bySide;
    private readonly (LineRange Side, LineRange Result)[] _byResult;
    private readonly int _sideLines;
    private readonly int _resultLines;

    private LineMap((LineRange Side, LineRange Result)[] blocks, List<(LineRange Side, LineRange Result)> segments, int sideLines, int resultLines)
    {
        _blocks = blocks;
        _bySide = [.. segments.Where(segment => !segment.Side.IsEmpty)];
        _byResult = [.. segments.Where(segment => !segment.Result.IsEmpty)];
        _sideLines = sideLines;
        _resultLines = resultLines;
    }

    /// <summary>The non-equal blocks this map was built from, in order.</summary>
    public IReadOnlyList<(LineRange Side, LineRange Result)> Blocks => _blocks;

    /// <summary>
    /// Builds the map from the non-<c>Equal</c> blocks in order; the equal runs between and around them are filled in
    /// one to one. Throws <see cref="ArgumentException"/> if the blocks are unordered, overlap, leave equal runs
    /// of unequal length, or exceed <paramref name="sideLines"/> / <paramref name="resultLines"/>.
    /// </summary>
    public static LineMap Build(IReadOnlyList<(LineRange Side, LineRange Result)> blocks, int sideLines, int resultLines)
    {
        ArgumentNullException.ThrowIfNull(blocks);
        ArgumentOutOfRangeException.ThrowIfNegative(sideLines);
        ArgumentOutOfRangeException.ThrowIfNegative(resultLines);
        var segments = new List<(LineRange Side, LineRange Result)>();
        int side = 0, result = 0;
        foreach (var (blockSide, blockResult) in blocks)
        {
            if (blockSide.Length < 0 || blockResult.Length < 0 || blockSide.Start < side || blockResult.Start < result
                || blockSide.End > sideLines || blockResult.End > resultLines)
            {
                throw new ArgumentException($"Block {blockSide} -> {blockResult} is out of order, overlapping or out of range.", nameof(blocks));
            }
            AddEqualRun(segments, side, blockSide.Start, result, blockResult.Start);
            if (!blockSide.IsEmpty || !blockResult.IsEmpty)
            {
                segments.Add((blockSide, blockResult));
            }
            side = blockSide.End;
            result = blockResult.End;
        }
        AddEqualRun(segments, side, sideLines, result, resultLines);
        return new LineMap([.. blocks], segments, sideLines, resultLines);
    }

    public double ToResult(double sideLine) =>
        Map(sideLine, _sideLines, _resultLines, _bySide, static segment => segment.Side, static segment => segment.Result);

    public double ToSide(double resultLine) =>
        Map(resultLine, _resultLines, _sideLines, _byResult, static segment => segment.Result, static segment => segment.Side);

    private static void AddEqualRun(List<(LineRange Side, LineRange Result)> segments, int sideFrom, int sideTo, int resultFrom, int resultTo)
    {
        if (sideTo - sideFrom != resultTo - resultFrom)
        {
            throw new ArgumentException($"The equal run between sides [{sideFrom},{sideTo}) and results [{resultFrom},{resultTo}) has unequal lengths.");
        }
        if (sideTo > sideFrom)
        {
            segments.Add((new LineRange(sideFrom, sideTo), new LineRange(resultFrom, resultTo)));
        }
    }

    /// <summary>
    /// Maps <paramref name="line"/> from the "from" space to the "to" space. <paramref name="segments"/> are those with a
    /// non-empty "from" range, so the one holding <paramref name="line"/> is found by binary search on its start.
    /// </summary>
    private static double Map(
        double line,
        int fromLines,
        int toLines,
        (LineRange Side, LineRange Result)[] segments,
        Func<(LineRange Side, LineRange Result), LineRange> from,
        Func<(LineRange Side, LineRange Result), LineRange> to)
    {
        // An empty "from" pane has one position, which is the end of the other pane.
        if (line >= fromLines)
        {
            return toLines;
        }
        if (!(line > 0))
        {
            return 0;
        }
        int low = 0, high = segments.Length - 1;
        while (low < high)
        {
            var middle = (low + high + 1) / 2;
            if (from(segments[middle]).Start <= line)
            {
                low = middle;
            }
            else
            {
                high = middle - 1;
            }
        }
        var source = from(segments[low]);
        var target = to(segments[low]);
        return target.IsEmpty ? target.Start : target.Start + ((line - source.Start) * target.Length / source.Length);
    }
}

/// <summary>The two maps the window needs: LEFT↔RESULT and RIGHT↔RESULT, derived from the plan and the live document.</summary>
public static class PaneMaps
{
    /// <summary>
    /// Pairs every plan region's range on a side with its current lines in the result. Non-equal regions are blocks,
    /// and so is an equal region whose lines no longer match one to one. No caching: this is O(regions).
    /// Never throws for any state of a document built from <paramref name="plan"/> (ruling R27): two regions can share
    /// a line in the result when a boundary sits mid-line, so the result ranges are first made a monotone tiling.
    /// </summary>
    public static (LineMap Left, LineMap Right) For(MergePlan plan, ResolutionDocument document)
    {
        ArgumentNullException.ThrowIfNull(plan);
        ArgumentNullException.ThrowIfNull(document);
        var views = document.Regions;
        if (views.Count != plan.Regions.Length)
        {
            throw new ArgumentException("The document does not belong to this plan.", nameof(document));
        }
        for (var index = 0; index < views.Count; index++)
        {
            if (views[index].RegionId != plan.Regions[index].Id)
            {
                throw new ArgumentException("The document does not belong to this plan.", nameof(document));
            }
        }
        var resultLines = LineCount(document.Text);
        var bounds = ResultBounds(views, resultLines);
        return (Build(plan, bounds, resultLines, static region => region.Left), Build(plan, bounds, resultLines, static region => region.Right));
    }

    /// <summary>
    /// Region <c>i</c> owns result lines [<c>bounds[i]</c>, <c>bounds[i + 1]</c>): a line shared by two regions belongs to
    /// the later one, and an empty region sits where the next non-empty one starts. Monotone by construction.
    /// </summary>
    private static int[] ResultBounds(IReadOnlyList<RegionView> views, int resultLines)
    {
        var bounds = new int[views.Count + 1];
        var next = resultLines;
        bounds[views.Count] = resultLines;
        for (var index = views.Count - 1; index >= 1; index--)
        {
            var view = views[index];
            if (view.End > view.Start)
            {
                next = Math.Clamp(view.ResultLines.Start, 0, next);
            }
            bounds[index] = next;
        }
        return bounds;
    }

    /// <summary>
    /// Side ranges are clamped into a gap-free tiling too, so every stretch between two blocks is a run of equal regions
    /// of matching lengths on both sides — exactly what <see cref="LineMap.Build"/> accepts.
    /// </summary>
    private static LineMap Build(MergePlan plan, int[] bounds, int resultLines, Func<PlanRegion, LineRange> sideOf)
    {
        var blocks = new List<(LineRange Side, LineRange Result)>();
        var sideEnd = 0;
        for (var index = 0; index < plan.Regions.Length; index++)
        {
            var planned = plan.Regions[index];
            var range = sideOf(planned);
            var start = Math.Max(range.Start, sideEnd);
            var side = new LineRange(start, Math.Max(range.End, start));
            var result = new LineRange(bounds[index], bounds[index + 1]);
            if (planned.Kind != RegionKind.Equal || side.Length != result.Length)
            {
                blocks.Add((side, result));
            }
            sideEnd = side.End;
        }
        if (plan.Regions.Length == 0 && resultLines > 0)
        {
            // Nothing was planned (three empty inputs) but text was typed: the whole result is one block.
            blocks.Add((new LineRange(0, 0), new LineRange(0, resultLines)));
        }
        return LineMap.Build(blocks, sideEnd, resultLines);
    }

    private static int LineCount(string text) =>
        text.Length == 0 ? 0 : text.AsSpan().Count('\n') + (text[^1] == '\n' ? 0 : 1);
}
