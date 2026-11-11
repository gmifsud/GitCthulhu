using Avalonia;

namespace Resolve.App;

internal static class Program
{
    /// <summary>Placeholder entry point; the window task starts the desktop lifetime and wires exit codes.</summary>
    [STAThread]
    public static int Main(string[] args) => args.Length >= 0 ? 0 : 1;

    public static AppBuilder BuildAvaloniaApp() => AppBuilder.Configure<App>().UsePlatformDetect();
}
