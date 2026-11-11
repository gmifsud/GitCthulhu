using Resolve.Core.Diff;
using Resolve.Core.Merge;
using Resolve.Core.Resolution;
using Resolve.Core.Text;

namespace Resolve.Core.Tests.Support;

/// <summary>Builds resolution documents from in-memory lines. No xUnit types: a UI test project may link this file.</summary>
internal static class Docs
{
    /// <summary>Base of 50 lines; LEFT changes lines 3 and 20; RIGHT changes lines 20 and 40 (one conflict, two clean changes).</summary>
    public static ResolutionDocument Criterion1()
    {
        var baseLines = Enumerable.Range(1, 50).Select(number => $"line {number}").ToArray();
        var left = (string[])baseLines.Clone();
        left[2] = "left 3";
        left[19] = "left 20";
        var right = (string[])baseLines.Clone();
        right[19] = "right 20";
        right[39] = "right 40";
        return Build(Lf(baseLines), Lf(left), Lf(right));
    }

    /// <summary>One conflict on the last line; the right side's last line has no terminator.</summary>
    public static ResolutionDocument NoFinalNewline() =>
        Build(Lf("a", "b", "c"), Lf("a", "b", "left c"), [.. Lf("a", "b"), new Line("right c", LineEnding.None)]);

    public static string[] Lines(string text) => text.Split('\n');

    public static Line[] Lf(params string[] contents) => [.. contents.Select(content => new Line(content, LineEnding.Lf))];

    public static ResolutionDocument Build(Line[] baseLines, Line[] left, Line[] right) =>
        ResolutionDocument.Create(Diff3Planner.Plan(baseLines, left, right, DiffAlgorithm.Histogram), baseLines, left, right, TimeProvider.System);
}
