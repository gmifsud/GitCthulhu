// Spike S1 — three AvaloniaEdit panes, connector bands, synchronised scrolling, measured.
// Throwaway: proves or refutes the band model (ADR 0004) on a 100k-line file. Not product code.
using System.Diagnostics;
using System.Text;
using System.Text.Json;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.ApplicationLifetimes;
using Avalonia.Markup.Xaml.Styling;
using Avalonia.Media;
using Avalonia.Media.Immutable;
using Avalonia.Themes.Fluent;
using Avalonia.Threading;
using Avalonia.VisualTree;
using Avalonia.Controls.Primitives;
using AvaloniaEdit;
using AvaloniaEdit.Rendering;
using Resolve.Core.Diff;
using Resolve.Core.Merge;
using Resolve.Core.Text;

namespace S1.ThreePane;

internal static class Program
{
    /// <summary>Set first thing in Main (a static initialiser would run lazily, at first use).</summary>
    public static long StartTimestamp { get; private set; }

    [STAThread]
    public static int Main(string[] args)
    {
        StartTimestamp = Stopwatch.GetTimestamp();
        return AppBuilder.Configure<SpikeApp>().UsePlatformDetect().StartWithClassicDesktopLifetime(args);
    }
}

internal sealed class SpikeApp : Application
{
    public override void Initialize()
    {
        Styles.Add(new FluentTheme());
        Styles.Add(new StyleInclude(new Uri("avares://S1.ThreePane/"))
        {
            Source = new Uri("avares://AvaloniaEdit/Themes/Fluent/AvaloniaEdit.xaml"),
        });
        RequestedThemeVariant = Avalonia.Styling.ThemeVariant.Dark;
    }

    public override void OnFrameworkInitializationCompleted()
    {
        if (ApplicationLifetime is IClassicDesktopStyleApplicationLifetime desktop)
        {
            var lines = int.TryParse(Environment.GetEnvironmentVariable("S1_LINES"), out var n) ? n : 100_000;
            desktop.MainWindow = new SpikeWindow(Workload.Create(lines), Environment.GetEnvironmentVariable("S1_OUT"));
        }
        base.OnFrameworkInitializationCompleted();
    }
}

/// <summary>Synthetic C#-shaped file with scattered edits on both sides; the result follows spec D4.</summary>
internal sealed record Workload(string Left, string Result, string Right, LineMap LeftToResult, LineMap RightToResult,
    RegionKind[] LeftKinds, RegionKind[] ResultKinds, RegionKind[] RightKinds, MergePlan Plan, long PlanMilliseconds)
{
    public static Workload Create(int lineCount)
    {
        var random = new Random(1);
        var baseLines = Enumerable.Range(0, lineCount)
            .Select(i => new Line(i % 7 == 0 ? $"    public int Method{i}(int value) => value * {i};" : $"        // line {i}: {(i % 3 == 0 ? "TODO" : "note")} {i * 31 % 997}", LineEnding.Lf))
            .ToArray();
        Line[] Mutate(string tag)
        {
            var copy = baseLines.ToList();
            for (var edit = 0; edit < lineCount / 300; edit++)
            {
                var at = random.Next(copy.Count);
                if (edit % 4 == 0)
                {
                    copy.Insert(at, new Line($"        var {tag}{edit} = Compute({edit});", LineEnding.Lf));
                }
                else
                {
                    copy[at] = new Line(copy[at].Content + $" // {tag} edit {edit}", LineEnding.Lf);
                }
            }
            return [.. copy];
        }
        var left = Mutate("left");
        var right = Mutate("right");
        var clock = Stopwatch.StartNew();
        var plan = Diff3Planner.Plan(baseLines, left, right, DiffAlgorithm.Histogram);
        clock.Stop();

        var result = new List<Line>();
        var leftKinds = new RegionKind[left.Length];
        var rightKinds = new RegionKind[right.Length];
        var resultKinds = new List<RegionKind>();
        var leftMap = new List<(int Side, int Result)>();
        var rightMap = new List<(int Side, int Result)>();
        foreach (var region in plan.Regions)
        {
            leftMap.Add((region.Left.Start, result.Count));
            rightMap.Add((region.Right.Start, result.Count));
            IEnumerable<Line> chosen = region.Kind switch
            {
                RegionKind.RightOnly => right[region.Right.Start..region.Right.End],
                RegionKind.Conflict => baseLines[region.Base.Start..region.Base.End],
                _ => left[region.Left.Start..region.Left.End],
            };
            foreach (var line in chosen)
            {
                result.Add(line);
                resultKinds.Add(region.Kind);
            }
            for (var i = region.Left.Start; i < region.Left.End; i++) leftKinds[i] = region.Kind;
            for (var i = region.Right.Start; i < region.Right.End; i++) rightKinds[i] = region.Kind;
        }
        leftMap.Add((left.Length, result.Count));
        rightMap.Add((right.Length, result.Count));
        static string Join(IEnumerable<Line> lines) => string.Concat(lines.Select(line => line.ToString()));
        return new Workload(Join(left), Join(result), Join(right), new LineMap(leftMap), new LineMap(rightMap),
            leftKinds, [.. resultKinds], rightKinds, plan, clock.ElapsedMilliseconds);
    }
}

