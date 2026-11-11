using Avalonia.Controls;
using Avalonia.Headless;
using Avalonia.Headless.XUnit;
using Avalonia.Input;
using Avalonia.Threading;
using AvaloniaEdit;
using Resolve.App.Editor;
using Resolve.App.Panes;
using Resolve.App.Tests.Support;
using Resolve.Core.Session;

namespace Resolve.App.Tests;

public class ThreePaneViewTests
{
    private static (Window Window, ThreePaneView View) Host(MergeToolSession session)
    {
        var view = new ThreePaneView(session);
        var window = new Window { Width = 1200, Height = 600, Content = view };
        window.Show();
        window.UpdateLayout();
        return (window, view);
    }

    [AvaloniaFact]
    public void Scrolling_left_in_an_equal_run_scrolls_result_one_to_one()
    {
        var (window, view) = Host(Sessions.Criterion1());
        // Lines 4-19 (zero-based 3..18) are unchanged on both sides: the maps are the identity there.
        view.Left.ScrollTo(view.Left.LineToY(5));
        window.UpdateLayout();
        Assert.Equal(view.Result.LineToY(5), view.Result.VerticalOffset, 1);
        Assert.Equal(view.Right.LineToY(5), view.Right.VerticalOffset, 1);
    }

    [AvaloniaFact]
    public void A_typed_line_in_result_shifts_the_maps_and_keeps_bands_attached()
    {
        var session = Sessions.Criterion1();
        var (window, view) = Host(session);
        var before = view.LeftBand.Blocks[0];
        Assert.Equal((2, 3), (before.Result.Start, before.Result.End));

        ((AvaloniaEditSurface)view.Result).Focus();
        window.KeyPressQwerty(PhysicalKey.Enter, RawInputModifiers.None);
        window.UpdateLayout();

        var region = session.Document.Regions[1];
        foreach (var band in new[] { view.LeftBand, view.RightBand })
        {
            var block = band.Blocks[0];
            Assert.Equal(3, block.Result.Start);
            Assert.Equal((region.ResultLines.Start, region.ResultLines.End), (block.Result.Start, block.Result.End));
        }
        Assert.Equal(session.Document.Text, ((AvaloniaEditSurface)view.Result).Text);
        // Scrolling maps through the shifted equal run: LEFT line 10 sits at RESULT line 11.
        view.Left.ScrollTo(view.Left.LineToY(10));
        window.UpdateLayout();
        Assert.Equal(view.Result.LineToY(11), view.Result.VerticalOffset, 1);
    }

    [AvaloniaFact]
    public void Bands_are_drawn_only_for_visible_non_equal_blocks()
    {
        var (window, view) = Host(Sessions.Long());
        view.Left.ScrollTo(0);
        window.UpdateLayout();

        var total = view.LeftBand.Blocks.Count;
        var firstLine = view.Left.YToLine(view.Left.VerticalOffset);
        var lastLine = view.Left.YToLine(view.Left.VerticalOffset + view.Left.ViewportHeight);
        var expected = view.LeftBand.Blocks.Count(block => block.Side.End >= firstLine && block.Side.Start <= lastLine);
        Assert.True(total > 20, $"expected many blocks in the long file, got {total}");
        Assert.True(expected > 0, "the top of the file holds at least one block");
        Assert.Equal(expected, view.LeftBand.VisibleBlockCount);
        Assert.True(view.LeftBand.VisibleBlockCount < total / 4);
    }

    [AvaloniaFact]
    public void Sync_off_scrolls_panes_independently()
    {
        var (window, view) = Host(Sessions.Criterion1());
        view.SyncScroll = false;
        view.Left.ScrollTo(view.Left.LineToY(5));
        window.UpdateLayout();
        Assert.Equal(view.Left.LineToY(5), view.Left.VerticalOffset, 1);
        Assert.Equal(0, view.Result.VerticalOffset);
        Assert.Equal(0, view.Right.VerticalOffset);
    }

    [AvaloniaFact]
    public void An_edit_with_a_bare_newline_in_a_crlf_document_leaves_editor_and_document_identical()
    {
        var session = Sessions.Crlf();
        var (_, view) = Host(session);
        Assert.Equal("\r\n", session.Document.NewLine);

        ((TextEditor)view.Result.View).Document.Insert(0, "a\nb");
        Dispatcher.UIThread.RunJobs(); // the correction is posted: the editor cannot change inside its own change event

        var surface = (AvaloniaEditSurface)view.Result;
        Assert.Equal(session.Document.Text, surface.Text);
        Assert.StartsWith("a\r\nb", surface.Text, StringComparison.Ordinal);
    }

    [AvaloniaFact]
    public void Regions_are_tinted_from_their_state_in_all_three_panes()
    {
        var (_, view) = Host(Sessions.Criterion1());
        var result = (AvaloniaEditSurface)view.Result;
        var left = (AvaloniaEditSurface)view.Left;
        var right = (AvaloniaEditSurface)view.Right;
        Assert.Null(result.TintAt(0));
        Assert.Equal(RegionTint.Applied, result.TintAt(2));
        Assert.Equal(RegionTint.Applied, left.TintAt(2));
        Assert.Equal(RegionTint.Unresolved, result.TintAt(19));
        Assert.Equal(RegionTint.Unresolved, right.TintAt(19));
        Assert.Equal(RegionTint.Applied, right.TintAt(39));
    }
}
