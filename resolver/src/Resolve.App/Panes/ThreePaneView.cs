using Avalonia;
using Avalonia.Controls;
using Avalonia.Media;
using Avalonia.Media.Immutable;
using Resolve.App.Editor;
using Resolve.Core.Merge;
using Resolve.Core.Resolution;
using Resolve.Core.Session;

namespace Resolve.App.Panes;

/// <summary>
/// LEFT | band | RESULT | band | RIGHT. Keeps the resolution document and the RESULT editor in step, tints and
/// connects the regions, and synchronises scrolling through the line maps (spec §10.5).
/// </summary>
public sealed class ThreePaneView : Grid
{
    private const double SplitterWidth = 3;
    private const double ScrollEpsilon = 0.5;

    private static readonly IBrush SplitterBrush = new ImmutableSolidColorBrush(Color.FromArgb(70, 148, 163, 184));

    private readonly MergeToolSession _session;
    private readonly AvaloniaEditSurface _left;
    private readonly AvaloniaEditSurface _result;
    private readonly AvaloniaEditSurface _right;
    private LineMap _leftMap;
    private LineMap _rightMap;
    private bool _syncing;

    public ThreePaneView(MergeToolSession session)
    {
        ArgumentNullException.ThrowIfNull(session);
        _session = session;
        var document = session.Document;
        _left = new AvaloniaEditSurface(string.Concat(session.Inputs.Left.Lines), readOnly: true, document.NewLine);
        _result = new AvaloniaEditSurface(document.Text, readOnly: false, document.NewLine);
        _right = new AvaloniaEditSurface(string.Concat(session.Inputs.Right.Lines), readOnly: true, document.NewLine);
        (_leftMap, _rightMap) = PaneMaps.For(session.Plan, document);
        LeftBand = new BandCanvas(_left, _result, sideIsLeft: true) { MinWidth = 12 };
        RightBand = new BandCanvas(_right, _result, sideIsLeft: false) { MinWidth = 12 };

        ColumnDefinitions = new ColumnDefinitions("*,Auto,0.08*,Auto,*,Auto,0.08*,Auto,*");
        Place(_left.View, 0);
        Place(Splitter(), 1);
        Place(LeftBand, 2);
        Place(Splitter(), 3);
        Place(_result.View, 4);
        Place(Splitter(), 5);
        Place(RightBand, 6);
        Place(Splitter(), 7);
        Place(_right.View, 8);

        _left.Scrolled += _ => OnScrolled(_left);
        _result.Scrolled += _ => OnScrolled(_result);
        _right.Scrolled += _ => OnScrolled(_right);
        _result.Edited += HandleResultEdit;
        Decorate();
    }

    public IEditorSurface Left => _left;

    public IEditorSurface Result => _result;

    public IEditorSurface Right => _right;

    /// <summary>When false the three panes scroll independently.</summary>
    public bool SyncScroll { get; set; } = true;

    internal BandCanvas LeftBand { get; }

    internal BandCanvas RightBand { get; }

    /// <summary>
    /// Raised after RESULT and the document absorbed a user edit, so the window can refresh what depends on the
    /// document. Raised outside the editor's own change handling; it never feeds back into the edit.
    /// </summary>
    public event Action? ResultEdited;

    /// <summary>Raised with the reason when an edit or refresh failed and RESULT was resynchronised from the document.</summary>
    public event Action<string>? Problem;

    /// <summary>Applies document-driven changes (a region action, undo, redo) to RESULT, then refreshes maps, tints and bands.</summary>
    public void Refresh(IReadOnlyList<TextChange> changes)
    {
        ArgumentNullException.ThrowIfNull(changes);
        try
        {
            _result.ApplyChanges(changes);
            Decorate();
        }
        catch (Exception error)
        {
            Recover(error);
        }
    }

    /// <summary>Brings a RESULT line into view, a third of the way down the viewport unless it is already comfortably visible.</summary>
    public void RevealLine(int resultLine)
    {
        ArgumentOutOfRangeException.ThrowIfNegative(resultLine);
        var first = _result.YToLine(_result.VerticalOffset);
        var last = _result.YToLine(_result.VerticalOffset + _result.ViewportHeight);
        if (resultLine > first && resultLine < last - 1)
        {
            return;
        }
        var visible = Math.Max(1, last - first);
        _result.ScrollTo(_result.LineToY(Math.Max(0, resultLine - (visible / 3))));
    }

    private void Place(Control control, int column)
    {
        SetColumn(control, column);
        Children.Add(control);
    }

    private static GridSplitter Splitter() => new()
    {
        Width = SplitterWidth,
        Background = SplitterBrush,
        ResizeDirection = GridResizeDirection.Columns,
        ResizeBehavior = GridResizeBehavior.PreviousAndNext,
    };

    /// <summary>
    /// RESULT already holds the user's change; the document may have rewritten it (typed newlines become its own),
    /// so bring RESULT in line. Never throws (ruling R27): this runs inside AvaloniaEdit's input handling, so anything
    /// unexpected — in the document, the maps or the decorations — resyncs RESULT from the document text and is announced.
    /// </summary>
    internal void HandleResultEdit(TextChange change)
    {
        try
        {
            var applied = _session.Document.Edit(change);
            if (applied.Count != 1 || applied[0].Offset != change.Offset || applied[0].RemovedLength != change.RemovedLength)
            {
                _result.Resync(_session.Document.Text);
            }
            else if (applied[0] != change)
            {
                // The surface holds change.InsertedText at the offset now; swap it for the document's text.
                _result.ApplyChanges([new TextChange(change.Offset, change.InsertedText.Length, applied[0].InsertedText)]);
            }
            Decorate();
        }
        catch (Exception error)
        {
            Recover(error);
        }
        ResultEdited?.Invoke();
    }