/// <summary>Monotone piecewise-linear map between two panes' line spaces (spec §10.5), breakpoints at region starts.</summary>
internal sealed class LineMap(List<(int Side, int Result)> points)
{
    public double ResultToSide(double resultLine) => Map(resultLine, p => p.Result, p => p.Side);

    public double SideToResult(double sideLine) => Map(sideLine, p => p.Side, p => p.Result);

    public IReadOnlyList<(int Side, int Result)> Points => points;

    private double Map(double value, Func<(int Side, int Result), int> from, Func<(int Side, int Result), int> to)
    {
        int lo = 0, hi = points.Count - 1;
        while (lo < hi)
        {
            var mid = (lo + hi + 1) / 2;
            if (from(points[mid]) <= value) lo = mid; else hi = mid - 1;
        }
        if (lo >= points.Count - 1) return to(points[^1]);
        var (a, b) = (points[lo], points[lo + 1]);
        var span = from(b) - from(a);
        return span == 0 ? to(a) : to(a) + (value - from(a)) * (to(b) - to(a)) / span;
    }
}

internal static class Palette
{
    // Immutable brushes, created once: the first run allocated one per visible line per frame.
    private static readonly IBrush LeftOnly = new ImmutableSolidColorBrush(Color.FromArgb(60, 59, 130, 246));
    private static readonly IBrush RightOnly = new ImmutableSolidColorBrush(Color.FromArgb(60, 34, 197, 94));
    private static readonly IBrush BothSame = new ImmutableSolidColorBrush(Color.FromArgb(50, 168, 85, 247));
    private static readonly IBrush Conflict = new ImmutableSolidColorBrush(Color.FromArgb(80, 239, 68, 68));

    public static IBrush? For(RegionKind kind) => kind switch
    {
        RegionKind.LeftOnly => LeftOnly,
        RegionKind.RightOnly => RightOnly,
        RegionKind.BothSame => BothSame,
        RegionKind.Conflict => Conflict,
        _ => null,
    };
}

/// <summary>
/// Line numbers for visible lines only. Width depends on the digit count alone, so an edit that keeps the
/// digit count never re-measures the editor (the stock margin did, and dominated keystroke latency).
/// </summary>
internal sealed class FastLineNumbers : AvaloniaEdit.Editing.AbstractMargin
{
    private static readonly IBrush Ink = new ImmutableSolidColorBrush(Color.FromRgb(110, 110, 120));
    private static readonly Typeface Face = new("DejaVu Sans Mono, Cascadia Code, Consolas, Menlo, monospace");
    private const double Size = 12;
    private readonly Dictionary<int, FormattedText> _shaped = [];
    private int _digits;

    protected override Size MeasureOverride(Size availableSize)
    {
        _digits = Math.Max(3, (Document?.LineCount ?? 1).ToString().Length);
        return new Size(_digits * Size * 0.62 + 12, 0);
    }

