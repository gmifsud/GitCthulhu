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
        document.Changed += OnDocumentChanged;
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

    private void OnDocumentChanged(object? sender, DocumentChangeEventArgs e)
    {
        if (!_applying)
        {
            Edited?.Invoke(new TextChange(e.Offset, e.RemovalLength, e.InsertedText.Text));
        }
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
