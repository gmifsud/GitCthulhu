using Resolve.Core.Cli;
using Resolve.Core.Git;
using Resolve.Core.Resolution;
using Resolve.Core.Merge;
using Resolve.Core.Session;

namespace Resolve.MergeToolProbe;

/// <summary>
/// Test-only stand-in for the window: takes the same <c>merge ...</c> arguments as <c>cthulhu-resolve</c>, drives a
/// <see cref="MergeToolSession"/> the way the window would, and returns the session's exit code. The action comes from
/// <c>PROBE_ACTION</c>: <c>accept-left-all</c>, <c>cancel</c> or <c>exit-unresolved</c>.
/// </summary>
internal static class Program
{
    private const string ActionVariable = "PROBE_ACTION";

    public static async Task<int> Main(string[] args)
    {
        var action = Environment.GetEnvironmentVariable(ActionVariable);
        if (action is not ("accept-left-all" or "cancel" or "exit-unresolved"))
        {
            await Console.Error.WriteLineAsync($"{ActionVariable} must be accept-left-all, cancel or exit-unresolved.");
            return (int)ExitCode.Usage;
        }

        if (CommandLine.Parse(args) is not ParseOutcome.Parsed parsed)
        {
            await Console.Error.WriteLineAsync("Invalid arguments.");
            return (int)ExitCode.Usage;
        }

        var backupRoot = Directory.CreateTempSubdirectory("cthulhu-probe-backups-").FullName;
        try
        {
            return await RunAsync(action, parsed.Arguments, new SessionOptions(backupRoot, TimeProvider.System));
        }
        finally
        {
            Directory.Delete(backupRoot, recursive: true);
        }
    }

    private static async Task<int> RunAsync(string action, MergeToolArguments arguments, SessionOptions options)
    {
        var outcome = await MergeToolSession.OpenAsync(new GitRunner(), arguments, options, CancellationToken.None);
        if (outcome is OpenOutcome.Failed failed)
        {
            await Console.Error.WriteLineAsync(failed.Message);
            return (int)failed.Code;
        }

        var session = ((OpenOutcome.Opened)outcome).Session;
        switch (action)
        {
            case "cancel":
                return (int)session.Cancel();
            case "exit-unresolved":
                return (int)session.ExitUnresolved();
            default:
                foreach (var region in session.Document.Regions.Where(region => region.Kind != RegionKind.Equal).ToArray())
                {
                    var command = new RegionCommand.Accept(region.RegionId, ResolutionChoice.Left);
                    if (session.Document.Apply(command) is CommandOutcome.NeedsConfirmation)
                    {
                        session.Document.Apply(command, confirmed: true);
                    }
                }
                return (int)session.SaveAndExit();
        }
    }
}
