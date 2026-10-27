using System.Text;
using Resolve.Core.Git;
using Resolve.Core.Tests.Support;
using Resolve.Core.Text;

namespace Resolve.Core.Tests;

public class GitLayerTests
{
    [Fact]
    public async Task Arguments_reach_git_verbatim_without_a_shell()
    {
        using var fixture = await GitFixture.CreateAsync();
        const string hostile = "a \"b\" 'c' $HOME `id` ; rm -rf / && echo | * ?";
        await fixture.RunAsync("config", "test.value", hostile);
        var result = await fixture.RunAsync("config", "--get", "test.value");
        Assert.Equal(hostile, Encoding.UTF8.GetString(result.StandardOutput).TrimEnd('\n'));
    }

    [Theory]
    [InlineData("git version 2.43.0", 2, 43, 0)]
    [InlineData("git version 2.47.1.windows.2\n", 2, 47, 1)]
    [InlineData("git version 2.39", 2, 39, 0)]
    public void Version_strings_parse(string text, int major, int minor, int patch) =>
        Assert.Equal(new GitVersion(major, minor, patch), GitVersion.Parse(text));

    [Fact]
    public async Task Installed_git_meets_the_minimum()
    {
        var version = await GitVersion.ProbeAsync(new GitRunner(), CancellationToken.None);
        Assert.True(version >= GitVersion.Minimum, $"git {version} is older than {GitVersion.Minimum}");
    }

    [Fact]
    public void Ls_files_records_group_by_path_in_ordinal_order()
    {
        var output = Encoding.UTF8.GetBytes(
            "100644 aaaa 1\tz.txt\0100644 bbbb 2\tz.txt\0100644 cccc 3\tz.txt\0" +
            "100755 dddd 2\t-lead ing ✓.cs\0100644 eeee 3\t-lead ing ✓.cs\0");
        var paths = UnmergedIndex.Parse(output);
        Assert.Equal(["-lead ing ✓.cs", "z.txt"], paths.Select(path => path.Path.Value));
        Assert.Equal(UnmergedKind.BothAdded, paths[0].Kind);
        Assert.Equal(GitMode.Executable, paths[0].Left!.Mode);
        Assert.Equal(UnmergedKind.BothModified, paths[1].Kind);
    }

    [Fact]
    public async Task Both_modified_inputs_come_from_the_index_stages()
    {
        using var fixture = await GitFixture.CreateAsync();
        var exit = await fixture.ConflictAsync(
            f => f.Write("f.txt", "one\ntwo\nthree\n"),
            f => f.Write("f.txt", "one\nLEFT\nthree\n"),
            f => f.Write("f.txt", "one\nRIGHT\nthree\n"));
        Assert.NotEqual(0, exit);
        var repo = await fixture.LocationAsync();
        var path = Assert.Single(await UnmergedIndex.ListAsync(fixture.Git, repo, CancellationToken.None));
        Assert.Equal(UnmergedKind.BothModified, path.Kind);

        var inputs = await ConflictAcquisition.AcquireAsync(fixture.Git, repo, path, CancellationToken.None);
        Assert.Equal("one\ntwo\nthree\n", Encoding.UTF8.GetString(inputs.Base!));
        Assert.Equal("one\nLEFT\nthree\n", Encoding.UTF8.GetString(inputs.Left!));
        Assert.Equal("one\nRIGHT\nthree\n", Encoding.UTF8.GetString(inputs.Right!));
        // The worktree holds markers; inputs never came from it.
        Assert.Contains("<<<<<<<", File.ReadAllText(Path.Join(fixture.Worktree, "f.txt")), StringComparison.Ordinal);
    }

    [Fact]
    public async Task Add_add_has_no_base_stage()
    {
        using var fixture = await GitFixture.CreateAsync();
        await fixture.ConflictAsync(
            f => f.Write("keep.txt", "x\n"),
            f => f.Write("new.txt", "left\n"),
            f => f.Write("new.txt", "right\n"));
        var repo = await fixture.LocationAsync();
        var path = Assert.Single(await UnmergedIndex.ListAsync(fixture.Git, repo, CancellationToken.None));
        Assert.Equal(UnmergedKind.BothAdded, path.Kind);
        var inputs = await ConflictAcquisition.AcquireAsync(fixture.Git, repo, path, CancellationToken.None);
        Assert.Null(inputs.Base);
    }

