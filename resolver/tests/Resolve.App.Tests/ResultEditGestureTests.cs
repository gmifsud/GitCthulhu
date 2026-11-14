using Avalonia.VisualTree;
using Avalonia.Headless;
using Avalonia.Headless.XUnit;
using Avalonia.Input;
using Avalonia.Threading;
using Resolve.App.Editor;
using Resolve.App.Shell;
using Resolve.App.Tests.Support;
using Resolve.Core.Merge;
using Resolve.Core.Resolution;

namespace Resolve.App.Tests;

/// <summary>
/// Final review C1 / ruling R27: ordinary RESULT edits whose region boundaries land mid-line made two regions share a
/// line, the line maps threw from inside AvaloniaEdit's input handling, and the session was lost. Every gesture here is
/// real key input on a fixture with an empty-base (insert/insert) conflict; after each, RESULT shows exactly the
/// document's text, nothing threw, and nothing had to be recovered.
/// </summary>
public class ResultEditGestureTests
{
    private static AvaloniaEditSurface Result(MergeToolWindow window) => (AvaloniaEditSurface)window.View.Result;

    private static RegionView InsertConflict(MergeToolWindow window) =>
        window.Session.Document.Regions.First(view => view.Kind == RegionKind.Conflict);

    private static RegionView ModifyConflict(MergeToolWindow window) =>
        window.Session.Document.Regions.Last(view => view.Kind == RegionKind.Conflict);

    private static void Press(MergeToolWindow window, PhysicalKey key, RawInputModifiers modifiers = RawInputModifiers.None)
    {
        window.KeyPressQwerty(key, modifiers);
        window.KeyReleaseQwerty(key, modifiers);
    }

    private static void Caret(MergeToolWindow window, int offset) => Result(window).CaretOffset = offset;

    /// <summary>
    /// The platform's command modifier: Ctrl, or Cmd on macOS, where the editor's select-all is
    /// Cmd+A and Ctrl+A selected nothing (so typing went in beside the old text).
    /// </summary>
    private static RawInputModifiers Command(MergeToolWindow window)
    {
        var modifiers = window.GetPlatformSettings()?.HotkeyConfiguration.CommandModifiers ?? KeyModifiers.Control;
        var raw = RawInputModifiers.None;
        if (modifiers.HasFlag(KeyModifiers.Control)) raw |= RawInputModifiers.Control;
        if (modifiers.HasFlag(KeyModifiers.Meta)) raw |= RawInputModifiers.Meta;
        return raw;
    }

    private static void AssertInStep(MergeToolWindow window, string gesture)
    {
        Dispatcher.UIThread.RunJobs();
        Assert.Equal(window.Session.Document.Text, Result(window).Text);
        // The fix is that nothing fails, not that a failure is recovered from.
        Assert.DoesNotContain("Something went wrong", window.Navigator.Status.Text ?? string.Empty, StringComparison.Ordinal);
        var (left, right) = PaneMaps.For(window.Session.Plan, window.Session.Document);
        Assert.True(left.Blocks.Count > 0 && right.Blocks.Count > 0, gesture);
    }

    /// <summary>Backspace at the first column of every region that starts a line, recomputing positions after each join.</summary>
    private static void BackspaceAtEveryBoundary(MergeToolWindow window)
    {
        for (var index = 1; index < window.Session.Document.Regions.Count; index++)
        {
            var start = window.Session.Document.Regions[index].Start;
            if (start > 0 && window.Session.Document.Text[start - 1] == '\n')
            {
                Caret(window, start);
                Press(window, PhysicalKey.Backspace);
                AssertInStep(window, $"backspace at region {index}");
            }
        }
    }

    /// <summary>Delete at the end of the line before every region that starts a line.</summary>
    private static void DeleteBeforeEveryBoundary(MergeToolWindow window)
    {
        for (var index = 1; index < window.Session.Document.Regions.Count; index++)
        {
            var start = window.Session.Document.Regions[index].Start;
            if (start > 0 && window.Session.Document.Text[start - 1] == '\n')
            {
                Caret(window, start - 1);
                Press(window, PhysicalKey.Delete);
                AssertInStep(window, $"delete before region {index}");
            }
        }
    }

    [AvaloniaTheory]
    [InlineData("type one character into the empty insert/insert conflict")]
    [InlineData("backspace at the first column of a conflict line")]
    [InlineData("delete at the end of a conflict line")]
    [InlineData("delete at the end of an applied change line")]
    [InlineData("join lines with backspace at every region boundary")]
    [InlineData("join lines with delete at every region boundary")]
    [InlineData("type into an empty region, then more on its line")]
    [InlineData("delete a selection from mid-line to mid-line across regions")]
    [InlineData("select all and type")]
    [InlineData("select all, delete, then type")]
    public void Gesture_keeps_result_in_step_with_the_document(string gesture)
    {
        var w = MergeWindows.Show(Sessions.InsertInsert(), _ => Task.FromResult(true));
        var insert = InsertConflict(w);
        Assert.Equal(insert.Start, insert.End);
        Result(w).Focus();

        switch (gesture)
        {
            case "type one character into the empty insert/insert conflict":
                Caret(w, insert.Start);
                w.KeyTextInput("x");
                Assert.Equal("x", w.Session.Document.Text[InsertConflict(w).Start..InsertConflict(w).End]);
                break;
            case "backspace at the first column of a conflict line":
                Caret(w, ModifyConflict(w).Start);
                Press(w, PhysicalKey.Backspace);
                break;
            case "delete at the end of a conflict line":
                Caret(w, ModifyConflict(w).End - 1);
                Press(w, PhysicalKey.Delete);
                break;
            case "delete at the end of an applied change line":
                Caret(w, w.Session.Document.Regions.First(view => view.Kind == RegionKind.LeftOnly).End - 1);
                Press(w, PhysicalKey.Delete);
                break;
            case "join lines with backspace at every region boundary":
                BackspaceAtEveryBoundary(w);
                break;
            case "join lines with delete at every region boundary":
                DeleteBeforeEveryBoundary(w);
                break;
            case "type into an empty region, then more on its line":
                Caret(w, insert.Start);
                w.KeyTextInput("p");
                AssertInStep(w, gesture);
                w.KeyTextInput("q");
                AssertInStep(w, gesture);
                Press(w, PhysicalKey.Enter);
                break;
            case "delete a selection from mid-line to mid-line across regions":
                var from = w.Session.Document.Regions.First(view => view.Kind == RegionKind.LeftOnly).Start + 2;
                var to = ModifyConflict(w).Start + 1;
                Result(w).Select(from, to - from);
                Press(w, PhysicalKey.Delete);
                break;
            case "select all and type":
                Press(w, PhysicalKey.A, Command(w));
                w.KeyTextInput("z");
                Assert.Equal("z", w.Session.Document.Text);
                break;
            case "select all, delete, then type":
                Press(w, PhysicalKey.A, Command(w));
                Press(w, PhysicalKey.Delete);
                AssertInStep(w, gesture);
                w.KeyTextInput("q");
                Press(w, PhysicalKey.Enter);
                break;
            default:
                throw new ArgumentOutOfRangeException(nameof(gesture), gesture, "Unknown gesture.");
        }

        AssertInStep(w, gesture);
        Assert.Null(w.Result);
    }
}
