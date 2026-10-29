using System.Text;
using Resolve.Core.Text;

namespace Resolve.Core.Merge;

/// <summary>Conflict labels written into marker lines.</summary>
public sealed record MarkerLabels(string Left, string Base, string Right);

/// <summary>
/// Renders a plan as git's diff3-style conflict markers. Used as the bridge to the <c>git merge-file --diff3</c>
/// oracle and for "export with markers"; the resolver itself never writes markers into a result.
/// </summary>
public static class MarkerRenderer
{
    public const int MarkerSize = 7;

    public static string RenderDiff3(MergePlan plan, IReadOnlyList<Line> baseLines, IReadOnlyList<Line> left, IReadOnlyList<Line> right, MarkerLabels labels)
    {
        ArgumentNullException.ThrowIfNull(plan);
        ArgumentNullException.ThrowIfNull(labels);
        var output = new StringBuilder();
        foreach (var region in plan.Regions)
        {
            switch (region.Kind)
            {
                case RegionKind.Equal:
                case RegionKind.LeftOnly:
                case RegionKind.BothSame:
                    Append(output, left, region.Left);
                    break;
                case RegionKind.RightOnly:
                    Append(output, right, region.Right);
                    break;
                case RegionKind.Conflict:
                    Marker(output, '<', labels.Left);
                    EndSection(output, Append(output, left, region.Left));
                    Marker(output, '|', labels.Base);
                    EndSection(output, Append(output, baseLines, region.Base));
                    Marker(output, '=', null);
                    EndSection(output, Append(output, right, region.Right));
                    Marker(output, '>', labels.Right);
                    break;
                default:
                    throw new InvalidOperationException($"Unknown region kind {region.Kind}.");
            }
        }
        return output.ToString();
    }

    /// <summary>Returns whether the section's last line lacked a newline (git then adds one before the next marker).</summary>
    private static bool Append(StringBuilder output, IReadOnlyList<Line> lines, LineRange range)
    {
        var missingNewline = false;
        for (var index = range.Start; index < range.End; index++)
        {
            output.Append(lines[index].Content).Append(lines[index].Terminator);
            missingNewline = lines[index].Ending == LineEnding.None;
        }
        return missingNewline;
    }

    private static void EndSection(StringBuilder output, bool missingNewline)
    {
        if (missingNewline)
        {
            output.Append('\n');
        }
    }

    private static void Marker(StringBuilder output, char mark, string? label)
    {
        output.Append(mark, MarkerSize);
        if (label is not null)
        {
            output.Append(' ').Append(label);
        }
        output.Append('\n');
    }
}