    public override void Render(DrawingContext context)
    {
        if (TextView is not { VisualLinesValid: true } view) return;
        if (_shaped.Count > 4096) _shaped.Clear(); // bounded: a screenful is ~60 entries
        foreach (var line in view.VisualLines)
        {
            var lineNumber = line.FirstDocumentLine.LineNumber;
            if (!_shaped.TryGetValue(lineNumber, out var number))
            {
                number = new FormattedText(lineNumber.ToString(), System.Globalization.CultureInfo.InvariantCulture,
                    FlowDirection.LeftToRight, Face, Size, Ink);
                _shaped[lineNumber] = number;
            }
            context.DrawText(number, new Point(Bounds.Width - 6 - number.Width, line.VisualTop - view.VerticalOffset));
        }
    }

    protected override void OnTextViewChanged(TextView? oldTextView, TextView? newTextView)
    {
        if (oldTextView is not null) oldTextView.VisualLinesChanged -= OnVisualLinesChanged;
        base.OnTextViewChanged(oldTextView, newTextView);
        if (newTextView is not null) newTextView.VisualLinesChanged += OnVisualLinesChanged;
    }

    protected override void OnDocumentChanged(AvaloniaEdit.Document.TextDocument? oldDocument, AvaloniaEdit.Document.TextDocument? newDocument)
    {
        if (oldDocument is not null) oldDocument.LineCountChanged -= OnLineCountChanged;
        base.OnDocumentChanged(oldDocument, newDocument);
        if (newDocument is not null) newDocument.LineCountChanged += OnLineCountChanged;
    }

    private void OnVisualLinesChanged(object? sender, EventArgs e) => InvalidateVisual();

    private void OnLineCountChanged(object? sender, EventArgs e)
    {
        if ((Document?.LineCount ?? 1).ToString().Length != _digits) InvalidateMeasure();
    }
}

/// <summary>Region tints for visible lines only: O(visible lines) per frame.</summary>
internal sealed class RegionBackground(RegionKind[] kinds) : IBackgroundRenderer
{
    public KnownLayer Layer => KnownLayer.Background;

    public void Draw(TextView textView, DrawingContext drawingContext)
    {
        if (!textView.VisualLinesValid) return;
        foreach (var line in textView.VisualLines)
        {
            var index = line.FirstDocumentLine.LineNumber - 1;
            if (index >= kinds.Length || Palette.For(kinds[index]) is not { } brush) continue;
            var top = line.VisualTop - textView.VerticalOffset;
            drawingContext.FillRectangle(brush, new Rect(0, top, textView.Bounds.Width, line.Height));
        }
    }
}

/// <summary>Connector bands between a source pane and the result pane, drawn for on-screen regions only.</summary>
internal sealed class BandCanvas : Control
{
    private readonly TextEditor _side;
    private readonly TextEditor _result;
    private readonly MergePlan _plan;
    private readonly bool _sideIsLeft;

    public BandCanvas(TextEditor side, TextEditor result, MergePlan plan, bool sideIsLeft)
    {
        (_side, _result, _plan, _sideIsLeft) = (side, result, plan, sideIsLeft);
        Width = 36;
        ClipToBounds = true;
    }

    public Func<int, (int Start, int End)>? ResultRangeOf { get; set; }

    public override void Render(DrawingContext context)
    {
        var lineHeight = _result.TextArea.TextView.DefaultLineHeight;
        var height = Bounds.Height;
        var sideTop = _side.VerticalOffset;
        var resultTop = _result.VerticalOffset;
        foreach (var region in VisibleRegions(sideTop / lineHeight, (sideTop + height) / lineHeight))
        {
            if (Palette.For(region.Kind) is not { } brush || ResultRangeOf is null) continue;
            var sideRange = _sideIsLeft ? region.Left : region.Right;
            var (rStart, rEnd) = ResultRangeOf(region.Id);
            var s0 = sideRange.Start * lineHeight - sideTop;
            var s1 = sideRange.End * lineHeight - sideTop;
            var r0 = rStart * lineHeight - resultTop;
            var r1 = rEnd * lineHeight - resultTop;
            var (x0, x1) = _sideIsLeft ? (0.0, Bounds.Width) : (Bounds.Width, 0.0);
            var geometry = new StreamGeometry();
            using (var g = geometry.Open())
            {
                var mid = Bounds.Width / 2;
                g.BeginFigure(new Point(x0, s0), true);
                g.CubicBezierTo(new Point(mid, s0), new Point(mid, r0), new Point(x1, r0));
                g.LineTo(new Point(x1, Math.Max(r1, r0 + 1)));
                g.CubicBezierTo(new Point(mid, Math.Max(r1, r0 + 1)), new Point(mid, Math.Max(s1, s0 + 1)), new Point(x0, Math.Max(s1, s0 + 1)));
                g.EndFigure(true);
            }
            context.DrawGeometry(brush, null, geometry);
        }
    }

