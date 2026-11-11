using Resolve.App.Shell;
using Resolve.Core.Session;

namespace Resolve.App.Tests.Support;

/// <summary>Shows real merge-tool windows over real git conflicts for UI tests.</summary>
internal static class MergeWindows
{
    /// <summary>Opens the criterion-1 session (50 lines; conflict at line 20, changes at 3 and 40) and returns the shown window.</summary>
    public static MergeToolWindow Criterion1(Func<string, Task<bool>> confirm, TextWriter? warnings = null) =>
        Show(Sessions.Criterion1(), confirm, warnings);

    public static MergeToolWindow Show(MergeToolSession session, Func<string, Task<bool>> confirm, TextWriter? warnings = null)
    {
        var window = new MergeToolWindow(session, confirm, warnings ?? TextWriter.Null) { Width = 1300, Height = 800 };
        window.Show();
        window.UpdateLayout();
        return window;
    }

    /// <summary>A confirm callback that records every prompt and answers the same way each time.</summary>
    public static Func<string, Task<bool>> Answering(bool answer, List<string> asked) => prompt =>
    {
        asked.Add(prompt);
        return Task.FromResult(answer);
    };
}
