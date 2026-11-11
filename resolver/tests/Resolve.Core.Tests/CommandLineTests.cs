using Resolve.Core.Cli;

namespace Resolve.Core.Tests;

public class CommandLineTests
{
    private static readonly string[] FourPaths = ["--base", "b", "--local", "l", "--remote", "r", "--merged", "m"];

    private static string[] Merge(params string[] rest) => ["merge", .. rest];

    [Fact]
    public void Merge_with_the_four_paths_parses()
    {
        var outcome = CommandLine.Parse(Merge(FourPaths));
        var parsed = Assert.IsType<ParseOutcome.Parsed>(outcome).Arguments;
        Assert.Equal(("b", "l", "r", "m"), (parsed.Base, parsed.Local, parsed.Remote, parsed.Merged));
        Assert.False(parsed.NoRepoContext);
        Assert.Null(parsed.LabelBase);
        Assert.Null(parsed.Repo);
    }

    [Fact]
    public void Leading_dash_paths_parse_with_equals_and_after_double_dash()
    {
        var equals = Assert.IsType<ParseOutcome.Parsed>(
            CommandLine.Parse(["merge", "--base=-b", "--local=-l", "--remote=-r", "--merged=-m"])).Arguments;
        Assert.Equal(("-b", "-l", "-r", "-m"), (equals.Base, equals.Local, equals.Remote, equals.Merged));

        var separated = Assert.IsType<ParseOutcome.Parsed>(
            CommandLine.Parse(["merge", "--base", "--", "-b", "--local", "l", "--remote", "r", "--merged", "--", "-m"])).Arguments;
        Assert.Equal(("-b", "l", "r", "-m"), (separated.Base, separated.Local, separated.Remote, separated.Merged));
    }

    [Fact]
    public void A_leading_dash_value_without_equals_or_double_dash_is_invalid()
    {
        Assert.IsType<ParseOutcome.Invalid>(CommandLine.Parse(["merge", "--base", "-b", "--local", "l", "--remote", "r", "--merged", "m"]));
    }

    [Fact]
    public void Equals_form_keeps_everything_after_the_first_equals()
    {
        var parsed = Assert.IsType<ParseOutcome.Parsed>(
            CommandLine.Parse(["merge", "--base=a=b", "--local", "l", "--remote", "r", "--merged=m"])).Arguments;
        Assert.Equal("a=b", parsed.Base);
    }

    [Theory]
    [InlineData("merge --local l --remote r --merged m")]
    [InlineData("merge --base b --local l --remote r --merged")]
    [InlineData("merge --base b --base c --local l --remote r --merged m")]
    [InlineData("merge --bogus x")]
    [InlineData("merge --base b --local l --remote r --merged m stray")]
    [InlineData("merge --base b --local l --remote r --merged m --no-repo-context=yes")]
    [InlineData("merge --base b --local l --remote r --merged m --")]
    [InlineData("bogus")]
    [InlineData("")]
    public void Bad_arguments_are_invalid(string line) =>
        Assert.IsType<ParseOutcome.Invalid>(CommandLine.Parse(line.Split(' ', StringSplitOptions.RemoveEmptyEntries)));

    [Theory]
    [InlineData("repo")]
    [InlineData("diff")]
    public void Later_phase_commands_are_not_available_yet(string command)
    {
        var invalid = Assert.IsType<ParseOutcome.Invalid>(CommandLine.Parse([command]));
        Assert.Contains("is not available yet (Phase 1)", invalid.Message);
        Assert.Contains(command, invalid.Message);
    }

    [Theory]
    [InlineData("--no-wait")]
    [InlineData("--reuse-window")]
    [InlineData("--readonly-sources")]
    public void Later_phase_flags_are_not_available_yet(string flag)
    {
        var invalid = Assert.IsType<ParseOutcome.Invalid>(CommandLine.Parse(Merge([.. FourPaths, flag])));
        Assert.Contains("is not available yet (Phase 1)", invalid.Message);
        Assert.Contains(flag, invalid.Message);
    }

    [Theory]
    [InlineData("--initial", "base")]
    [InlineData("--initial=base+nonconflicting", null)]
    public void Initial_is_not_available_yet(string first, string? value)
    {
        string[] extra = value is null ? [first] : [first, value];
        var invalid = Assert.IsType<ParseOutcome.Invalid>(CommandLine.Parse(Merge([.. FourPaths, .. extra])));
        Assert.Contains("--initial is not available yet (Phase 1)", invalid.Message);
    }

    [Fact]
    public void Wait_is_accepted_and_ignored()
    {
        Assert.IsType<ParseOutcome.Parsed>(CommandLine.Parse(Merge([.. FourPaths, "--wait"])));
    }

    [Fact]
    public void Labels_repo_and_no_repo_context_are_kept()
    {
        var parsed = Assert.IsType<ParseOutcome.Parsed>(CommandLine.Parse(Merge(
            [.. FourPaths, "--label-local", "x", "--label-remote=y", "--label-base", "z", "--repo", "d", "--no-repo-context"]))).Arguments;
        Assert.Equal(("z", "x", "y", "d"), (parsed.LabelBase, parsed.LabelLocal, parsed.LabelRemote, parsed.Repo));
        Assert.True(parsed.NoRepoContext);
    }

    [Fact]
    public void Help_and_version()
    {
        var help = Assert.IsType<ParseOutcome.Help>(CommandLine.Parse(["--help"]));
        Assert.Equal(CommandLine.Usage, help.Text);
        Assert.IsType<ParseOutcome.Help>(CommandLine.Parse(Merge("--help")));
        var version = Assert.IsType<ParseOutcome.Version>(CommandLine.Parse(["--version"]));
        Assert.StartsWith("cthulhu-resolve ", version.Text);
        Assert.True(version.Text.Length > "cthulhu-resolve ".Length);
    }

    [Fact]
    public void Exit_codes_are_the_spec_values() =>
        Assert.Equal(new[] { 0, 1, 2, 3, 4, 5 }, Enum.GetValues<ExitCode>().Select(c => (int)c));
}
