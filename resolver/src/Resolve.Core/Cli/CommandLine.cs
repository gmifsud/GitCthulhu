using System.Reflection;

namespace Resolve.Core.Cli;

/// <summary>Process exit codes of the merge tool (spec §8.2). Git, with <c>trustExitCode=true</c>, acts on these.</summary>
public enum ExitCode
{
    Resolved = 0,
    Unresolved = 1,
    Usage = 2,
    InputUnreadable = 3,
    WriteFailed = 4,
    ExternalChangeDeclined = 5,
}

/// <summary>The paths and options of <c>cthulhu-resolve merge</c>. Paths are verbatim; nothing is globbed or normalised.</summary>
public sealed record MergeToolArguments(
    string Base,
    string Local,
    string Remote,
    string Merged,
    string? LabelBase,
    string? LabelLocal,
    string? LabelRemote,
    string? Repo,
    bool NoRepoContext);

/// <summary>What the command line asked for. Parsing never exits, prints or touches the file system.</summary>
public abstract record ParseOutcome
{
    private ParseOutcome() { }

    public sealed record Parsed(MergeToolArguments Arguments) : ParseOutcome;

    public sealed record Help(string Text) : ParseOutcome;

    public sealed record Version(string Text) : ParseOutcome;

    public sealed record Invalid(string Message) : ParseOutcome;
}

/// <summary>
/// Hand-written parser for the one Phase 1 sub-command. Forms: <c>--opt value</c>, <c>--opt=value</c>, and
/// <c>--opt -- value</c>. A value starting with <c>-</c> is accepted only in the last two forms.
/// </summary>
public static class CommandLine
{
    public const string Usage =
        "Usage:\n" +
        "  cthulhu-resolve merge --base <file> --local <file> --remote <file> --merged <file>\n" +
        "                        [--label-base <s>] [--label-local <s>] [--label-remote <s>]\n" +
        "                        [--repo <dir>] [--no-repo-context] [--wait]\n" +
        "  cthulhu-resolve --version | --help\n" +
        "\n" +
        "A path beginning with '-' must be given as --merged=<file> or after '--' (--merged -- <file>).\n" +
        "Exit codes: 0 resolved, 1 unresolved, 2 usage, 3 input unreadable, 4 write failed, 5 external change declined.";

    private const string NotYet = "is not available yet (Phase 1)";

    private static readonly string[] NotYetCommands = ["repo", "diff"];
    private static readonly string[] NotYetFlags = ["--no-wait", "--reuse-window", "--readonly-sources"];
    private static readonly string[] ValuedOptions =
        ["--base", "--local", "--remote", "--merged", "--label-base", "--label-local", "--label-remote", "--repo"];

    public static ParseOutcome Parse(IReadOnlyList<string> args)
    {
        ArgumentNullException.ThrowIfNull(args);
        if (args.Count == 0)
        {
            return new ParseOutcome.Invalid("A command is required.\n" + Usage);
        }

        var command = args[0];
        return command switch
        {
            "--help" => new ParseOutcome.Help(Usage),
            "--version" => VersionOutcome(),
            "merge" => ParseMerge(args),
            _ when NotYetCommands.Contains(command, StringComparer.Ordinal) => new ParseOutcome.Invalid($"'{command}' {NotYet}."),
            _ => new ParseOutcome.Invalid($"Unknown command '{command}'.\n" + Usage),
        };
    }

    private static ParseOutcome.Version VersionOutcome()
    {
        var assembly = typeof(CommandLine).Assembly;
        var version = assembly.GetCustomAttribute<AssemblyInformationalVersionAttribute>()?.InformationalVersion
            ?? assembly.GetName().Version?.ToString()
            ?? "unknown";
        return new ParseOutcome.Version($"cthulhu-resolve {version}");
    }

    private static ParseOutcome ParseMerge(IReadOnlyList<string> args)
    {
        var values = new Dictionary<string, string>(StringComparer.Ordinal);
        var noRepoContext = false;
        for (var index = 1; index < args.Count; index++)
        {
            var token = args[index];
            if (!token.StartsWith("--", StringComparison.Ordinal) || token.Length == 2)
            {
                return new ParseOutcome.Invalid($"Unexpected argument '{token}'.\n" + Usage);
            }

            var equals = token.IndexOf('=', StringComparison.Ordinal);
            var name = equals < 0 ? token : token[..equals];
            var inline = equals < 0 ? null : token[(equals + 1)..];

            switch (name)
            {
                case "--help" when inline is null:
                    return new ParseOutcome.Help(Usage);
                case "--version" when inline is null:
                    return VersionOutcome();
                case "--wait" when inline is null:
                    continue; // the default; accepted so callers can be explicit
                case "--no-repo-context" when inline is null:
                    noRepoContext = true;
                    continue;
            }

            if (name == "--initial" || NotYetFlags.Contains(name, StringComparer.Ordinal))
            {
                return new ParseOutcome.Invalid($"{name} {NotYet}.");
            }
            if (!ValuedOptions.Contains(name, StringComparer.Ordinal))
            {
                return new ParseOutcome.Invalid($"Unknown option '{name}'.\n" + Usage);
            }
            if (values.ContainsKey(name))
            {
                return new ParseOutcome.Invalid($"{name} was given more than once.");
            }

            string? value = inline;
            if (value is null)
            {
                var next = index + 1;
                if (next < args.Count && args[next] == "--")
                {
                    next++; // the value after '--' is literal, even with a leading dash
                }
                else if (next < args.Count && args[next].StartsWith('-'))
                {
                    return new ParseOutcome.Invalid($"{name} needs a value; '{args[next]}' looks like an option. Use {name}=<value> or {name} -- <value>.");
                }
                if (next >= args.Count)
                {
                    return new ParseOutcome.Invalid($"{name} needs a value.");
                }
                value = args[next];
                index = next;
            }
            if (value.Length == 0)
            {
                return new ParseOutcome.Invalid($"{name} needs a non-empty value.");
            }
            values[name] = value;
        }

        string[] required = ["--base", "--local", "--remote", "--merged"];
        var missing = required.Where(option => !values.ContainsKey(option)).ToArray();
        if (missing.Length > 0)
        {
            return new ParseOutcome.Invalid($"Missing required option(s): {string.Join(", ", missing)}.\n" + Usage);
        }

        return new ParseOutcome.Parsed(new MergeToolArguments(
            values["--base"],
            values["--local"],
            values["--remote"],
            values["--merged"],
            values.GetValueOrDefault("--label-base"),
            values.GetValueOrDefault("--label-local"),
            values.GetValueOrDefault("--label-remote"),
            values.GetValueOrDefault("--repo"),
            noRepoContext));
    }
}
