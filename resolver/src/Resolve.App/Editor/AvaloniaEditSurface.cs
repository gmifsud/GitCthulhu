using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.Primitives;
using Avalonia.Input;
using Avalonia.Media;
using AvaloniaEdit;
using AvaloniaEdit.Document;
using AvaloniaEdit.Rendering;
using Resolve.Core.Resolution;

namespace Resolve.App.Editor;

/// <summary>
/// <see cref="IEditorSurface"/> over AvaloniaEdit. It owns no undo (the resolution document does), reports every
/// document change as one <see cref="TextChange"/>, and stays silent while it applies changes handed back to it.
/// </summary>
public sealed class AvaloniaEditSurface : IEditorSurface
{
    private readonly TextEditor _editor;
    private readonly string _newLine;
    private readonly RegionBackground _background = new();
    private ScrollViewer? _viewer;
    private double? _pendingOffset;
    private double _lastOffset;
    private bool _applying;
    private ITextSource? _before;
    private int _touchedStart;
    private int _untouchedTail;

    public AvaloniaEditSurface(string text, bool readOnly, string newLine)
    {
        ArgumentNullException.ThrowIfNull(text);
        ArgumentNullException.ThrowIfNull(newLine);
        _newLine = newLine;
        var document = new TextDocument(text);
        document.UndoStack.SizeLimit = 0;
        _editor = new TextEditor
        {
            Document = document,
            IsReadOnly = readOnly,
            FontFamily = new FontFamily("DejaVu Sans Mono, Cascadia Code, Consolas, Menlo, monospace"),
            FontSize = 13,
            HorizontalScrollBarVisibility = ScrollBarVisibility.Auto,
        };
        _editor.TextArea.TextView.BackgroundRenderers.Add(_background);
        document.UpdateStarted += OnUpdateStarted;
        document.Changed += OnDocumentChanged;
        document.UpdateFinished += OnUpdateFinished;
        _editor.AddHandler(InputElement.KeyDownEvent, OnKeyDown, Avalonia.Interactivity.RoutingStrategies.Tunnel);
        _editor.TemplateApplied += OnTemplateApplied;
        _editor.TextArea.TextView.ScrollOffsetChanged += OnScrollOffsetChanged;
    }

    public event Action<TextChange>? Edited;

    public event Action<double>? Scrolled;

    public bool IsReadOnly => _editor.IsReadOnly;

    public Control View => _editor;

    public double VerticalOffset => _editor.VerticalOffset;

    public double ViewportHeight => _editor.TextArea.TextView.Bounds.Height;

    internal string Text => _editor.Document.Text;

    internal RegionTint? TintAt(int line) => _background.TintAt(line);

    /// <summary>Gives keyboard focus to the text area (the TextEditor itself is not focusable).</summary>
    public bool Focus() => _editor.TextArea.Focus();

    public int CaretLine => _editor.TextArea.Caret.Line - 1;

    /// <summary>The caret as a character offset into the text.</summary>
    internal int CaretOffset
    {
        get => _editor.CaretOffset;
        set => _editor.CaretOffset = Math.Clamp(value, 0, _editor.Document.TextLength);
    }

    /// <summary>True while keyboard focus is in this pane's text area.</summary>
    internal bool HasFocus => _editor.TextArea.IsKeyboardFocusWithin;

    /// <summary>Selects [<paramref name="start"/>, <paramref name="start"/> + <paramref name="length"/>) and puts the caret at its end.</summary>
    internal void Select(int start, int length) => _editor.Select(start, length);

    public void MoveCaretToLine(int line)
    {
        var document = _editor.Document;
        var number = Math.Clamp(line + 1, 1, document.LineCount);
        _editor.CaretOffset = document.GetLineByNumber(number).Offset;
    }

    private double LineHeight => _editor.TextArea.TextView.DefaultLineHeight;

    public double LineToY(int line) => line * LineHeight;

    /// <summary>Line containing <paramref name="y"/>; the epsilon absorbs the rounding of offsets that sit exactly on a line edge.</summary>
    public int YToLine(double y) => LineHeight <= 0 ? 0 : Math.Max(0, (int)Math.Floor((y / LineHeight) + 1e-6));

    public void ScrollTo(double y)
    {
        var offset = Math.Max(0, y);
        if (_viewer is null)
        {
            _editor.ApplyTemplate();
        }
        if (_viewer is null)
        {
            _pendingOffset = offset;
            return;
        }
        // TextEditor.ScrollToVerticalOffset is a no-op in AvaloniaEdit 12.0.0 (spike S1); the template's viewer scrolls.
        _viewer.Offset = new Vector(_viewer.Offset.X, offset);
    }

