namespace Resolve.Core.Git;

/// <summary>
/// The three inputs of one unmerged path, in working-tree form: smudge filters, <c>eol</c> and
/// <c>working-tree-encoding</c> applied, exactly as git would check them out. A null side is an absent stage.
/// Gitlinks carry no content (the object is a commit in another repository).
/// </summary>
public sealed record ConflictInputs(UnmergedPath Path, byte[]? Base, byte[]? Left, byte[]? Right)
{
    public byte[]? this[Stage stage] => stage switch
    {
        Stage.Base => Base,
        Stage.Left => Left,
        Stage.Right => Right,
        _ => throw new ArgumentOutOfRangeException(nameof(stage)),
    };
}

public static class ConflictAcquisition
{
    /// <summary>
    /// Reads each present stage with <c>git cat-file --filters :N:&lt;path&gt;</c>. Unlike
    /// <c>checkout-index --temp</c> this writes nothing into the worktree, and unlike <c>git show :N:path</c>
    /// it applies the same conversions a checkout would.
    /// </summary>
    public static async Task<ConflictInputs> AcquireAsync(
        IGitRunner runner, RepoLocation repo, UnmergedPath path, CancellationToken cancellationToken)
    {
        ArgumentNullException.ThrowIfNull(path);
        if (path.PathKind == PathKind.Gitlink)
        {
            return new ConflictInputs(path, null, null, null);
        }
        var stages = await Task.WhenAll(
            ReadStageAsync(runner, repo, path, Stage.Base, cancellationToken),
            ReadStageAsync(runner, repo, path, Stage.Left, cancellationToken),
            ReadStageAsync(runner, repo, path, Stage.Right, cancellationToken)).ConfigureAwait(false);
        return new ConflictInputs(path, stages[0], stages[1], stages[2]);
    }

    private static async Task<byte[]?> ReadStageAsync(
        IGitRunner runner, RepoLocation repo, UnmergedPath path, Stage stage, CancellationToken cancellationToken)
    {
        ArgumentNullException.ThrowIfNull(runner);
        ArgumentNullException.ThrowIfNull(repo);
        if (path[stage] is null)
        {
            return null;
        }
        // ":N:path" is relative to the top of the tree; run from the worktree root.
        var arguments = new[] { "cat-file", "--filters", $":{(int)stage}:{path.Path.Value}" };
        var result = await runner.RunCheckedAsync(
            new GitInvocation(repo.Worktree, arguments) { ReadOnly = true }, cancellationToken).ConfigureAwait(false);
        return result.StandardOutput;
    }
}
