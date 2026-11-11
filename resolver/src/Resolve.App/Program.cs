using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.ApplicationLifetimes;
using Resolve.App.Shell;
using Resolve.Core.Cli;
using Resolve.Core.Git;
using Resolve.Core.Persistence;
using Resolve.Core.Session;

namespace Resolve.App;

internal static class Program
{
    /// <summary>Exactly one member is set: an exit code to return now, or a session to show in the window.</summary>
    internal sealed record Prepared(int? ExitCode, MergeToolSession? Session);

    /// <summary>The window result as a process exit code; closing without a result leaves the file unresolved.</summary>
    internal static int ExitCodeOf(ExitCode? windowResult) => (int)(windowResult ?? ExitCode.Unresolved);

    /// <summary>
    /// Everything before the window: parse, check git, open the session. Separate from <see cref="Main"/> because
    /// the Avalonia lifetime must start on the main thread, which these awaits do not resume on.
    /// </summary>
    internal static async Task<Prepared> PrepareAsync(string[] args, TextWriter stdout, TextWriter stderr)
    {
        ArgumentNullException.ThrowIfNull(args);
        ArgumentNullException.ThrowIfNull(stdout);
        ArgumentNullException.ThrowIfNull(stderr);

        switch (CommandLine.Parse(args))
        {
            case ParseOutcome.Invalid invalid:
                await stderr.WriteLineAsync(invalid.Message);
                await stderr.WriteLineAsync(CommandLine.Usage);
                return new Prepared((int)ExitCode.Usage, null);
            case ParseOutcome.Help help:
                await stdout.WriteLineAsync(help.Text);
                return new Prepared(0, null);
            case ParseOutcome.Version version:
                await stdout.WriteLineAsync(version.Text);
                return new Prepared(0, null);
            case ParseOutcome.Parsed parsed:
                return await OpenAsync(parsed.Arguments, stderr);
            default:
                throw new InvalidOperationException("Unhandled parse outcome.");
        }
    }

    [STAThread]
    public static int Main(string[] args)
    {
        var prepared = PrepareAsync(args, Console.Out, Console.Error).GetAwaiter().GetResult();
        return prepared.ExitCode ?? Show(args, prepared.Session!);
    }

    public static AppBuilder BuildAvaloniaApp() => AppBuilder.Configure<App>().UsePlatformDetect();

    private static int Show(string[] args, MergeToolSession session)
    {
        var lifetime = new ClassicDesktopStyleApplicationLifetime
        {
            Args = args,
            ShutdownMode = ShutdownMode.OnMainWindowClose,
        };
        BuildAvaloniaApp().SetupWithLifetime(lifetime);

        MergeToolWindow? window = null;
        window = new MergeToolWindow(session, message => ConfirmDialog.ShowAsync(window!, message));
        lifetime.MainWindow = window;
        lifetime.Start(args);
        return ExitCodeOf(window.Result);
    }

    private static async Task<Prepared> OpenAsync(MergeToolArguments arguments, TextWriter stderr)
    {
        var runner = new GitRunner();
        var git = await GitVersion.ProbeAsync(runner, CancellationToken.None);
        if (git < GitVersion.Minimum)
        {
            await stderr.WriteLineAsync($"git {GitVersion.Minimum.Major}.{GitVersion.Minimum.Minor} or later is required (found {git}).");
            return new Prepared((int)ExitCode.Usage, null);
        }

        var directory = Path.GetDirectoryName(Path.GetFullPath(arguments.Merged));
        if (directory is not null && Directory.Exists(directory))
        {
            AtomicFileWriter.CleanOrphans(directory, TimeSpan.FromHours(1));
        }

        var outcome = await MergeToolSession.OpenAsync(runner, arguments, SessionOptions.Default, CancellationToken.None);
        switch (outcome)
        {
            case OpenOutcome.Opened opened:
                return new Prepared(null, opened.Session);
            case OpenOutcome.Failed failed:
                await stderr.WriteLineAsync(failed.Message);
                return new Prepared((int)failed.Code, null);
            default:
                throw new InvalidOperationException("Unhandled open outcome.");
        }
    }
}
