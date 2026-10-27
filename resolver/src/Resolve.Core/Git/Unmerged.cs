using System.Globalization;
using System.Text;

namespace Resolve.Core.Git;

public enum Stage : byte
{
    Base = 1,
    Left = 2,
    Right = 3,
}

/// <summary>Git object mode as recorded in the index (octal).</summary>
public readonly record struct GitMode(int Value)
{
    public static readonly GitMode Regular = new(Convert.ToInt32("100644", 8));
    public static readonly GitMode Executable = new(Convert.ToInt32("100755", 8));
    public static readonly GitMode Symlink = new(Convert.ToInt32("120000", 8));
    public static readonly GitMode Gitlink = new(Convert.ToInt32("160000", 8));

    public static GitMode ParseOctal(string text) => new(Convert.ToInt32(text, 8));

    public override string ToString() => Convert.ToString(Value, 8).PadLeft(6, '0');
}

public sealed record StageEntry(Stage Stage, GitMode Mode, string ObjectId);

/// <summary>Porcelain-v2 XY codes for unmerged entries, derived from which stages exist.</summary>
public enum UnmergedKind
{
    /// <summary>UU</summary>
    BothModified,
    /// <summary>AA</summary>
    BothAdded,
    /// <summary>DU — deleted on the left (stage 2 absent).</summary>
    DeletedByUs,
    /// <summary>UD — deleted on the right (stage 3 absent).</summary>
    DeletedByThem,
    /// <summary>AU</summary>
    AddedByUs,
    /// <summary>UA</summary>
    AddedByThem,
    /// <summary>DD</summary>
    BothDeleted,
}

public enum PathKind
{
    Regular,
    Symlink,
    Gitlink,
}

public sealed record UnmergedPath(RepoPath Path, StageEntry? Base, StageEntry? Left, StageEntry? Right)
{
    public UnmergedKind Kind => (Base, Left, Right) switch
    {
        (not null, not null, not null) => UnmergedKind.BothModified,
        (null, not null, not null) => UnmergedKind.BothAdded,
        (not null, null, not null) => UnmergedKind.DeletedByUs,
        (not null, not null, null) => UnmergedKind.DeletedByThem,
        (null, not null, null) => UnmergedKind.AddedByUs,
        (null, null, not null) => UnmergedKind.AddedByThem,
        (not null, null, null) => UnmergedKind.BothDeleted,
        _ => throw new InvalidOperationException($"'{Path}' has no stages."),
    };

    /// <summary>Gitlink beats symlink beats regular: any stage with the special mode makes the path special.</summary>
    public PathKind PathKind
    {
        get
        {
            var modes = new[] { Base, Left, Right }.Where(entry => entry is not null).Select(entry => entry!.Mode).ToArray();
            return modes.Contains(GitMode.Gitlink) ? PathKind.Gitlink
                : modes.Contains(GitMode.Symlink) ? PathKind.Symlink
                : PathKind.Regular;
        }
    }

    public StageEntry? this[Stage stage] => stage switch
    {
        Stage.Base => Base,
        Stage.Left => Left,
        Stage.Right => Right,
        _ => throw new ArgumentOutOfRangeException(nameof(stage)),
    };
}

/// <summary>Reads unmerged index entries. The index is authoritative; conflict markers are never parsed for inputs.</summary>
public static class UnmergedIndex
{
    public static async Task<IReadOnlyList<UnmergedPath>> ListAsync(IGitRunner runner, RepoLocation repo, CancellationToken cancellationToken)
    {
        ArgumentNullException.ThrowIfNull(runner);
        ArgumentNullException.ThrowIfNull(repo);
        var result = await runner.RunCheckedAsync(
            new GitInvocation(repo.Worktree, ["ls-files", "--unmerged", "-z", "--full-name"]) { ReadOnly = true },
            cancellationToken).ConfigureAwait(false);
        return Parse(result.StandardOutput);
    }

    /// <summary>
    /// Parses <c>ls-files -u -z</c>: records of <c>&lt;mode&gt; SP &lt;oid&gt; SP &lt;stage&gt; TAB &lt;path&gt; NUL</c>.
    /// Paths are decoded as strict UTF-8 (git stores bytes; core.quotepath=off keeps them raw).
    /// Result is ordered by path (ordinal) so output is deterministic.
    /// </summary>
    public static IReadOnlyList<UnmergedPath> Parse(ReadOnlySpan<byte> output)
    {
        var strict = new UTF8Encoding(encoderShouldEmitUTF8Identifier: false, throwOnInvalidBytes: true);
        var byPath = new SortedDictionary<string, StageEntry?[]>(StringComparer.Ordinal);
        foreach (var record in Split(output))
        {
            var text = strict.GetString(record);
            var tab = text.IndexOf('\t', StringComparison.Ordinal);
            if (tab < 0)
            {
                throw new FormatException($"Malformed ls-files record: '{text}'.");
            }
            var header = text[..tab].Split(' ');
            if (header.Length != 3)
            {
                throw new FormatException($"Malformed ls-files header: '{text[..tab]}'.");
            }
            var stage = (Stage)int.Parse(header[2], CultureInfo.InvariantCulture);
            if (stage is < Stage.Base or > Stage.Right)
            {
                throw new FormatException($"Unexpected stage {header[2]}.");
            }
            var path = text[(tab + 1)..];
            if (!byPath.TryGetValue(path, out var stages))
            {
                stages = new StageEntry?[3];
                byPath[path] = stages;
            }
            stages[(int)stage - 1] = new StageEntry(stage, GitMode.ParseOctal(header[0]), header[1]);
        }
        return byPath.Select(pair => new UnmergedPath(new RepoPath(pair.Key), pair.Value[0], pair.Value[1], pair.Value[2])).ToList();
    }

    private static List<byte[]> Split(ReadOnlySpan<byte> output)
    {
        var records = new List<byte[]>();
        while (!output.IsEmpty)
        {
            var end = output.IndexOf((byte)0);
            var record = end < 0 ? output : output[..end];
            if (!record.IsEmpty)
            {
                records.Add(record.ToArray());
            }
            output = end < 0 ? [] : output[(end + 1)..];
        }
        return records;
    }
}
