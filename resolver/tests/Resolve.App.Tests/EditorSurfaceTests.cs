using Avalonia.Controls;
using Avalonia.Headless.XUnit;
using Avalonia.Headless;
using Avalonia.Input;
using Resolve.App.Editor;
using Resolve.Core.Resolution;

namespace Resolve.App.Tests;

public class EditorSurfaceTests
{
    private static (Window Window, AvaloniaEditSurface Surface) Host(string text, bool readOnly = false, string newLine = "\n")
    {
        var surface = new AvaloniaEditSurface(text, readOnly, newLine);
        var window = new Window { Width = 600, Height = 400, Content = surface.View };
        window.Show();
        window.UpdateLayout();
        surface.Focus();
        Assert.True(window.FocusManager?.GetFocusedElement() is not null, "nothing focused");
        return (window, surface);
    }

    [AvaloniaFact]
    public void Typing_raises_one_edited_change()
    {
        var (window, surface) = Host("");
        var edits = new List<TextChange>();
        surface.Edited += edits.Add;
        window.KeyTextInput("a");
        window.KeyTextInput("b");
        Assert.Equal([new TextChange(0, 0, "a"), new TextChange(1, 0, "b")], edits);
    }

    [AvaloniaFact]
    public void Applied_changes_do_not_echo_as_edits()
    {
        var (_, surface) = Host("hello world");
        var edits = new List<TextChange>();
        surface.Edited += edits.Add;
        surface.ApplyChanges([new TextChange(0, 5, "HEY"), new TextChange(3, 0, "!")]);
        Assert.Empty(edits);
        Assert.Equal("HEY! world", surface.Text);
    }

    [AvaloniaFact]
    public void Enter_inserts_the_configured_newline()
    {
        var (window, surface) = Host("", newLine: "\r\n");
        var edits = new List<TextChange>();
        surface.Edited += edits.Add;
        window.KeyPressQwerty(PhysicalKey.Enter, RawInputModifiers.None);
        Assert.Equal([new TextChange(0, 0, "\r\n")], edits);
    }

    [AvaloniaFact]
    public void Read_only_surfaces_ignore_typing()
    {
        var (window, surface) = Host("fixed", readOnly: true);
        var edits = new List<TextChange>();
        surface.Edited += edits.Add;
        window.KeyTextInput("x");
        window.KeyPressQwerty(PhysicalKey.Enter, RawInputModifiers.None);
        Assert.True(surface.IsReadOnly);
        Assert.Empty(edits);
        Assert.Equal("fixed", surface.Text);
    }

    [AvaloniaFact]
    public void Scroll_to_moves_the_view_and_reports_it()
    {
        var text = string.Join("\n", Enumerable.Range(0, 500).Select(i => $"line {i}"));
        var (window, surface) = Host(text);
        window.UpdateLayout();
        var scrolled = new List<double>();
        surface.Scrolled += scrolled.Add;
        surface.ScrollTo(surface.LineToY(200));
        window.UpdateLayout();
        Assert.Equal(200, surface.YToLine(surface.VerticalOffset));
        Assert.NotEmpty(scrolled);
        Assert.True(surface.ViewportHeight > 0);
    }

    [AvaloniaFact]
    public void Decorations_cover_their_lines_only_and_can_be_replaced()
    {
        var (window, surface) = Host(string.Join("\n", Enumerable.Range(0, 50).Select(i => $"line {i}")));
        // Given out of order and with an empty range: lookup must still be by line, end exclusive.
        surface.SetDecorations([new Decoration(5, 6, RegionTint.Unresolved), new Decoration(1, 3, RegionTint.FromLeft), new Decoration(9, 9, RegionTint.Both)]);
        window.UpdateLayout();
        Assert.Null(surface.TintAt(0));
        Assert.Equal(RegionTint.FromLeft, surface.TintAt(1));
        Assert.Equal(RegionTint.FromLeft, surface.TintAt(2));
        Assert.Null(surface.TintAt(3));
        Assert.Equal(RegionTint.Unresolved, surface.TintAt(5));
        Assert.Null(surface.TintAt(6));
        Assert.Null(surface.TintAt(9));
        surface.SetDecorations([]);
        window.UpdateLayout();
        Assert.Null(surface.TintAt(1));
    }
}
