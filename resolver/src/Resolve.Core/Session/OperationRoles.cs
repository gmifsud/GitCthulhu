using Resolve.Core.Cli;
using Resolve.Core.Git;

namespace Resolve.Core.Session;

public enum OperationKind
{
    Merge,
    Rebase,
    CherryPick,
    Revert,
    Unknown,
}

/// <summary>What LEFT and RIGHT mean for the git operation in progress, with optional detail from the caller's labels.</summary>
public sealed record SideLabels(OperationKind Operation, string LeftRole, string RightRole, string? LeftDetail, string? RightDetail, string? Banner);

/// <summary>Labels the two sides by the operation that produced the conflict (spec section 3.3, Phase 1 subset).</summary>
public static class OperationRoles
{
    private const string RebaseBanner =
        "Rebase: LEFT is the branch you are rebasing onto (Git's \"ours\"). RIGHT is your own commit (Git's \"theirs\").";

    public static async Task<SideLabels> DetectAsync(IGitRunner runner, RepoLocation? repo, MergeToolArguments arguments, CancellationToken ct)
    {
        ArgumentNullException.ThrowIfNull(runner);
        ArgumentNullException.ThrowIfNull(arguments);

        if (repo is not null)
        {
            if (await ExistsAsync(runner, repo, "rebase-merge", ct).ConfigureAwait(false)
                || await ExistsAsync(runner, repo, "rebase-apply", ct).ConfigureAwait(false))
            {
                return Make(OperationKind.Rebase, "Upstream / onto + already-rebased", "Your commit being replayed", RebaseBanner);
            }
            if (await ExistsAsync(runner, repo, "MERGE_HEAD", ct).ConfigureAwait(false))
            {
                return Make(OperationKind.Merge, "Current branch", "Incoming branch", null);
            }
            if (await ExistsAsync(runner, repo, "CHERRY_PICK_HEAD", ct).ConfigureAwait(false))
            {
                return Make(OperationKind.CherryPick, "Current branch", "Picked commit", null);
            }
            if (await ExistsAsync(runner, repo, "REVERT_HEAD", ct).ConfigureAwait(false))
            {
                return Make(OperationKind.Revert, "Current branch", "Inverse of commit", null);
            }
        }
        return Make(OperationKind.Unknown, "Git stage 2", "Git stage 3", null);

        SideLabels Make(OperationKind operation, string left, string right, string? banner) =>
            new(operation, left, right, arguments.LabelLocal, arguments.LabelRemote, banner);
    }

    private static async Task<bool> ExistsAsync(IGitRunner runner, RepoLocation repo, string name, CancellationToken ct)
    {
        var path = await repo.GitPathAsync(runner, name, ct).ConfigureAwait(false);
        return File.Exists(path) || Directory.Exists(path);
    }
}
