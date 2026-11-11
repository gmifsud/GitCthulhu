using Resolve.Core.Cli;
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

    [Fact]
    public void The_window_result_is_the_exit_code()
    {
        Assert.Equal(0, Program.ExitCodeOf(ExitCode.Resolved));
        Assert.Equal(1, Program.ExitCodeOf(null));
        Assert.Equal(4, Program.ExitCodeOf(ExitCode.WriteFailed));
    }
}
