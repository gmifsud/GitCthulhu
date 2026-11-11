using Resolve.Core.Git;

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
}
