using Resolve.Core.Git;
using Resolve.Core.Session;

namespace Resolve.Core.Tests.Support;

/// <summary>Small shared builders for tests. No xUnit types: a UI test project may link this file.</summary>
internal static class Fixtures
{
    /// <summary>A git runner with no user or system config, over a fresh temp home.</summary>
    public static IGitRunner Git()
    {
        var home = Directory.CreateTempSubdirectory("cthulhu-home-").FullName;
        File.WriteAllText(Path.Join(home, ".gitconfig"), "");
        return new IsolatedGitRunner(home);
    }

    /// <summary>Criterion 1: a.txt of 50 lines; LEFT changes lines 3 and 20, RIGHT changes lines 20 and 40; the merge is left conflicted.</summary>
    public static Task<GitFixture> Criterion1ConflictAsync() => Criterion1ConflictAsync("a.txt");

    /// <summary>Criterion 1 at a chosen worktree-relative path (spaces, a leading dash and non-ASCII names included).</summary>
    public static async Task<GitFixture> Criterion1ConflictAsync(string path)
    {
        var fixture = await GitFixture.CreateAsync();
        var baseLines = Enumerable.Range(1, 50).Select(number => $"line {number}").ToArray();
        string Text(Action<string[]> change)
        {
            var lines = (string[])baseLines.Clone();
            change(lines);
            return string.Concat(lines.Select(line => line + "\n"));
        }
        await fixture.ConflictAsync(
            f => f.Write(path, Text(_ => { })),
            f => f.Write(path, Text(lines => { lines[2] = "left 3"; lines[19] = "left 20"; })),
            f => f.Write(path, Text(lines => { lines[19] = "right 20"; lines[39] = "right 40"; })));
        return fixture;
    }

    /// <summary>Session options over a fresh temp backup root.</summary>
    public static SessionOptions Options() =>
        new(Directory.CreateTempSubdirectory("cthulhu-backups-").FullName, TimeProvider.System);

    /// <summary>Sorted worktree-relative file names, excluding <c>.git</c>.</summary>
    public static string[] Listing(string directory) =>
    [
        .. Directory.EnumerateFiles(directory, "*", SearchOption.AllDirectories)
            .Select(path => Path.GetRelativePath(directory, path).Replace('\\', '/'))
            .Where(path => !path.StartsWith(".git/", StringComparison.Ordinal))
            .Order(StringComparer.Ordinal),
    ];
}
