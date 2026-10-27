using System.Text;
using Resolve.Core.Git;

namespace Resolve.Core.Tests.Support;

/// <summary>
/// Runs git with no user or system config: HOME and the global config point into the fixture, so the
/// machine's aliases, hooks, signing and autocrlf never leak into a test. Deterministic author and dates.
/// </summary>
internal sealed class IsolatedGitRunner : IGitRunner
{
    private readonly GitRunner _inner = new();
    private readonly Dictionary<string, string?> _environment;

    public IsolatedGitRunner(string home)
    {
        _environment = new Dictionary<string, string?>
        {
            ["HOME"] = home,
            ["XDG_CONFIG_HOME"] = Path.Join(home, ".config"),
            ["GIT_CONFIG_GLOBAL"] = Path.Join(home, ".gitconfig"),
            ["GIT_CONFIG_NOSYSTEM"] = "1",
            ["GIT_AUTHOR_NAME"] = "Fixture",
            ["GIT_AUTHOR_EMAIL"] = "fixture@example.invalid",
            ["GIT_COMMITTER_NAME"] = "Fixture",
            ["GIT_COMMITTER_EMAIL"] = "fixture@example.invalid",
            ["GIT_AUTHOR_DATE"] = "2026-01-01T00:00:00Z",
            ["GIT_COMMITTER_DATE"] = "2026-01-01T00:00:00Z",
        };
    }

    public Task<GitResult> RunAsync(GitInvocation invocation, CancellationToken cancellationToken)
    {
        var environment = new Dictionary<string, string?>(_environment);
        foreach (var (key, value) in invocation.Environment)
        {
            environment[key] = value;
        }
        return _inner.RunAsync(invocation with { Environment = environment }, cancellationToken);
    }
}

/// <summary>A throwaway repository built with real git commands.</summary>
internal sealed class GitFixture : IDisposable
{
    private GitFixture(string root)
    {
        Root = root;
        Worktree = Path.Join(root, "repo");
        Directory.CreateDirectory(Worktree);
        File.WriteAllText(Path.Join(root, ".gitconfig"), "");
        Git = new IsolatedGitRunner(root);
    }

    public string Root { get; }

    public string Worktree { get; }

    public IGitRunner Git { get; }

    public static async Task<GitFixture> CreateAsync()
    {
        var fixture = new GitFixture(Directory.CreateTempSubdirectory("cthulhu-fixture-").FullName);
        await fixture.RunAsync("init", "-q", "-b", "main");
        await fixture.RunAsync("config", "commit.gpgsign", "false");
        await fixture.RunAsync("config", "core.autocrlf", "false");
        await fixture.RunAsync("config", "merge.conflictStyle", "merge");
        return fixture;
    }

    public async Task<GitResult> RunAsync(params string[] arguments) =>
        await Git.RunCheckedAsync(new GitInvocation(Worktree, arguments), CancellationToken.None);

    public Task<GitResult> TryRunAsync(params string[] arguments) =>
        Git.RunAsync(new GitInvocation(Worktree, arguments), CancellationToken.None);

    public void Write(string path, string text) => WriteBytes(path, Encoding.UTF8.GetBytes(text));

    public void WriteBytes(string path, byte[] bytes)
    {
        var full = Path.Join(Worktree, path);
        Directory.CreateDirectory(Path.GetDirectoryName(full)!);
        File.WriteAllBytes(full, bytes);
    }

    public void Delete(string path) => File.Delete(Path.Join(Worktree, path));

    public async Task CommitAllAsync(string message)
    {
        await RunAsync("add", "-A");
        await RunAsync("commit", "-q", "--allow-empty", "-m", message);
    }

    /// <summary>Base on main, one change per branch, then merge right into left. Returns git's merge exit code.</summary>
    public async Task<int> ConflictAsync(Action<GitFixture> baseState, Action<GitFixture> leftChange, Action<GitFixture> rightChange)
    {
        baseState(this);
        await CommitAllAsync("base");
        await RunAsync("checkout", "-q", "-b", "right");
        rightChange(this);
        await CommitAllAsync("right change");
        await RunAsync("checkout", "-q", "main");
        leftChange(this);
        await CommitAllAsync("left change");
        var merge = await TryRunAsync("merge", "-q", "--no-edit", "right");
        return merge.ExitCode;
    }

    public Task<RepoLocation> LocationAsync() => RepoLocation.DiscoverAsync(Git, Worktree, CancellationToken.None);

    public void Dispose()
    {
        try
        {
            Directory.Delete(Root, recursive: true);
        }
        catch (IOException)
        {
            // Best effort on Windows if an antivirus scanner holds a handle.
        }
    }
}
