using Avalonia;
using Avalonia.Controls;
using Avalonia.Media;
using Resolve.App.Editor;
using Resolve.Core.Merge;

namespace Resolve.App.Panes;

/// <summary>One non-equal region as a band sees it: its lines in the side pane, its lines in RESULT, and its tint.</summary>
internal readonly record struct BandBlock(LineRange Side, LineRange Result, RegionTint Tint);

/// <summary>
/// The connector between a side pane and RESULT: one cubic-Bézier quad per non-equal block that is on screen in
/// either pane. Blocks are ordered, so a binary search finds the first candidate and a walk stops at the first block
/// past both viewports: O(log n + visible), never O(blocks) (ADR 0007).
/// </summary>
internal sealed class BandCanvas : Control
{
    private static readonly IPen[] Outlines = [.. Enum.GetValues<RegionTint>().Select(tint => (IPen)new Pen(RegionBackground.InkFor(tint), 1))];

    private readonly IEditorSurface _side;
    private readonly IEditorSurface _result;
    private readonly bool _sideIsLeft;
    private BandBlock[] _blocks = [];

    public BandCanvas(IEditorSurface side, IEditorSurface result, bool sideIsLeft)
    {
        ArgumentNullException.ThrowIfNull(side);
        ArgumentNullException.ThrowIfNull(result);
        _side = side;
        _result = result;
        _sideIsLeft = sideIsLeft;
        ClipToBounds = true;
        IsHitTestVisible = false;
    }

    internal IReadOnlyList<BandBlock> Blocks => _blocks;

    /// <summary>Blocks that would be drawn right now: on screen in the side pane or in RESULT.</summary>
    internal int VisibleBlockCount => VisibleBlocks().Count();

    /// <summary>Replaces the blocks; they must be ordered by both side start and result start.</summary>
    internal void SetBlocks(IReadOnlyList<BandBlock> blocks)
    {
        ArgumentNullException.ThrowIfNull(blocks);
        _blocks = [.. blocks];
        InvalidateVisual();
    }

    public override void Render(DrawingContext context)
    {
        ArgumentNullException.ThrowIfNull(context);
        var sideTop = _side.VerticalOffset;
        var resultTop = _result.VerticalOffset;
        var (sideX, resultX) = _sideIsLeft ? (0.0, Bounds.Width) : (Bounds.Width, 0.0);
        var middle = Bounds.Width / 2;
        foreach (var block in VisibleBlocks())
        {
            var sideStart = _side.LineToY(block.Side.Start) - sideTop;
            var sideEnd = Math.Max(_side.LineToY(block.Side.End) - sideTop, sideStart + 1);
            var resultStart = _result.LineToY(block.Result.Start) - resultTop;
            var resultEnd = Math.Max(_result.LineToY(block.Result.End) - resultTop, resultStart + 1);
            var geometry = new StreamGeometry();
            using (var figure = geometry.Open())
            {
                figure.BeginFigure(new Point(sideX, sideStart), true);
                figure.CubicBezierTo(new Point(middle, sideStart), new Point(middle, resultStart), new Point(resultX, resultStart));
                figure.LineTo(new Point(resultX, resultEnd));
                figure.CubicBezierTo(new Point(middle, resultEnd), new Point(middle, sideEnd), new Point(sideX, sideEnd));
                figure.EndFigure(true);
            }
            context.DrawGeometry(RegionBackground.WashFor(block.Tint), Outlines[(int)block.Tint], geometry);
        }
    }

    private IEnumerable<BandBlock> VisibleBlocks()
    {
        var sideFirst = _side.YToLine(_side.VerticalOffset);
        var sideLast = _side.YToLine(_side.VerticalOffset + _side.ViewportHeight);
        var resultFirst = _result.YToLine(_result.VerticalOffset);
        var resultLast = _result.YToLine(_result.VerticalOffset + _result.ViewportHeight);
        var start = Math.Min(FirstEndingAtOrAfter(sideFirst, static block => block.Side.End), FirstEndingAtOrAfter(resultFirst, static block => block.Result.End));
        for (var index = start; index < _blocks.Length; index++)
        {
            var block = _blocks[index];
            if (block.Side.Start > sideLast && block.Result.Start > resultLast)
            {
                yield break;
            }
            if ((block.Side.End >= sideFirst && block.Side.Start <= sideLast) || (block.Result.End >= resultFirst && block.Result.Start <= resultLast))
            {
                yield return block;
            }
        }
    }

    /// <summary>Binary search: the first block whose end (non-decreasing) is at or after the line.</summary>
    private int FirstEndingAtOrAfter(int line, Func<BandBlock, int> end)
    {
        int low = 0, high = _blocks.Length;
        while (low < high)
        {
            var middle = (low + high) / 2;
            if (end(_blocks[middle]) >= line)
            {
                high = middle;
            }
            else
            {
                low = middle + 1;
            }
        }
        return low;
    }
}
