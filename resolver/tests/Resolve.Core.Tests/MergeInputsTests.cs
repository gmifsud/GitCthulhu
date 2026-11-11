using Resolve.Core.Cli;
using Resolve.Core.Session;
using Resolve.Core.Tests.Support;

namespace Resolve.Core.Tests;

public sealed class MergeInputsTests
{
    [Fact]
    public async Task A_merge_conflict_loads_three_texts_with_merge_roles()
    {
        using var fx = await GitFixture.CreateAsync();
        await fx.ConflictAsync(f => f.Write("a.txt", "one\nbase\nthree\n"), f => f.Write("a.txt", "one\nleft\nthree\n"), f => f.Write("a.txt", "one\nright\nthree\n"));
        var args = await MergeToolCall.WriteStageFilesAsync(fx, "a.txt");
        var loaded = Assert.IsType<LoadOutcome.Loaded>(await MergeInputLoader.LoadAsync(fx.Git, args, default)).Inputs;
        Assert.Equal(BaseKind.Present, loaded.BaseKind);
        Assert.Equal("base", loaded.Base.Lines[1].Content);
        Assert.Equal("left", loaded.Left.Lines[1].Content);
        Assert.Equal("right", loaded.Right.Lines[1].Content);
        Assert.Equal((OperationKind.Merge, "Current branch", "Incoming branch"), (loaded.Labels.Operation, loaded.Labels.LeftRole, loaded.Labels.RightRole));
        Assert.Equal("a.txt", loaded.RelativePath?.Value);
        Assert.NotNull(loaded.Repo);
        Assert.NotNull(loaded.MergedAtLoad);
    }

    [Fact]
    public async Task Labels_from_the_command_line_become_details()
    {
        using var fx = await GitFixture.CreateAsync();
        await fx.ConflictAsync(f => f.Write("a.txt", "b\n"), f => f.Write("a.txt", "l\n"), f => f.Write("a.txt", "r\n"));
        var args = (await MergeToolCall.WriteStageFilesAsync(fx, "a.txt")) with { LabelLocal = "main", LabelRemote = "right" };
        var loaded = Assert.IsType<LoadOutcome.Loaded>(await MergeInputLoader.LoadAsync(fx.Git, args, default)).Inputs;
        Assert.Equal(("main", "right"), (loaded.Labels.LeftDetail, loaded.Labels.RightDetail));
    }

    [Fact]
    public async Task An_empty_base_with_no_stage_1_is_absent()
    {
        using var fx = await GitFixture.CreateAsync();
        await fx.ConflictAsync(f => f.Write("seed.txt", "seed\n"), f => f.Write("a.txt", "left\n"), f => f.Write("a.txt", "right\n"));
        var args = await MergeToolCall.WriteStageFilesAsync(fx, "a.txt");
        Assert.Equal(0, new FileInfo(args.Base).Length);
        var loaded = Assert.IsType<LoadOutcome.Loaded>(await MergeInputLoader.LoadAsync(fx.Git, args, default)).Inputs;
        Assert.Equal(BaseKind.Absent, loaded.BaseKind);
        Assert.Empty(loaded.Base.Lines);
    }

    [Fact]
    public async Task An_empty_base_with_a_real_empty_stage_1_is_an_empty_stage()
    {
        using var fx = await GitFixture.CreateAsync();
        await fx.ConflictAsync(f => f.Write("a.txt", ""), f => f.Write("a.txt", "left\n"), f => f.Write("a.txt", "right\n"));
        var args = await MergeToolCall.WriteStageFilesAsync(fx, "a.txt");
        var loaded = Assert.IsType<LoadOutcome.Loaded>(await MergeInputLoader.LoadAsync(fx.Git, args, default)).Inputs;
        Assert.Equal(BaseKind.EmptyStage, loaded.BaseKind);
        Assert.Empty(loaded.Base.Lines);
    }

    [Fact]
    public async Task An_empty_base_without_repo_context_is_assumed_absent()
    {
        using var fx = await GitFixture.CreateAsync();
        await fx.ConflictAsync(f => f.Write("seed.txt", "seed\n"), f => f.Write("a.txt", "left\n"), f => f.Write("a.txt", "right\n"));
        var args = (await MergeToolCall.WriteStageFilesAsync(fx, "a.txt")) with { NoRepoContext = true };
        var loaded = Assert.IsType<LoadOutcome.Loaded>(await MergeInputLoader.LoadAsync(fx.Git, args, default)).Inputs;
        Assert.Equal(BaseKind.AbsentAssumed, loaded.BaseKind);
        Assert.Null(loaded.Repo);
        Assert.Null(loaded.RelativePath);
        Assert.Equal((OperationKind.Unknown, "Git stage 2", "Git stage 3"), (loaded.Labels.Operation, loaded.Labels.LeftRole, loaded.Labels.RightRole));
    }

