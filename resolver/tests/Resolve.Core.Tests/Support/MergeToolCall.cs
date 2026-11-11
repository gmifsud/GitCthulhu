using System.Text;
using Resolve.Core.Cli;
using Resolve.Core.Session;
using Resolve.Core.Text;

namespace Resolve.Core.Tests.Support;

/// <summary>Builds what <c>git mergetool</c> would pass: stage files in a temp dir, plus the worktree path as MERGED. No xUnit types: a UI test project links this file.</summary>
internal static class MergeToolCall
{
    public static async Task<MergeToolArguments> WriteStageFilesAsync(GitFixture fixture, string path)
    {
        ArgumentNullException.ThrowIfNull(fixture);
        var result = await fixture.RunAsync("checkout-index", "--temp", "--stage=all", "-z", "--", path);
        var output = Encoding.UTF8.GetString(result.StandardOutput);
        // "<stage1> SP <stage2> SP <stage3> TAB <path> NUL"; a missing stage is spelled ".".
        var record = output.Split('\0', StringSplitOptions.RemoveEmptyEntries).SingleOrDefault()
            ?? throw new InvalidOperationException($"checkout-index produced no single record for '{path}'.");
        var tab = record.IndexOf('\t', StringComparison.Ordinal);
        var names = (tab < 0 ? record : record[..tab]).Split(' ');
        if (names.Length != 3)
        {
            throw new InvalidOperationException($"Unexpected checkout-index record: '{record}'.");
        }
        var directory = Directory.CreateTempSubdirectory("cthulhu-stages-").FullName;
        return new MergeToolArguments(
            Place(fixture, directory, names[0], "BASE"),
            Place(fixture, directory, names[1], "LOCAL"),
            Place(fixture, directory, names[2], "REMOTE"),
            Path.Join(fixture.Worktree, path),
            LabelBase: null,
            LabelLocal: null,
            LabelRemote: null,
            Repo: null,
            NoRepoContext: false);
    }

    private static string Place(GitFixture fixture, string directory, string name, string target)
    {
        var destination = Path.Join(directory, target);
        if (name == ".")
        {
            File.WriteAllBytes(destination, []);
        }
        else
        {
            File.Move(Path.IsPathRooted(name) ? name : Path.Join(fixture.Worktree, name), destination);
        }
        return destination;
    }
}

/// <summary>Builds <see cref="MergeInputs"/> straight from texts, with no repository. No xUnit types.</summary>
internal static class Inputs
{
    /// <summary>A null base text is an absent base (empty file, no stage 1).</summary>
    public static Task<MergeInputs> FromTextsAsync(
        string? baseText,
        string left,
        string right,
        TextEncodingKind leftEncoding = TextEncodingKind.Utf8,
        TextEncodingKind rightEncoding = TextEncodingKind.Utf8)
    {
        var directory = Directory.CreateTempSubdirectory("cthulhu-inputs-").FullName;
        var baseDecoded = Make(baseText ?? "", TextEncodingKind.Utf8);
        var leftDecoded = Make(left, leftEncoding);
        var rightDecoded = Make(right, rightEncoding);
        var basePath = Path.Join(directory, "BASE");
        var leftPath = Path.Join(directory, "LOCAL");
        var rightPath = Path.Join(directory, "REMOTE");
        var mergedPath = Path.Join(directory, "MERGED");
        File.WriteAllBytes(basePath, baseDecoded.Encode());
        File.WriteAllBytes(leftPath, leftDecoded.Encode());
        File.WriteAllBytes(rightPath, rightDecoded.Encode());
        File.WriteAllBytes(mergedPath, []);
        var labels = new SideLabels(OperationKind.Unknown, "Git stage 2", "Git stage 3", null, null, null);
        return Task.FromResult(new MergeInputs(
            mergedPath, basePath, leftPath, rightPath,
            Repo: null,
            RelativePath: null,
            baseText is null ? BaseKind.Absent : BaseKind.Present,
            baseDecoded, leftDecoded, rightDecoded, labels,
            MergedAtLoad: null));
    }

    private static DecodedText Make(string text, TextEncodingKind encoding) => new(encoding, TextCodec.SplitLines(text));
}