    [Theory]
    [InlineData(true, UnmergedKind.DeletedByThem)]
    [InlineData(false, UnmergedKind.DeletedByUs)]
    public async Task Modify_delete_is_classified_by_the_missing_stage(bool leftModifies, UnmergedKind expected)
    {
        using var fixture = await GitFixture.CreateAsync();
        await fixture.ConflictAsync(
            f => f.Write("f.txt", "base\n"),
            f => { if (leftModifies) { f.Write("f.txt", "changed\n"); } else { f.Delete("f.txt"); } },
            f => { if (leftModifies) { f.Delete("f.txt"); } else { f.Write("f.txt", "changed\n"); } });
        var repo = await fixture.LocationAsync();
        var path = Assert.Single(await UnmergedIndex.ListAsync(fixture.Git, repo, CancellationToken.None));
        Assert.Equal(expected, path.Kind);
    }

    [Fact]
    public async Task Hostile_file_names_are_literal_paths()
    {
        using var fixture = await GitFixture.CreateAsync();
        // "*" and ":" are legal on POSIX filesystems but not on NTFS; both names are pathspec-magic hazards.
        var name = OperatingSystem.IsWindows() ? "-dash [x] (top) ünï.txt" : "-dash *star [x] :(top) ünï.txt";
        await fixture.ConflictAsync(
            f => { f.Write(name, "a\n"); f.Write("decoy.txt", "a\n"); },
            f => f.Write(name, "left\n"),
            f => f.Write(name, "right\n"));
        var repo = await fixture.LocationAsync();
        var path = Assert.Single(await UnmergedIndex.ListAsync(fixture.Git, repo, CancellationToken.None));
        Assert.Equal(name, path.Path.Value);
        var inputs = await ConflictAcquisition.AcquireAsync(fixture.Git, repo, path, CancellationToken.None);
        Assert.Equal("right\n", Encoding.UTF8.GetString(inputs.Right!));
    }

    [Fact]
    public async Task Inputs_are_in_working_tree_form_after_eol_conversion()
    {
        using var fixture = await GitFixture.CreateAsync();
        fixture.Write(".gitattributes", "*.txt text eol=crlf\n");
        await fixture.ConflictAsync(
            f => f.Write("f.txt", "a\r\nb\r\n"),
            f => f.Write("f.txt", "a\r\nleft\r\n"),
            f => f.Write("f.txt", "a\r\nright\r\n"));
        var repo = await fixture.LocationAsync();
        var path = Assert.Single(await UnmergedIndex.ListAsync(fixture.Git, repo, CancellationToken.None));
        var inputs = await ConflictAcquisition.AcquireAsync(fixture.Git, repo, path, CancellationToken.None);

        // Blobs are stored normalised (LF); the working-tree form re-applies CRLF, as a checkout would.
        var raw = await fixture.RunAsync("cat-file", "-p", path.Left!.ObjectId);
        Assert.Equal("a\nleft\n", Encoding.UTF8.GetString(raw.StandardOutput));
        Assert.Equal("a\r\nleft\r\n", Encoding.UTF8.GetString(inputs.Left!));
        var decoded = Assert.IsType<DecodeOutcome.Text>(TextCodec.Decode(inputs.Left!)).Value;
        Assert.Equal((0, 2), decoded.EndingCounts);
    }

    [Fact]
    public async Task Linked_worktrees_are_recognised()
    {
        using var fixture = await GitFixture.CreateAsync();
        fixture.Write("f.txt", "x\n");
        await fixture.CommitAllAsync("init");
        var linked = Path.Join(fixture.Root, "linked");
        await fixture.RunAsync("worktree", "add", "-q", "-b", "other", linked);
        var location = await RepoLocation.DiscoverAsync(fixture.Git, linked, CancellationToken.None);
        Assert.True(location.IsLinkedWorktree);
        var mergeHead = await location.GitPathAsync(fixture.Git, "MERGE_HEAD", CancellationToken.None);
        Assert.Contains("worktrees/linked", mergeHead.Replace('\\', '/'), StringComparison.Ordinal);
    }
}
