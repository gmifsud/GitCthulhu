using Avalonia.Controls;
using Avalonia.Headless;
using Avalonia.Headless.XUnit;
using Avalonia.Input;
using Avalonia.Threading;
using Resolve.Core.Resolution;
using Resolve.Core.Merge;
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
        // No dispatcher turn: the correction is made synchronously inside the editor's UpdateFinished.

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

    [AvaloniaFact]
    public void Two_inserts_in_one_editor_update_reach_the_document_as_one_corrected_edit()
    {
        var session = Sessions.Crlf();
        var (_, view) = Host(session);
        var surface = (AvaloniaEditSurface)view.Result;
        var edits = new List<TextChange>();
        surface.Edited += edits.Add;
        var editor = ((TextEditor)view.Result.View).Document;

        editor.BeginUpdate();
        editor.Insert(0, "\n");
        editor.Insert(1, "  ");
        editor.EndUpdate();

        Assert.Equal([new TextChange(0, 0, "\n  ")], edits);
        Assert.Equal(session.Document.Text, surface.Text);
        Assert.StartsWith("\r\n  line 1", surface.Text, StringComparison.Ordinal);
    }

    [AvaloniaFact]
    public void An_unexpected_edit_resyncs_result_from_the_document_instead_of_throwing()
    {
        var session = Sessions.Criterion1();
        var (_, view) = Host(session);
        var surface = (AvaloniaEditSurface)view.Result;
        ((TextEditor)view.Result.View).Document.Insert(0, "stray");   // editor now differs from the document
        session.Document.Edit(new TextChange(0, 5, ""));               // ...whose own text is back to the original

        view.HandleResultEdit(new TextChange(99999, 0, "x"));         // the document rejects it

        Assert.Equal(session.Document.Text, surface.Text);
    }

    [AvaloniaFact]
    public void Sync_off_clamps_band_endpoints_to_the_viewport()
    {
        var (window, view) = Host(Sessions.Long());
        view.SyncScroll = false;
        view.Left.ScrollTo(0);
        view.Result.ScrollTo(view.Result.LineToY(1000));
        window.UpdateLayout();

        var height = view.LeftBand.Bounds.Height;
        var quads = view.LeftBand.Quads().ToList();
        Assert.NotEmpty(quads);
        Assert.All(quads, quad => Assert.All(
            [quad.SideStart, quad.SideEnd, quad.ResultStart, quad.ResultEnd],
            y => Assert.InRange(y, 0, height)));
        Assert.Contains(quads, quad => quad.SideStart == 0 || quad.ResultStart == height || quad.ResultStart == 0);
    }

    [AvaloniaFact]
    public void Sync_off_counts_blocks_in_both_windows_only()
    {
        var (window, view) = Host(Sessions.Long());
        view.SyncScroll = false;
        view.Left.ScrollTo(0);
        view.Result.ScrollTo(view.Result.LineToY(1000));
        window.UpdateLayout();

        int Visible(IEditorSurface pane, Func<BandBlock, LineRange> range)
        {
            var first = pane.YToLine(pane.VerticalOffset);
            var last = pane.YToLine(pane.VerticalOffset + pane.ViewportHeight);
            return view.LeftBand.Blocks.Select((block, index) => (block, index))
                .Count(entry => range(entry.block).End >= first && range(entry.block).Start <= last);
        }
        var inBoth = view.LeftBand.Blocks.Count(block =>
            Overlaps(view.Left, block.Side) && Overlaps(view.Result, block.Result));
        Assert.Equal(Visible(view.Left, b => b.Side) + Visible(view.Result, b => b.Result) - inBoth, view.LeftBand.VisibleBlockCount);

        static bool Overlaps(IEditorSurface pane, LineRange range) =>
            range.End >= pane.YToLine(pane.VerticalOffset) && range.Start <= pane.YToLine(pane.VerticalOffset + pane.ViewportHeight);
    }

    [AvaloniaFact]
    public void Accepting_left_right_both_and_editing_manually_tint_all_three_panes()
    {
        var session = Sessions.Criterion1();
        var (_, view) = Host(session);
        var conflict = session.Document.Regions.First(region => region.Kind == RegionKind.Conflict).RegionId;

        void Check(RegionTint tint)
        {
            var region = session.Document.Regions[conflict];
            var planned = session.Plan.Regions[conflict];
            Assert.Equal(tint, ((AvaloniaEditSurface)view.Result).TintAt(region.ResultLines.Start));
            Assert.Equal(tint, ((AvaloniaEditSurface)view.Left).TintAt(planned.Left.Start));
            Assert.Equal(tint, ((AvaloniaEditSurface)view.Right).TintAt(planned.Right.Start));
            Assert.All(view.LeftBand.Blocks.Concat(view.RightBand.Blocks).Where(block => block.Tint == tint), _ => { });
            Assert.Contains(view.LeftBand.Blocks, block => block.Tint == tint);
        }

        foreach (var (choice, tint) in new[]
        {
            (ResolutionChoice.Left, RegionTint.FromLeft),
            (ResolutionChoice.Right, RegionTint.FromRight),
            (ResolutionChoice.LeftThenRight, RegionTint.Both),
        })
        {
            var outcome = Assert.IsType<CommandOutcome.Changed>(session.Document.Apply(new RegionCommand.Accept(conflict, choice)));
            view.Refresh(outcome.Changes);
            Check(tint);
        }

        ((TextEditor)view.Result.View).Document.Insert(session.Document.Regions[conflict].Start, "x");
        Check(RegionTint.Manual);
    }
}
