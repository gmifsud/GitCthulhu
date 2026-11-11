using System.Text;
using Resolve.Core.Tests.Support;

namespace Resolve.Core.Tests;

/// <summary>Spec acceptance 8: git's real <c>git mergetool</c> contract, driven through the test-only probe (no display needed).</summary>
public sealed class MergeToolAcceptanceTests
{
    private const string Path = "dir with space/-ünïcode.txt";

    private static async Task<GitFixture> ConfiguredAsync()
    {
        var fixture = await Fixtures.Criterion1ConflictAsync(Path);
        await fixture.RunAsync("config", "mergetool.probe.cmd", Probe.Command());
        await fixture.RunAsync("config", "mergetool.probe.trustExitCode", "true");
        await fixture.RunAsync("config", "mergetool.keepBackup", "false");
        return fixture;
    }

    private static string[] Leftovers(GitFixture fixture) =>
    [
        .. Fixtures.Listing(fixture.Worktree).Where(file =>
            file.Contains("_BASE_", StringComparison.Ordinal) || file.Contains("_LOCAL_", StringComparison.Ordinal)
            || file.Contains("_REMOTE_", StringComparison.Ordinal) || file.EndsWith(".orig", StringComparison.Ordinal)),
    ];

    [Fact]
    public async Task Criterion_8_git_mergetool_resolves_and_git_stages_the_file()
    {
        using var fx = await ConfiguredAsync();
        var left = (await fx.RunAsync("show", ":2:" + Path)).StandardOutput;

        var run = await fx.TryRunWithEnvAsync(new() { ["PROBE_ACTION"] = "accept-left-all" }, "mergetool", "--no-prompt", "--tool", "probe", "--", Path);

        Assert.True(run.ExitCode == 0, Encoding.UTF8.GetString(run.StandardError));
        var stages = await fx.RunAsync("ls-files", "-s", "-z", "--", Path);
        var records = Encoding.UTF8.GetString(stages.StandardOutput).Split('\0', StringSplitOptions.RemoveEmptyEntries);
        var record = Assert.Single(records);
        Assert.Matches(@"^\d{6} [0-9a-f]+ 0\t", record);
        Assert.Empty(Leftovers(fx));
        Assert.Equal(left, File.ReadAllBytes(System.IO.Path.Join(fx.Worktree, Path)));
    }

    [Fact]
    public async Task Cancel_through_git_mergetool_keeps_the_conflict()
    {
        using var fx = await ConfiguredAsync();

        var run = await fx.TryRunWithEnvAsync(new() { ["PROBE_ACTION"] = "cancel" }, "mergetool", "--no-prompt", "--tool", "probe", "--", Path);

        Assert.NotEqual(0, run.ExitCode);
        var unmerged = await fx.RunAsync("ls-files", "-u", "-z", "--", Path);
        var stages = Encoding.UTF8.GetString(unmerged.StandardOutput).Split('\0', StringSplitOptions.RemoveEmptyEntries)
            .Select(record => record[(record.IndexOf(' ', record.IndexOf(' ', StringComparison.Ordinal) + 1))..][1..2]);
        Assert.Equal(["1", "2", "3"], stages.Order(StringComparer.Ordinal));
    }
}
