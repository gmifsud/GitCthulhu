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

    /// <summary>Where a band's four corners sit on the canvas, each clamped to the canvas height.</summary>
    internal readonly record struct Quad(double SideStart, double SideEnd, double ResultStart, double ResultEnd, RegionTint Tint);

    /// <summary>The quads that would be drawn now. With sync off the panes can be far apart, so endpoints clamp to the viewport.</summary>
    internal IEnumerable<Quad> Quads()
    {
        var sideTop = _side.VerticalOffset;
        var resultTop = _result.VerticalOffset;
        var height = Bounds.Height;
        double Clamp(double y) => Math.Clamp(y, 0, height);
        foreach (var block in VisibleBlocks())
        {
            var sideStart = _side.LineToY(block.Side.Start) - sideTop;
            var sideEnd = Math.Max(_side.LineToY(block.Side.End) - sideTop, sideStart + 1);
            var resultStart = _result.LineToY(block.Result.Start) - resultTop;
            var resultEnd = Math.Max(_result.LineToY(block.Result.End) - resultTop, resultStart + 1);
            yield return new Quad(Clamp(sideStart), Clamp(sideEnd), Clamp(resultStart), Clamp(resultEnd), block.Tint);
        }
    }

    public override void Render(DrawingContext context)
    {
        ArgumentNullException.ThrowIfNull(context);
        var (sideX, resultX) = _sideIsLeft ? (0.0, Bounds.Width) : (Bounds.Width, 0.0);
        var middle = Bounds.Width / 2;
        foreach (var quad in Quads())
        {
            var geometry = new StreamGeometry();
            using (var figure = geometry.Open())
            {
                figure.BeginFigure(new Point(sideX, quad.SideStart), true);
                figure.CubicBezierTo(new Point(middle, quad.SideStart), new Point(middle, quad.ResultStart), new Point(resultX, quad.ResultStart));
                figure.LineTo(new Point(resultX, quad.ResultEnd));
                figure.CubicBezierTo(new Point(middle, quad.ResultEnd), new Point(middle, quad.SideEnd), new Point(sideX, quad.SideEnd));
                figure.EndFigure(true);
            }
            context.DrawGeometry(RegionBackground.WashFor(quad.Tint), Outlines[(int)quad.Tint], geometry);
        }
    }

    /// <summary>
    /// Blocks on screen in the side pane or in RESULT: one window scan per pane (binary search, then walk while on
    /// screen), united by block index. O(log n + visible) however far apart the panes are scrolled.
    /// </summary>
    private IEnumerable<BandBlock> VisibleBlocks()
    {
        var (sideFrom, sideTo) = Window(_side, static block => block.Side);
        var (resultFrom, resultTo) = Window(_result, static block => block.Result);
        for (var index = sideFrom; index < sideTo; index++)
        {
            yield return _blocks[index];
        }
        for (var index = resultFrom; index < resultTo; index++)
        {
            if (index < sideFrom || index >= sideTo)
            {
                yield return _blocks[index];
            }
        }
    }

    /// <summary>The contiguous index range of blocks whose range in <paramref name="pane"/> touches its viewport.</summary>
    private (int From, int To) Window(IEditorSurface pane, Func<BandBlock, LineRange> range)
    {
        var first = pane.YToLine(pane.VerticalOffset);
        var last = pane.YToLine(pane.VerticalOffset + pane.ViewportHeight);
        int low = 0, high = _blocks.Length;
        while (low < high)
        {
            var middle = (low + high) / 2;
            if (range(_blocks[middle]).End >= first)
            {
                high = middle;
            }
            else
            {
                low = middle + 1;
            }
        }
        var to = low;
        while (to < _blocks.Length && range(_blocks[to]).Start <= last)
        {
            to++;
        }
        return (low, to);
    }
}
