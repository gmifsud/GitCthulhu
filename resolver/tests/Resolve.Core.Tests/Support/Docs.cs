using Resolve.Core.Diff;
using Resolve.Core.Merge;
using Resolve.Core.Resolution;
using Resolve.Core.Text;

namespace Resolve.Core.Tests.Support;

/// <summary>Builds resolution documents from in-memory lines. No xUnit types: a UI test project may link this file.</summary>
internal static class Docs
{
    /// <summary>Base of 50 lines; LEFT changes lines 3 and 20; RIGHT changes lines 20 and 40 (one conflict, two clean changes).</summary>
    public static ResolutionDocument Criterion1(TimeProvider? clock = null)
    {
        var baseLines = Enumerable.Range(1, 50).Select(number => $"line {number}").ToArray();
        var left = (string[])baseLines.Clone();
        left[2] = "left 3";
        left[19] = "left 20";
        var right = (string[])baseLines.Clone();
        right[19] = "right 20";
        right[39] = "right 40";
        return Build(Lf(baseLines), Lf(left), Lf(right), clock);
    }

    /// <summary>One conflict on the last line; the right side's last line has no terminator.</summary>
    public static ResolutionDocument NoFinalNewline() =>
        Build(Lf("a", "b", "c"), Lf("a", "b", "left c"), [.. Lf("a", "b"), new Line("right c", LineEnding.None)]);

    /// <summary>
    /// Applies one pseudo-random operation (<paramref name="op"/> in 0..5) derived from the document's current
    /// state, so a sequence of ops is deterministic. Mixes edits, accepts, resets, acknowledgement and apply-all.
    /// </summary>
    public static void RandomOp(ResolutionDocument doc, int op)
    {
        ArgumentNullException.ThrowIfNull(doc);
        var conflict = doc.Regions.First(view => view.Kind == RegionKind.Conflict).RegionId;
        var length = doc.Text.Length;
        var seed = (doc.Version * 37) + op;
        switch (op)
        {
            case 0:
                doc.Edit(new TextChange(seed % (length + 1), 0, "ab\n"));
                break;
            case 1 when length > 0:
                doc.Edit(new TextChange(seed % length, 1, ""));
                break;
            case 2:
                doc.Apply(new RegionCommand.Accept(conflict, ResolutionChoice.Left), confirmed: true);
                break;
            case 3:
                doc.Apply(new RegionCommand.Accept(conflict, seed % 2 == 0 ? ResolutionChoice.LeftThenRight : ResolutionChoice.Right), confirmed: true);
                break;
            case 4:
                doc.Apply(new RegionCommand.Reset(seed % doc.Regions.Count), confirmed: true);
                break;
            case 5:
                doc.Apply(seed % 2 == 0 ? new RegionCommand.Acknowledge(conflict) : new RegionCommand.ApplyAllNonConflicting());
                break;
            default:
                break;
        }
    }

    public static string[] Lines(string text) => text.Split('\n');

    public static Line[] Lf(params string[] contents) => [.. contents.Select(content => new Line(content, LineEnding.Lf))];

    public static ResolutionDocument Build(Line[] baseLines, Line[] left, Line[] right, TimeProvider? clock = null) =>
        ResolutionDocument.Create(Diff3Planner.Plan(baseLines, left, right, DiffAlgorithm.Histogram), baseLines, left, right, clock ?? TimeProvider.System);
}
