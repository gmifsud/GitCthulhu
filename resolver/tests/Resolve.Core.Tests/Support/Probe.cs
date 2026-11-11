namespace Resolve.Core.Tests.Support;

/// <summary>Locates the test-only <c>resolve-probe</c> console, copied beside the test assembly, and builds the git config line that runs it.</summary>
internal static class Probe
{
    public static string DllPath => Path.Join(AppContext.BaseDirectory, "probe", "resolve-probe.dll");

    /// <summary>
    /// A <c>mergetool.&lt;name&gt;.cmd</c> value. Git runs it through its own shell on every platform, so both paths are
    /// double-quoted with forward slashes and the shell variables stay inside double quotes.
    /// </summary>
    public static string Command()
    {
        if (!File.Exists(DllPath))
        {
            throw new FileNotFoundException("The probe was not copied next to the test assembly.", DllPath);
        }
        var host = Environment.GetEnvironmentVariable("DOTNET_HOST_PATH");
        return $"{Quote(string.IsNullOrEmpty(host) ? "dotnet" : host)} {Quote(DllPath)} " +
            "merge --base \"$BASE\" --local \"$LOCAL\" --remote \"$REMOTE\" --merged \"$MERGED\"";
    }

    private static string Quote(string path) => "\"" + path.Replace('\\', '/') + "\"";
}
