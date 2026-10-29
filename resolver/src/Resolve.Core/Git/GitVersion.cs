using System.Globalization;
using System.Text;
using System.Text.RegularExpressions;

namespace Resolve.Core.Git;

/// <summary>Parsed <c>git version</c>, e.g. "git version 2.47.1.windows.2".</summary>
public sealed partial record GitVersion(int Major, int Minor, int Patch) : IComparable<GitVersion>
{
    /// <summary>merge-tree --write-tree (2.38) is the floor; see spec §6.2.</summary>
    public static readonly GitVersion Minimum = new(2, 38, 0);

    /// <summary><c>git merge-file --object-id</c>.</summary>
    public static readonly GitVersion MergeFileObjectId = new(2, 43, 0);

    public static GitVersion Parse(string text)
    {
        var match = VersionPattern().Match(text);
        if (!match.Success)
        {
            throw new FormatException($"Not a git version string: '{text.Trim()}'.");
        }
        return new GitVersion(
            int.Parse(match.Groups[1].Value, CultureInfo.InvariantCulture),
            int.Parse(match.Groups[2].Value, CultureInfo.InvariantCulture),
            match.Groups[3].Success ? int.Parse(match.Groups[3].Value, CultureInfo.InvariantCulture) : 0);
    }

    public static async Task<GitVersion> ProbeAsync(IGitRunner runner, CancellationToken cancellationToken)
    {
        ArgumentNullException.ThrowIfNull(runner);
        var result = await runner.RunCheckedAsync(
            new GitInvocation(Environment.CurrentDirectory, ["version"]) { ReadOnly = true }, cancellationToken).ConfigureAwait(false);
        return Parse(Encoding.UTF8.GetString(result.StandardOutput));
    }

    public int CompareTo(GitVersion? other) =>
        other is null ? 1 : (Major, Minor, Patch).CompareTo((other.Major, other.Minor, other.Patch));

    public static bool operator <(GitVersion left, GitVersion right) => left.CompareTo(right) < 0;
    public static bool operator >(GitVersion left, GitVersion right) => left.CompareTo(right) > 0;
    public static bool operator <=(GitVersion left, GitVersion right) => left.CompareTo(right) <= 0;
    public static bool operator >=(GitVersion left, GitVersion right) => left.CompareTo(right) >= 0;

    public override string ToString() => $"{Major}.{Minor}.{Patch}";

    [GeneratedRegex(@"git version (\d+)\.(\d+)(?:\.(\d+))?")]
    private static partial Regex VersionPattern();
}
