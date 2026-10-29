using System.Globalization;
using System.Text;
using System.Text.RegularExpressions;
using Resolve.Core.Git;

namespace Resolve.Core.Diff;

/// <summary>
/// Line edits computed by git itself (<c>git diff --no-index -U0</c>), so a plan built from them has exactly the
/// regions git's own merge machinery produces. Only hunk headers are read; content lines are never parsed.
/// Every option that could change the alignment is pinned, so the user's diff config cannot alter it.
/// </summary>
public static partial class GitLineDiff
{
    public static async Task<IReadOnlyList<Edit>> ComputeAsync(
        IGitRunner runner, string workingDirectory, string pathA, string pathB, DiffAlgorithm algorithm, CancellationToken cancellationToken)
    {
        ArgumentNullException.ThrowIfNull(runner);
        var arguments = new[]
        {
            "diff", "--no-index", "--no-color", "--no-ext-diff", "--no-textconv", "--text", "--no-renames",
            "--no-indent-heuristic", algorithm == DiffAlgorithm.Histogram ? "--diff-algorithm=histogram" : "--diff-algorithm=myers",
            "-U0", "--", pathA, pathB,
        };
        var result = await runner.RunAsync(new GitInvocation(workingDirectory, arguments) { ReadOnly = true }, cancellationToken).ConfigureAwait(false);
        // --no-index exits 1 when the files differ; anything else is an error.
        if (result.ExitCode is not (0 or 1))
        {
            throw new GitCommandException(arguments, result);
        }
        return ParseHunkHeaders(result.StandardOutput);
    }

    /// <summary>
    /// "@@ -a[,b] +c[,d] @@": b or d of 0 means an empty range positioned *after* line a (or c), so its
    /// zero-based start is a, not a - 1. An omitted count is 1.
    /// </summary>
    public static IReadOnlyList<Edit> ParseHunkHeaders(ReadOnlySpan<byte> unifiedDiff)
    {
        var edits = new List<Edit>();
        foreach (Match match in HunkHeader().Matches(Encoding.Latin1.GetString(unifiedDiff)))
        {
            var (aStart, aEnd) = Range(match.Groups[1].Value, match.Groups[2].Value);
            var (bStart, bEnd) = Range(match.Groups[3].Value, match.Groups[4].Value);
            edits.Add(new Edit(aStart, aEnd, bStart, bEnd));
        }
        return LineDiff.Normalise(edits);
    }

    private static (int Start, int End) Range(string start, string count)
    {
        var line = int.Parse(start, CultureInfo.InvariantCulture);
        var length = count.Length == 0 ? 1 : int.Parse(count, CultureInfo.InvariantCulture);
        var zeroBased = length == 0 ? line : line - 1;
        return (zeroBased, zeroBased + length);
    }

    [GeneratedRegex(@"^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@", RegexOptions.Multiline)]
    private static partial Regex HunkHeader();
}
