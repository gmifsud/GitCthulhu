using Avalonia;
using Avalonia.Media;
using Avalonia.Media.Immutable;
using AvaloniaEdit.Rendering;

namespace Resolve.App.Editor;

/// <summary>
/// Paints region tints for the visible lines only (ADR 0007: O(visible)). Each tint is a full-line wash plus a
/// 3-px left-edge glyph whose pattern is unique to the tint, so state is never carried by colour alone.
/// Brushes are created once and immutable (spike S1 finding 5).
/// </summary>
internal sealed class RegionBackground : IBackgroundRenderer
{
    private const double GlyphWidth = 3;
    private const double Period = 8;

    private static readonly IBrush[] Washes =
    [
        new ImmutableSolidColorBrush(Color.FromArgb(80, 239, 68, 68)),   // Unresolved
        new ImmutableSolidColorBrush(Color.FromArgb(60, 59, 130, 246)),  // FromLeft
        new ImmutableSolidColorBrush(Color.FromArgb(60, 34, 197, 94)),   // FromRight
        new ImmutableSolidColorBrush(Color.FromArgb(50, 168, 85, 247)),  // Both
        new ImmutableSolidColorBrush(Color.FromArgb(60, 245, 158, 11)),  // Manual
        new ImmutableSolidColorBrush(Color.FromArgb(30, 148, 163, 184)), // Applied
    ];

    private static readonly IBrush[] Inks =
    [
        new ImmutableSolidColorBrush(Color.FromRgb(239, 68, 68)),
        new ImmutableSolidColorBrush(Color.FromRgb(59, 130, 246)),
        new ImmutableSolidColorBrush(Color.FromRgb(34, 197, 94)),
        new ImmutableSolidColorBrush(Color.FromRgb(168, 85, 247)),
        new ImmutableSolidColorBrush(Color.FromRgb(245, 158, 11)),
        new ImmutableSolidColorBrush(Color.FromRgb(148, 163, 184)),
    ];

    private Decoration[] _decorations = [];

    public KnownLayer Layer => KnownLayer.Background;

    /// <summary>The wash brush for a tint, shared with the connector bands so the panes and bands match.</summary>
    internal static IBrush WashFor(RegionTint tint) => Washes[(int)tint];

    /// <summary>The solid ink for a tint, used for band outlines.</summary>
    internal static IBrush InkFor(RegionTint tint) => Inks[(int)tint];

    public void Set(IReadOnlyList<Decoration> decorations)
    {
        var sorted = decorations.Where(d => d.EndLine > d.StartLine).ToArray();
        Array.Sort(sorted, (a, b) => a.StartLine.CompareTo(b.StartLine));
        _decorations = sorted;
    }

    public void Draw(TextView textView, DrawingContext drawingContext)
    {
        if (_decorations.Length == 0 || !textView.VisualLinesValid)
        {
            return;
        }
        foreach (var line in textView.VisualLines)
        {
            if (TintAt(line.FirstDocumentLine.LineNumber - 1) is not { } tint)
            {
                continue;
            }
            var top = line.VisualTop - textView.VerticalOffset;
            drawingContext.FillRectangle(Washes[(int)tint], new Rect(0, top, textView.Bounds.Width, line.Height));
            DrawGlyph(drawingContext, Inks[(int)tint], tint, line.VisualTop, top, line.Height);
        }
    }

    /// <summary>Binary search for the last decoration starting at or before the line; O(log n) per visible line.</summary>
    internal RegionTint? TintAt(int line)
    {
        if (_decorations.Length == 0)
        {
            return null;
        }
        int lo = 0, hi = _decorations.Length - 1;
        while (lo < hi)
        {
            var mid = (lo + hi + 1) / 2;
            if (_decorations[mid].StartLine <= line) lo = mid; else hi = mid - 1;
        }
        var found = _decorations[lo];
        return line >= found.StartLine && line < found.EndLine ? found.Tint : null;
    }

    /// <summary>Patterns phase on the document-space y so they run unbroken across adjacent lines.</summary>
    private static void DrawGlyph(DrawingContext context, IBrush ink, RegionTint tint, double documentTop, double top, double height)
    {
        switch (tint)
        {
            case RegionTint.Unresolved: // solid bar
                context.FillRectangle(ink, new Rect(0, top, GlyphWidth, height));
                break;
            case RegionTint.FromLeft: // dashed bar
                Segments(context, ink, documentTop, top, height, on: 5, step: Period, x: 0, width: GlyphWidth);
                break;
            case RegionTint.FromRight: // dotted bar
                Segments(context, ink, documentTop, top, height, on: 2, step: 4, x: 0, width: GlyphWidth);
                break;
            case RegionTint.Both: // double bar
                context.FillRectangle(ink, new Rect(0, top, 1, height));
                context.FillRectangle(ink, new Rect(2, top, 1, height));
                break;
            case RegionTint.Manual: // hatched: diagonal ticks
                for (var y = FirstTick(documentTop, 4) - documentTop; y < height; y += 4)
                {
                    for (var d = 0; d < 3; d++)
                    {
                        context.FillRectangle(ink, new Rect(d, top + y + d, 1, 1));
                    }
                }
                break;
            default: // Applied: thin bar
                context.FillRectangle(ink, new Rect(0, top, 1, height));
                break;
        }
    }

    private static void Segments(DrawingContext context, IBrush ink, double documentTop, double top, double height, double on, double step, double x, double width)
    {
        for (var y = FirstTick(documentTop, step) - documentTop; y < height; y += step)
        {
            var from = Math.Max(y, 0);
            var to = Math.Min(y + on, height);
            if (to > from)
            {
                context.FillRectangle(ink, new Rect(x, top + from, width, to - from));
            }
        }
    }

    private static double FirstTick(double documentTop, double step) => Math.Floor(documentTop / step) * step;
}