    [Fact]
    public async Task A_merged_file_outside_any_repository_has_no_repo_context()
    {
        using var fx = await GitFixture.CreateAsync();
        var directory = Directory.CreateTempSubdirectory("cthulhu-norepo-").FullName;
        try
        {
            var files = new[] { "base", "local", "remote", "merged" }.Select(name => Path.Join(directory, name)).ToArray();
            foreach (var file in files)
            {
                File.WriteAllText(file, "x\n");
            }
            var args = new MergeToolArguments(files[0], files[1], files[2], files[3], null, null, null, null, false);
            var loaded = Assert.IsType<LoadOutcome.Loaded>(await MergeInputLoader.LoadAsync(fx.Git, args, default)).Inputs;
            Assert.Null(loaded.Repo);
            Assert.Equal(BaseKind.Present, loaded.BaseKind);
            Assert.Equal(OperationKind.Unknown, loaded.Labels.Operation);
        }
        finally
        {
            Directory.Delete(directory, recursive: true);
        }
    }

    [Fact]
    public async Task A_binary_side_is_unreadable()
    {
        using var fx = await GitFixture.CreateAsync();
        await fx.ConflictAsync(f => f.Write("a.txt", "b\n"), f => f.Write("a.txt", "l\n"), f => f.Write("a.txt", "r\n"));
        var args = await MergeToolCall.WriteStageFilesAsync(fx, "a.txt");
        File.WriteAllBytes(args.Local, [0x61, 0x00, 0x62]);
        var outcome = Assert.IsType<LoadOutcome.Unreadable>(await MergeInputLoader.LoadAsync(fx.Git, args, default));
        Assert.Contains("local", outcome.Reason, StringComparison.Ordinal);
        Assert.Contains("binary", outcome.Reason, StringComparison.Ordinal);
    }

    [Fact]
    public async Task An_undecodable_side_is_unreadable()
    {
        using var fx = await GitFixture.CreateAsync();
        await fx.ConflictAsync(f => f.Write("a.txt", "b\n"), f => f.Write("a.txt", "l\n"), f => f.Write("a.txt", "r\n"));
        var args = await MergeToolCall.WriteStageFilesAsync(fx, "a.txt");
        File.WriteAllBytes(args.Remote, [0x61, 0xFF, 0xFE, 0x62]);
        var outcome = Assert.IsType<LoadOutcome.Unreadable>(await MergeInputLoader.LoadAsync(fx.Git, args, default));
        Assert.Contains("remote is not decodable", outcome.Reason, StringComparison.Ordinal);
    }

    [Fact]
    public async Task A_rebase_conflict_gets_rebase_roles_and_the_banner()
    {
        using var fx = await GitFixture.CreateAsync();
        fx.Write("a.txt", "one\nbase\nthree\n");
        await fx.CommitAllAsync("base");
        await fx.RunAsync("checkout", "-q", "-b", "topic");
        fx.Write("a.txt", "one\nmine\nthree\n");
        await fx.CommitAllAsync("mine");
        await fx.RunAsync("checkout", "-q", "main");
        fx.Write("a.txt", "one\nupstream\nthree\n");
        await fx.CommitAllAsync("upstream");
        await fx.RunAsync("checkout", "-q", "topic");
        var rebase = await fx.TryRunAsync("rebase", "main");
        Assert.NotEqual(0, rebase.ExitCode);
        var args = await MergeToolCall.WriteStageFilesAsync(fx, "a.txt");
        var loaded = Assert.IsType<LoadOutcome.Loaded>(await MergeInputLoader.LoadAsync(fx.Git, args, default)).Inputs;
        Assert.Equal(OperationKind.Rebase, loaded.Labels.Operation);
        Assert.Equal("Your commit being replayed", loaded.Labels.RightRole);
        Assert.Contains("Git's \"ours\"", loaded.Labels.Banner, StringComparison.Ordinal);
        Assert.Equal("upstream", loaded.Left.Lines[1].Content);
    }

    [Fact]
    public async Task A_cherry_pick_conflict_gets_cherry_pick_roles()
    {
        using var fx = await GitFixture.CreateAsync();
        await fx.ConflictAsync(f => f.Write("a.txt", "b\n"), f => f.Write("a.txt", "l\n"), f => f.Write("a.txt", "r\n"));
        await fx.RunAsync("merge", "--abort");
        var pick = await fx.TryRunAsync("cherry-pick", "right");
        Assert.NotEqual(0, pick.ExitCode);
        var args = await MergeToolCall.WriteStageFilesAsync(fx, "a.txt");
        var loaded = Assert.IsType<LoadOutcome.Loaded>(await MergeInputLoader.LoadAsync(fx.Git, args, default)).Inputs;
        Assert.Equal((OperationKind.CherryPick, "Picked commit"), (loaded.Labels.Operation, loaded.Labels.RightRole));
    }

    [Fact]
    public async Task Inputs_are_not_held_open()
    {
        using var fx = await GitFixture.CreateAsync();
        await fx.ConflictAsync(f => f.Write("a.txt", "b\n"), f => f.Write("a.txt", "l\n"), f => f.Write("a.txt", "r\n"));
        var args = await MergeToolCall.WriteStageFilesAsync(fx, "a.txt");
        Assert.IsType<LoadOutcome.Loaded>(await MergeInputLoader.LoadAsync(fx.Git, args, default));
        File.Delete(args.Base);
        File.Delete(args.Local);
        File.Delete(args.Remote);
        Assert.False(File.Exists(args.Local));
    }
}
