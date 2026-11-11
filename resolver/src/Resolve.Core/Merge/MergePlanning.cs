using Resolve.Core.Diff;
using Resolve.Core.Git;
using Resolve.Core.Session;
using Resolve.Core.Text;

namespace Resolve.Core.Merge;

/// <summary>
/// Plans a merge-tool session from its inputs. When git can read every input as the same UTF-8 flavour, the edit
/// scripts come from git's own histogram diff over the files it passed, so the regions are the conflicts git reports.
/// Otherwise (UTF-16/32, or differing BOMs) git's byte-wise diff would not see the decoded lines, so it plans in-process.
/// </summary>
public static class MergePlanning
{
    public static async Task<MergePlan> PlanAsync(IGitRunner runner, MergeInputs inputs, CancellationToken ct)
    {
        ArgumentNullException.ThrowIfNull(runner);
        ArgumentNullException.ThrowIfNull(inputs);

        var interner = new LineInterner();
        var baseIds = interner.Intern(inputs.Base.Lines);
        var leftIds = interner.Intern(inputs.Left.Lines);
        var rightIds = interner.Intern(inputs.Right.Lines);

        if (!GitCanDiff(inputs))
        {
            return Diff3Planner.Plan(baseIds, leftIds, rightIds, DiffAlgorithm.Histogram);
        }

        var workingDirectory = Path.GetDirectoryName(Path.GetFullPath(inputs.LeftFile)) ?? Path.GetTempPath();
        string? emptyDirectory = null;
        try
        {
            var basePath = inputs.BaseKind == BaseKind.Present ? inputs.BaseFile : null;
            if (basePath is null)
            {
                emptyDirectory = Directory.CreateTempSubdirectory("cthulhu-resolve-").FullName;
                basePath = Path.Join(emptyDirectory, "empty");
                await File.WriteAllBytesAsync(basePath, [], ct).ConfigureAwait(false);
            }
            var leftEdits = await GitLineDiff.ComputeAsync(runner, workingDirectory, basePath, inputs.LeftFile, DiffAlgorithm.Histogram, ct).ConfigureAwait(false);
            var rightEdits = await GitLineDiff.ComputeAsync(runner, workingDirectory, basePath, inputs.RightFile, DiffAlgorithm.Histogram, ct).ConfigureAwait(false);
            return Diff3Planner.Plan(baseIds, leftIds, rightIds, leftEdits, rightEdits, DiffAlgorithm.Histogram);
        }
        finally
        {
            if (emptyDirectory is not null)
            {
                Directory.Delete(emptyDirectory, recursive: true);
            }
        }
    }

    /// <summary>Every input with content shares one encoding, and it is UTF-8 with or without a BOM.</summary>
    private static bool GitCanDiff(MergeInputs inputs)
    {
        var encodings = new[] { inputs.Base, inputs.Left, inputs.Right }
            .Where(text => !text.Lines.IsEmpty)
            .Select(text => text.Encoding)
            .Distinct()
            .ToList();
        return encodings.Count <= 1 && encodings.All(kind => kind is TextEncodingKind.Utf8 or TextEncodingKind.Utf8Bom);
    }
}