    public void ApplyChanges(IReadOnlyList<TextChange> changes)
    {
        ArgumentNullException.ThrowIfNull(changes);
        _applying = true;
        try
        {
            _editor.Document.BeginUpdate();
            try
            {
                foreach (var change in changes)
                {
                    _editor.Document.Replace(change.Offset, change.RemovedLength, change.InsertedText);
                }
            }
            finally
            {
                _editor.Document.EndUpdate();
            }
        }
        finally
        {
            _applying = false;
        }
    }

    public void SetDecorations(IReadOnlyList<Decoration> decorations)
    {
        ArgumentNullException.ThrowIfNull(decorations);
        _background.Set(decorations);
        _editor.TextArea.TextView.InvalidateLayer(KnownLayer.Background);
    }

    /// <summary>The text as it was when the outermost update began: an O(1) snapshot of the rope.</summary>
    private void OnUpdateStarted(object? sender, EventArgs e)
    {
        if (_applying)
        {
            return;
        }
        _before = _editor.Document.CreateSnapshot();
        _touchedStart = int.MaxValue;
        _untouchedTail = int.MaxValue;
    }

    /// <summary>
    /// Every change moves the bounds of what can differ: nothing before the lowest offset touched, and nothing in the
    /// longest tail that no change reached, so the final diff only reads the span in between.
    /// </summary>
    private void OnDocumentChanged(object? sender, DocumentChangeEventArgs e)
    {
        if (_applying || _before is null)
        {
            return;
        }
        _touchedStart = Math.Min(_touchedStart, e.Offset);
        _untouchedTail = Math.Min(_untouchedTail, _editor.Document.TextLength - (e.Offset + e.InsertionLength));
    }

    /// <summary>One <see cref="Edited"/> per update: the minimal replacement between the text before and after it.</summary>
    private void OnUpdateFinished(object? sender, EventArgs e)
    {
        var before = _before;
        if (_applying || before is null)
        {
            return;
        }
        _before = null;
        var document = _editor.Document;
        var start = Math.Min(_touchedStart, Math.Min(before.TextLength, document.TextLength));
        var tail = Math.Max(0, Math.Min(_untouchedTail, Math.Min(before.TextLength, document.TextLength) - start));
        var removed = before.GetText(start, before.TextLength - tail - start);
        var inserted = document.GetText(start, document.TextLength - tail - start);
        var prefix = CommonPrefix(removed, inserted);
        var suffix = CommonSuffix(removed.AsSpan(prefix), inserted.AsSpan(prefix));
        var change = new TextChange(start + prefix, removed.Length - prefix - suffix, inserted[prefix..(inserted.Length - suffix)]);
        if (change.RemovedLength > 0 || change.InsertedText.Length > 0)
        {
            Edited?.Invoke(change);
        }
    }

    private static int CommonPrefix(ReadOnlySpan<char> a, ReadOnlySpan<char> b) => a.CommonPrefixLength(b);

    private static int CommonSuffix(ReadOnlySpan<char> a, ReadOnlySpan<char> b)
    {
        var count = 0;
        while (count < a.Length && count < b.Length && a[^(count + 1)] == b[^(count + 1)])
        {
            count++;
        }
        return count;
    }

    /// <summary>Replaces the whole text with <paramref name="text"/> without raising <see cref="Edited"/>, keeping the caret where it was (clamped).</summary>
    internal void Resync(string text)
    {
        ArgumentNullException.ThrowIfNull(text);
        var caret = _editor.CaretOffset;
        _applying = true;
        try
        {
            _editor.Document.Text = text;
        }
        finally
        {
            _applying = false;
        }
        _editor.CaretOffset = Math.Min(caret, text.Length);
    }

    /// <summary>Enter inserts the configured line ending; AvaloniaEdit would insert the document's own default.</summary>
    private void OnKeyDown(object? sender, KeyEventArgs e)
    {
        if (e.Key is not (Key.Enter or Key.Return) || e.KeyModifiers != KeyModifiers.None)
        {
            return;
        }
        e.Handled = true;
        if (!_editor.IsReadOnly)
        {
            _editor.TextArea.Selection.ReplaceSelectionWithText(_newLine);
        }
    }

    private void OnTemplateApplied(object? sender, Avalonia.Controls.Primitives.TemplateAppliedEventArgs e)
    {
        _viewer = e.NameScope.Find<ScrollViewer>("PART_ScrollViewer");
        if (_viewer is not null && _pendingOffset is { } offset)
        {
            _pendingOffset = null;
            _viewer.Offset = new Vector(_viewer.Offset.X, offset);
        }
    }

    private void OnScrollOffsetChanged(object? sender, EventArgs e)
    {
        var offset = _editor.VerticalOffset;
        if (offset != _lastOffset)
        {
            _lastOffset = offset;
            Scrolled?.Invoke(offset);
        }
    }
}
