using Avalonia.Controls;
using Resolve.Core.Resolution;

namespace Resolve.App.Editor;

/// <summary>How a region is tinted; each tint also draws its own left-edge glyph so colour is never the only signal.</summary>
public enum RegionTint { Unresolved, FromLeft, FromRight, Both, Manual, Applied }

/// <summary>Lines [StartLine, EndLine): zero-based, end exclusive.</summary>
public readonly record struct Decoration(int StartLine, int EndLine, RegionTint Tint);

/// <summary>
/// One text pane. It only displays text, reports edits as <see cref="TextChange"/>s and applies the changes the
/// document hands back; it owns no undo and no region logic.
/// </summary>
public interface IEditorSurface
{
    bool IsReadOnly { get; }

    double LineToY(int line);

    int YToLine(double y);

    double VerticalOffset { get; }

    double ViewportHeight { get; }

    void ScrollTo(double y);

    /// <summary>Applies changes to the text without raising <see cref="Edited"/>.</summary>
    void ApplyChanges(IReadOnlyList<TextChange> changes);

    void SetDecorations(IReadOnlyList<Decoration> decorations);

    event Action<TextChange>? Edited;

    event Action<double>? Scrolled;

    Control View { get; }
}