    private IEnumerable<PlanRegion> VisibleRegions(double firstLine, double lastLine)
    {
        // Binary search on side start, then walk while on screen: O(log n + visible).
        var regions = _plan.Regions;
        int lo = 0, hi = regions.Length - 1;
        while (lo < hi)
        {
            var mid = (lo + hi + 1) / 2;
            if ((_sideIsLeft ? regions[mid].Left.Start : regions[mid].Right.Start) <= firstLine) lo = mid; else hi = mid - 1;
        }
        for (var i = lo; i < regions.Length; i++)
        {
            var start = _sideIsLeft ? regions[i].Left.Start : regions[i].Right.Start;
            if (start > lastLine + 1) yield break;
            if (regions[i].Kind != RegionKind.Equal) yield return regions[i];
        }
    }
}

internal sealed class SpikeWindow : Window
{
    private readonly Workload _work;
    private readonly string? _outputPath;
    private readonly TextEditor _left;
    private readonly TextEditor _result;
    private readonly TextEditor _right;
    private readonly BandCanvas _leftBand;
    private readonly BandCanvas _rightBand;
    private readonly int[] _resultStarts;
    private readonly List<double> _syncCostMs = [];
    private bool _syncing;

    public SpikeWindow(Workload work, string? outputPath)
    {
        (_work, _outputPath) = (work, outputPath);
        Title = "S1 three-pane spike";
        Width = 1600;
        Height = 1000;
        _left = Editor(work.Left, readOnly: true, work.LeftKinds);
        _result = Editor(work.Result, readOnly: false, work.ResultKinds);
        _right = Editor(work.Right, readOnly: true, work.RightKinds);
        _resultStarts = work.LeftToResult.Points.Select(p => p.Result).ToArray();
        _leftBand = new BandCanvas(_left, _result, work.Plan, sideIsLeft: true) { ResultRangeOf = ResultRange };
        _rightBand = new BandCanvas(_right, _result, work.Plan, sideIsLeft: false) { ResultRangeOf = ResultRange };

        var grid = new Grid { ColumnDefinitions = new ColumnDefinitions("*,Auto,*,Auto,*") };
        void Place(Control control, int column)
        {
            Grid.SetColumn(control, column);
            grid.Children.Add(control);
        }
        Place(_left, 0);
        Place(_leftBand, 1);
        Place(_result, 2);
        Place(_rightBand, 3);
        Place(_right, 4);
        Content = grid;

        _result.TextArea.TextView.ScrollOffsetChanged += (_, _) => SyncFrom(_result);
        _left.TextArea.TextView.ScrollOffsetChanged += (_, _) => SyncFrom(_left);
        _right.TextArea.TextView.ScrollOffsetChanged += (_, _) => SyncFrom(_right);
        Opened += (_, _) => Dispatcher.UIThread.Post(() => _ = MeasureAsync(), DispatcherPriority.Background);
    }

    // Regions keep their initial result ranges in the spike (no anchors yet; Phase 1 adds them).
    private (int, int) ResultRange(int regionId) => (_resultStarts[regionId], _resultStarts[regionId + 1]);

