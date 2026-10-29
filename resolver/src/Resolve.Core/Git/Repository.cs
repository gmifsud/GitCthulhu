using System.Text;

namespace Resolve.Core.Git;

/// <summary>
/// Repo-relative path: '/'-separated, exactly as the index spells it. No Unicode normalisation, so an NFD
/// name from macOS round-trips. Comparison is ordinal; case-folding collisions are a separate check.
/// </summary>
public readonly record struct RepoPath
{
    public RepoPath(string value)
    {
        ArgumentException.ThrowIfNullOrEmpty(value);
        if (value.Contains('\0', StringComparison.Ordinal) || value.StartsWith('/') || value.Contains('\\', StringComparison.Ordinal))
        {
            throw new ArgumentException($"Not a repo-relative path: '{value}'.", nameof(value));
        }
        Value = value;
    }

    public string Value { get; }

    /// <summary>Absolute filesystem path under <paramref name="worktree"/>.</summary>
    public string ToFullPath(string worktree) =>
        Path.GetFullPath(Path.Join(worktree, Value.Replace('/', Path.DirectorySeparatorChar)));

    public override string ToString() => Value;
}

/// <summary>Where a worktree's files and git data live. Linked worktrees have a git dir separate from the common dir.</summary>
public sealed record RepoLocation(string Worktree, string GitDir, string CommonDir)
{
    public bool IsLinkedWorktree => !string.Equals(
        Path.TrimEndingDirectorySeparator(GitDir), Path.TrimEndingDirectorySeparator(CommonDir), StringComparison.Ordinal);

    /// <summary>
    /// Finds the repository containing <paramref name="directory"/>. Paths come back absolute
    /// (<c>--path-format=absolute</c>, git 2.31+), so callers never join relative git output by hand.
    /// </summary>
    public static async Task<RepoLocation> DiscoverAsync(IGitRunner runner, string directory, CancellationToken cancellationToken)
    {
        ArgumentNullException.ThrowIfNull(runner);
        var result = await runner.RunCheckedAsync(
            new GitInvocation(directory, ["rev-parse", "--path-format=absolute", "--show-toplevel", "--git-dir", "--git-common-dir"]) { ReadOnly = true },
            cancellationToken).ConfigureAwait(false);
        var lines = Encoding.UTF8.GetString(result.StandardOutput).Split('\n', StringSplitOptions.RemoveEmptyEntries);
        if (lines.Length != 3)
        {
            throw new GitCommandException($"Unexpected rev-parse output: {lines.Length} lines.");
        }
        return new RepoLocation(lines[0], lines[1], lines[2]);
    }

    /// <summary>Resolves a git-dir-relative name (MERGE_HEAD, rebase-merge, index.lock) the way git does for this worktree.</summary>
    public async Task<string> GitPathAsync(IGitRunner runner, string name, CancellationToken cancellationToken)
    {
        ArgumentNullException.ThrowIfNull(runner);
        var result = await runner.RunCheckedAsync(
            new GitInvocation(Worktree, ["rev-parse", "--path-format=absolute", "--git-path", name]) { ReadOnly = true },
            cancellationToken).ConfigureAwait(false);
        return Encoding.UTF8.GetString(result.StandardOutput).TrimEnd('\n');
    }
}
