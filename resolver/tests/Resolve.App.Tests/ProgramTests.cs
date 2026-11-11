using System.ComponentModel;
using System.Text;
using Resolve.Core.Cli;
using Resolve.Core.Git;
using Resolve.Core.Session;
using Resolve.Core.Tests.Support;

namespace Resolve.App.Tests;

public sealed class ProgramTests
{
    private static async Task<(Program.Prepared Prepared, string Out, string Err)> PrepareAsync(params string[] args)
    {
        using var stdout = new StringWriter();
        using var stderr = new StringWriter();
        var prepared = await Program.PrepareAsync(args, stdout, stderr);
        return (prepared, stdout.ToString(), stderr.ToString());
    }

    [Fact]
    public async Task Usage_errors_exit_2_with_the_reason_on_stderr()
    {
        var (prepared, _, err) = await PrepareAsync("merge");

        Assert.Equal(2, prepared.ExitCode);
        Assert.Null(prepared.Session);
        Assert.Contains("--base", err, StringComparison.Ordinal);
        Assert.Contains("Usage:", err, StringComparison.Ordinal);
    }

    [Fact]
    public async Task Help_and_version_exit_0_with_text_on_stdout()
    {
        var (help, helpOut, _) = await PrepareAsync("--help");
        var (version, versionOut, _) = await PrepareAsync("--version");

        Assert.Equal(0, help.ExitCode);
        Assert.Equal(CommandLine.Usage, helpOut.TrimEnd());
        Assert.Equal(0, version.ExitCode);
        Assert.StartsWith("cthulhu-resolve ", versionOut, StringComparison.Ordinal);
    }

    [Fact]
    public async Task An_unreadable_input_exits_3_and_opens_no_session()
    {
        var directory = Directory.CreateTempSubdirectory("cthulhu-program-").FullName;
        try
        {
            string Make(string name, byte[] bytes)
            {
                var path = Path.Join(directory, name);
                File.WriteAllBytes(path, bytes);
                return path;
            }
            byte[] text = [0x61, 0x0A];
            var (prepared, _, err) = await PrepareAsync(
                "merge",
                "--base", Make("BASE", text),
                "--local", Make("LOCAL", [0x61, 0x00, 0x62, 0x00, 0x00, 0x01]),
                "--remote", Make("REMOTE", text),
                "--merged", Make("MERGED", text));

            Assert.Equal(3, prepared.ExitCode);
            Assert.Null(prepared.Session);
            Assert.NotEmpty(err);
        }
        finally
        {
            Directory.Delete(directory, recursive: true);
        }
    }

    [Fact]
    public async Task A_successful_open_returns_a_session_and_writes_nothing()
    {
        using var fixture = await GitFixture.CreateAsync();
        await fixture.ConflictAsync(
            f => f.Write("a.txt", "one\n"),
            f => f.Write("a.txt", "left\n"),
            f => f.Write("a.txt", "right\n"));
        var call = await MergeToolCall.WriteStageFilesAsync(fixture, "a.txt");
        var before = File.ReadAllBytes(call.Merged);

        var (prepared, _, err) = await PrepareAsync(
            "merge", "--base", call.Base, "--local", call.Local, "--remote", call.Remote, "--merged", call.Merged);

        Assert.Null(prepared.ExitCode);
        Assert.NotNull(prepared.Session);
        Assert.Empty(err);
        prepared.Session.Cancel();
        Assert.Equal(before, File.ReadAllBytes(call.Merged));
    }

    private sealed class FakeGit(Func<GitResult> respond) : IGitRunner
    {
        public Task<GitResult> RunAsync(GitInvocation invocation, CancellationToken cancellationToken) => Task.FromResult(respond());
    }

    private static async Task<(Program.Prepared Prepared, string Err)> PrepareWithGitAsync(Func<GitResult> respond)
    {
        using var stdout = new StringWriter();
        using var stderr = new StringWriter();
        var prepared = await Program.PrepareAsync(
            ["merge", "--base", "b", "--local", "l", "--remote", "r", "--merged", "m"], stdout, stderr, new FakeGit(respond));
        return (prepared, stderr.ToString());
    }

    [Fact]
    public async Task Git_that_cannot_start_exits_2_with_the_reason()
    {
        var (prepared, err) = await PrepareWithGitAsync(() => throw new Win32Exception("The system cannot find the file specified"));

        Assert.Equal(2, prepared.ExitCode);
        Assert.Contains("Could not run git: The system cannot find the file specified", err, StringComparison.Ordinal);
    }

    [Fact]
    public async Task Git_failing_its_version_probe_exits_2()
    {
        var (prepared, err) = await PrepareWithGitAsync(() => new GitResult(128, [], Encoding.UTF8.GetBytes("boom")));

        Assert.Equal(2, prepared.ExitCode);
        Assert.Contains("Could not run git:", err, StringComparison.Ordinal);
    }

    [Fact]
    public async Task An_unparseable_git_version_exits_2()
    {
        var (prepared, err) = await PrepareWithGitAsync(() => new GitResult(0, Encoding.UTF8.GetBytes("git version banana"), []));

        Assert.Equal(2, prepared.ExitCode);
        Assert.Contains("Could not run git:", err, StringComparison.Ordinal);
    }

    [Fact]
    public async Task A_git_below_the_minimum_exits_2()
    {
        var (prepared, err) = await PrepareWithGitAsync(() => new GitResult(0, Encoding.UTF8.GetBytes("git version 2.30.0"), []));

        Assert.Equal(2, prepared.ExitCode);
        Assert.Contains("git 2.38 or later is required", err, StringComparison.Ordinal);
    }

    private static async Task<(MergeToolSession Session, MergeToolArguments Call, GitFixture Fixture)> OpenRealAsync()
    {
        var fixture = await GitFixture.CreateAsync();
        await fixture.ConflictAsync(
            f => f.Write("a.txt", "one\n"),
            f => f.Write("a.txt", "left\n"),
            f => f.Write("a.txt", "right\n"));
        var call = await MergeToolCall.WriteStageFilesAsync(fixture, "a.txt");
        var outcome = await MergeToolSession.OpenAsync(fixture.Git, call, Fixtures.Options(), CancellationToken.None);
        return (((OpenOutcome.Opened)outcome).Session, call, fixture);
    }

    [Fact]
    public async Task A_window_that_throws_exits_1_cancels_the_session_and_writes_nothing()
    {
        var (session, call, fixture) = await OpenRealAsync();
        using var owned = fixture;
        var before = File.ReadAllBytes(call.Merged);
        using var stderr = new StringWriter();

        var code = Program.RunWindow(session, _ => throw new InvalidOperationException("no display"), stderr);

        Assert.Equal(1, code);
        Assert.True(session.IsCancelled);
        Assert.Contains("The window failed: no display", stderr.ToString(), StringComparison.Ordinal);
        Assert.Equal(before, File.ReadAllBytes(call.Merged));
    }

    [Fact]
    public async Task A_window_that_returns_a_result_gives_its_exit_code()
    {
        var (session, _, fixture) = await OpenRealAsync();
        using var owned = fixture;
        using var stderr = new StringWriter();

        Assert.Equal(0, Program.RunWindow(session, _ => ExitCode.Resolved, stderr));
        Assert.Empty(stderr.ToString());
    }

    [Fact]
    public void The_window_result_is_the_exit_code()
    {
        Assert.Equal(0, Program.ExitCodeOf(ExitCode.Resolved));
        Assert.Equal(1, Program.ExitCodeOf(null));
        Assert.Equal(4, Program.ExitCodeOf(ExitCode.WriteFailed));
    }
}