    private static TextEditor Editor(string text, bool readOnly, RegionKind[] kinds)
    {
        var editor = new TextEditor
        {
            Text = text,
            IsReadOnly = readOnly,
            ShowLineNumbers = Environment.GetEnvironmentVariable("S1_LINENUMBERS") == "stock",
            FontFamily = new FontFamily("DejaVu Sans Mono, Cascadia Code, Consolas, Menlo, monospace"),
            FontSize = 13,
            HorizontalScrollBarVisibility = Avalonia.Controls.Primitives.ScrollBarVisibility.Auto,
        };
        if (Environment.GetEnvironmentVariable("S1_LINENUMBERS") is not ("stock" or "none"))
        {
            editor.TextArea.LeftMargins.Insert(0, new FastLineNumbers());
        }
        editor.TextArea.TextView.BackgroundRenderers.Add(new RegionBackground(kinds));
        return editor;
    }

    /// <summary>
    /// AvaloniaEdit 12.0.0's <c>TextEditor.ScrollToVerticalOffset</c> is a no-op (measured in this spike); the
    /// template's ScrollViewer does scroll. The product's IEditorSurface adapter hides this.
    /// </summary>
    private static void SetVerticalOffset(TextEditor editor, double offset)
    {
        if (editor.GetVisualDescendants().OfType<ScrollViewer>().FirstOrDefault() is { } viewer)
        {
            viewer.Offset = new Vector(viewer.Offset.X, Math.Max(0, offset));
        }
    }

    private void SyncFrom(TextEditor master)
    {
        if (_syncing) return;
        _syncing = true;
        var clock = Stopwatch.StartNew();
        try
        {
            var lineHeight = master.TextArea.TextView.DefaultLineHeight;
            var masterLine = master.VerticalOffset / lineHeight;
            var resultLine = master == _result ? masterLine
                : master == _left ? _work.LeftToResult.SideToResult(masterLine)
                : _work.RightToResult.SideToResult(masterLine);
            if (master != _result) SetVerticalOffset(_result, resultLine * lineHeight);
            if (master != _left) SetVerticalOffset(_left, _work.LeftToResult.ResultToSide(resultLine) * lineHeight);
            if (master != _right) SetVerticalOffset(_right, _work.RightToResult.ResultToSide(resultLine) * lineHeight);
            _leftBand.InvalidateVisual();
            _rightBand.InvalidateVisual();
        }
        finally
        {
            _syncCostMs.Add(clock.Elapsed.TotalMilliseconds);
            _syncing = false;
        }
    }

    private Task<TimeSpan> NextFrameAsync()
    {
        // Continuations must not run inside the frame callback, or the next request lands in the same frame
        // and no layout happens in between (the first run measured 0.01 ms "frames" for that reason).
        var done = new TaskCompletionSource<TimeSpan>(TaskCreationOptions.RunContinuationsAsynchronously);
        RequestAnimationFrame(time => done.TrySetResult(time));
        return done.Task;
    }

