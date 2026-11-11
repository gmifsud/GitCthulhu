using System.Collections.Immutable;
using Resolve.Core.Cli;
using Resolve.Core.Git;
using Resolve.Core.Persistence;
using Resolve.Core.Text;

namespace Resolve.Core.Session;

/// <summary>Whether the common ancestor is a real file, a real empty file, or missing.</summary>
public enum BaseKind
{
    Present,

    /// <summary>Stage 1 exists and is an empty file: the empty file is the real base.</summary>
    EmptyStage,

    /// <summary>Git has no stage 1 for the path (add/add): the base is "nothing".</summary>
    Absent,

    /// <summary>Empty <c>$BASE</c> and no repository to ask: assumed to mean no base.</summary>
    AbsentAssumed,
}

/// <summary>The three decoded inputs plus the context needed to plan and save. <see cref="Base"/> has no lines unless <see cref="BaseKind"/> is <see cref="BaseKind.Present"/>.</summary>
public sealed record MergeInputs(
    string MergedPath,
    string BaseFile,
    string LeftFile,
    string RightFile,
    RepoLocation? Repo,
    RepoPath? RelativePath,
    BaseKind BaseKind,
    DecodedText Base,
    DecodedText Left,
    DecodedText Right,
    SideLabels Labels,
    FileFingerprint? MergedAtLoad);

public abstract record LoadOutcome
{
    private LoadOutcome() { }

    public sealed record Loaded(MergeInputs Inputs) : LoadOutcome;

    public sealed record Unreadable(string Reason) : LoadOutcome;
}

public static class MergeInputLoader
{
    public static async Task<LoadOutcome> LoadAsync(IGitRunner runner, MergeToolArguments arguments, CancellationToken ct)
    {
        ArgumentNullException.ThrowIfNull(runner);
        ArgumentNullException.ThrowIfNull(arguments);

        byte[] baseBytes, leftBytes, rightBytes;
        try
        {
            baseBytes = File.ReadAllBytes(arguments.Base);
            leftBytes = File.ReadAllBytes(arguments.Local);
            rightBytes = File.ReadAllBytes(arguments.Remote);
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException)
        {
            return new LoadOutcome.Unreadable($"cannot read input: {error.Message}");
        }

        var left = Decode("local", leftBytes, out var leftFailure);
        if (left is null)
        {
            return new LoadOutcome.Unreadable(leftFailure!);
        }
        var right = Decode("remote", rightBytes, out var rightFailure);
        if (right is null)
        {
            return new LoadOutcome.Unreadable(rightFailure!);
        }
        DecodedText? baseText = null;
        if (baseBytes.Length > 0)
        {
            baseText = Decode("base", baseBytes, out var baseFailure);
            if (baseText is null)
            {
                return new LoadOutcome.Unreadable(baseFailure!);
            }
        }

        var mergedPath = Path.GetFullPath(arguments.Merged);
        var repo = await FindRepoAsync(runner, arguments, mergedPath, ct).ConfigureAwait(false);
        var relative = repo is null ? null : RelativeTo(repo, mergedPath);

        BaseKind kind;
        if (baseText is not null)
        {
            kind = BaseKind.Present;
        }
        else if (repo is null || relative is null)
        {
            kind = BaseKind.AbsentAssumed;
        }
        else
        {
            var unmerged = await UnmergedIndex.ListAsync(runner, repo, ct).ConfigureAwait(false);
            var entry = unmerged.FirstOrDefault(path => path.Path == relative.Value);
            kind = entry is null ? BaseKind.AbsentAssumed
                : entry.Base is null ? BaseKind.Absent
                : BaseKind.EmptyStage;
        }

        var labels = await OperationRoles.DetectAsync(runner, repo, arguments, ct).ConfigureAwait(false);
        return new LoadOutcome.Loaded(new MergeInputs(
            mergedPath,
            arguments.Base,
            arguments.Local,
            arguments.Remote,
            repo,
            relative,
            kind,
            baseText ?? new DecodedText(left.Encoding, ImmutableArray<Line>.Empty),
            left,
            right,
            labels,
            FileFingerprint.Capture(mergedPath)));
    }

    private static DecodedText? Decode(string which, byte[] bytes, out string? failure)
    {
        switch (TextCodec.Decode(bytes))
        {
            case DecodeOutcome.Text text:
                failure = null;
                return text.Value;
            case DecodeOutcome.Binary:
                failure = $"{which} is binary";
                return null;
            case DecodeOutcome.Undecodable undecodable:
                failure = $"{which} is not decodable: {undecodable.Reason}";
                return null;
            default:
                throw new InvalidOperationException("Unknown decode outcome.");
        }
    }

    /// <summary>No repository, or one that cannot be found, is context we lack, not an error.</summary>
    private static async Task<RepoLocation?> FindRepoAsync(IGitRunner runner, MergeToolArguments arguments, string mergedPath, CancellationToken ct)
    {
        if (arguments.NoRepoContext)
        {
            return null;
        }
        try
        {
            var directory = arguments.Repo ?? Path.GetDirectoryName(mergedPath) ?? mergedPath;
            return await RepoLocation.DiscoverAsync(runner, directory, ct).ConfigureAwait(false);
        }
        catch (GitCommandException)
        {
            return null;
        }
    }

    private static RepoPath? RelativeTo(RepoLocation repo, string mergedPath)
    {
        var relative = Path.GetRelativePath(repo.Worktree, mergedPath).Replace('\\', '/');
        return relative == ".." || relative.StartsWith("../", StringComparison.Ordinal) || Path.IsPathRooted(relative)
            ? null
            : new RepoPath(relative);
    }
}