    /// <summary>
    /// Shows the document's text in RESULT again if the two have drifted apart. For the window's safety nets: a
    /// command that failed part-way may have left RESULT behind the document, and the document is what is saved.
    /// Never throws; if this fails too, the next edit or command tries again.
    /// </summary>
    internal void ShowDocument()
    {
        try
        {
            if (_result.Text != _session.Document.Text)
            {
                _result.Resync(_session.Document.Text);
            }
            Decorate();
        }
        catch (Exception)
        {
            // Called from a catch already: there is nothing further to report or to do.
        }
    }

    /// <summary>The document is the truth: show its text again, redraw what can be redrawn, and tell the user.</summary>
    private void Recover(Exception error)
    {
        try
        {
            _result.Resync(_session.Document.Text);
            Decorate();
        }
        catch (Exception again)
        {
            // Nothing more can be done here; the text is still the document's and the next edit tries again.
            error = new AggregateException(error, again);
        }
        Problem?.Invoke(error.Message);
    }

    private void Decorate()
    {
        var document = _session.Document;
        (_leftMap, _rightMap) = PaneMaps.For(_session.Plan, document);
        var regions = document.Regions;
        List<Decoration> result = [], left = [], right = [];
        List<BandBlock> leftBlocks = [], rightBlocks = [];
        for (var index = 0; index < regions.Count; index++)
        {
            var view = regions[index];
            if (TintOf(view) is not { } tint)
            {
                continue;
            }
            var planned = _session.Plan.Regions[index];
            result.Add(new Decoration(view.ResultLines.Start, view.ResultLines.End, tint));
            left.Add(new Decoration(planned.Left.Start, planned.Left.End, tint));
            right.Add(new Decoration(planned.Right.Start, planned.Right.End, tint));
            leftBlocks.Add(new BandBlock(planned.Left, view.ResultLines, tint));
            rightBlocks.Add(new BandBlock(planned.Right, view.ResultLines, tint));
        }
        _result.SetDecorations(result);
        _left.SetDecorations(left);
        _right.SetDecorations(right);
        LeftBand.SetBlocks(leftBlocks);
        RightBand.SetBlocks(rightBlocks);
    }

    /// <summary>Null for an equal region; a non-conflict region still in its default state (spec D4) reads as applied.</summary>
    private static RegionTint? TintOf(RegionView view)
    {
        if (view.Kind == RegionKind.Equal)
        {
            return null;
        }
        var isDefault = view.Kind switch
        {
            RegionKind.LeftOnly or RegionKind.BothSame => view.Choice == ResolutionChoice.Left,
            RegionKind.RightOnly => view.Choice == ResolutionChoice.Right,
            _ => false,
        };
        return isDefault
            ? RegionTint.Applied
            : view.Choice switch
            {
                ResolutionChoice.Unresolved => RegionTint.Unresolved,
                ResolutionChoice.Left => RegionTint.FromLeft,
                ResolutionChoice.Right => RegionTint.FromRight,
                ResolutionChoice.LeftThenRight or ResolutionChoice.RightThenLeft => RegionTint.Both,
                ResolutionChoice.Manual => RegionTint.Manual,
                _ => RegionTint.Applied,
            };
    }

    /// <summary>The pane that raised the event is the master; the others follow through the line maps. The guard stops the followers' own events from feeding back.</summary>
    private void OnScrolled(AvaloniaEditSurface master)
    {
        LeftBand.InvalidateVisual();
        RightBand.InvalidateVisual();
        if (!SyncScroll || _syncing)
        {
            return;
        }
        _syncing = true;
        try
        {
            var line = FractionalLine(master);
            var resultLine = master == _result ? line : (master == _left ? _leftMap : _rightMap).ToResult(line);
            if (master != _result)
            {
                Follow(_result, resultLine);
            }
            if (master != _left)
            {
                Follow(_left, _leftMap.ToSide(resultLine));
            }
            if (master != _right)
            {
                Follow(_right, _rightMap.ToSide(resultLine));
            }
        }
        finally
        {
            _syncing = false;
        }
    }

    private static double FractionalLine(AvaloniaEditSurface surface)
    {
        var line = surface.YToLine(surface.VerticalOffset);
        var height = surface.LineToY(line + 1) - surface.LineToY(line);
        return height <= 0 ? line : line + ((surface.VerticalOffset - surface.LineToY(line)) / height);
    }

    private static void Follow(AvaloniaEditSurface surface, double line)
    {
        var whole = (int)Math.Floor(line);
        var target = surface.LineToY(whole) + ((line - whole) * (surface.LineToY(whole + 1) - surface.LineToY(whole)));
        if (Math.Abs(target - surface.VerticalOffset) > ScrollEpsilon)
        {
            surface.ScrollTo(target);
        }
    }
}