    private async Task MeasureAsync()
    {
        await NextFrameAsync();
        var firstFrameMs = Stopwatch.GetElapsedTime(Program.StartTimestamp).TotalMilliseconds;
        for (var warm = 0; warm < 30; warm++) await NextFrameAsync();
        // Scroll: master = result pane, 600 frames, 40 px per frame (fast wheel scrolling).
        var frameMs = new List<double>();
        var last = Stopwatch.GetTimestamp();
        for (var frame = 0; frame < 600; frame++)
        {
            SetVerticalOffset(_result, _result.VerticalOffset + 40);
            await NextFrameAsync();
            var now = Stopwatch.GetTimestamp();
            frameMs.Add(Stopwatch.GetElapsedTime(last, now).TotalMilliseconds);
            last = now;
        }

        // Jumps: next-conflict style navigation across the file.
        var jumpMs = new List<double>();
        var conflicts = _work.Plan.Regions.Where(r => r.Kind == RegionKind.Conflict).Take(40).ToArray();
        foreach (var region in conflicts)
        {
            var start = Stopwatch.GetTimestamp();
            _result.ScrollToLine(_resultStarts[region.Id] + 1);
            await NextFrameAsync();
            jumpMs.Add(Stopwatch.GetElapsedTime(start).TotalMilliseconds);
        }

        // Typing: insert one character mid-file, measure edit → next rendered frame.
        _result.ScrollToLine(_result.Document.LineCount / 2);
        await NextFrameAsync();
        var middle = _result.Document.GetLineByNumber(_result.Document.LineCount / 2);
        var keyMs = new List<double>();
        var editMs = new List<double>();
        for (var key = 0; key < 200; key++)
        {
            var start = Stopwatch.GetTimestamp();
            _result.Document.Insert(middle.EndOffset, "x");
            editMs.Add(Stopwatch.GetElapsedTime(start).TotalMilliseconds);
            await NextFrameAsync();
            keyMs.Add(Stopwatch.GetElapsedTime(start).TotalMilliseconds);
        }

        // Isolate Document.Insert cost: detached document vs TextEditor-attached vs bare TextArea.
        static List<double> Inserts(AvaloniaEdit.Document.TextDocument document)
        {
            var at = document.GetLineByNumber(document.LineCount / 2).EndOffset;
            var times = new List<double>();
            for (var i = 0; i < 200; i++)
            {
                var start = Stopwatch.GetTimestamp();
                document.Insert(at, "x");
                times.Add(Stopwatch.GetElapsedTime(start).TotalMilliseconds);
            }
            return times;
        }
        var detached = Inserts(new AvaloniaEdit.Document.TextDocument(_work.Result));
        var bareArea = new AvaloniaEdit.Editing.TextArea { Document = new AvaloniaEdit.Document.TextDocument(_work.Result) };
        var areaInserts = Inserts(bareArea.Document);
        var editorProbe = new TextEditor { Document = new AvaloniaEdit.Document.TextDocument(_work.Result) };
        var editorInserts = Inserts(editorProbe.Document);

        GC.Collect();
        var report = new
        {
            lines = _work.Result.Count(c => c == '\n'),
            regions = _work.Plan.Regions.Length,
            conflicts = _work.Plan.ConflictCount,
            changes = _work.Plan.ChangeCount,
            planMs = _work.PlanMilliseconds,
            firstFrameMs = Math.Round(firstFrameMs),
            scrollFrameMs = Stats(frameMs),
            syncHandlerMs = Stats(_syncCostMs),
            conflictJumpMs = Stats(jumpMs),
            keystrokeToFrameMs = Stats(keyMs),
            documentInsertMs = Stats(editMs),
            insertDetachedMs = Stats(detached),
            insertBareTextAreaMs = Stats(areaInserts),
            insertOffscreenTextEditorMs = Stats(editorInserts),
            gen2Collections = GC.CollectionCount(2),
            managedHeapMb = Math.Round(GC.GetTotalMemory(true) / 1048576.0, 1),
            workingSetMb = Math.Round(Process.GetCurrentProcess().WorkingSet64 / 1048576.0, 1),
            lineNumbers = Environment.GetEnvironmentVariable("S1_LINENUMBERS") ?? "fast",
            os = System.Runtime.InteropServices.RuntimeInformation.OSDescription,
        };
        var json = JsonSerializer.Serialize(report, new JsonSerializerOptions { WriteIndented = true });
        Console.WriteLine(json);
        if (_outputPath is not null)
        {
            await File.WriteAllTextAsync(_outputPath, json, Encoding.UTF8);
            if (Environment.GetEnvironmentVariable("S1_SCREENSHOT") is null)
            {
                Close();
            }
            else if ((conflicts.FirstOrDefault() ?? _work.Plan.Regions.FirstOrDefault(r => r.Kind != RegionKind.Equal)) is { } shown)
            {
                _result.ScrollToLine(Math.Max(1, _resultStarts[shown.Id] - 10));
            }
        }
    }

    private static object Stats(List<double> values)
    {
        if (values.Count == 0) return new { count = 0 };
        var sorted = values.Order().ToArray();
        double At(double q) => Math.Round(sorted[Math.Min(sorted.Length - 1, (int)(q * sorted.Length))], 2);
        return new { count = sorted.Length, p50 = At(0.50), p95 = At(0.95), p99 = At(0.99), max = Math.Round(sorted[^1], 2) };
    }
}
