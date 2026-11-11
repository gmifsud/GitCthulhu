using Resolve.Core.Session;
using Resolve.Core.Tests.Support;

namespace Resolve.App.Tests.Support;

/// <summary>Opens real merge-tool sessions over real git conflicts for UI tests. Task 10 extends it.</summary>
internal static class Sessions
{
    private const string FileName = "a.txt";

    /// <summary>Criterion 1: 50 lines; LEFT changes 3 and 20, RIGHT changes 20 and 40 (a conflict at 20).</summary>
    public static MergeToolSession Criterion1() => Block(async () => await Fixtures.Criterion1ConflictAsync());

    /// <summary>Criterion 1 with CRLF line endings throughout, so the document newline is CRLF.</summary>
    public static MergeToolSession Crlf() => FromLines(
        newLine: "\r\n",
        lineCount: 50,
        left: lines => { lines[2] = "left 3"; lines[19] = "left 20"; },
        right: lines => { lines[19] = "right 20"; lines[39] = "right 40"; });

    /// <summary>
    /// A Criterion-1-shaped file of <paramref name="lineCount"/> lines: LEFT changes every 100th line from line 3,
    /// RIGHT every 100th from line 53, and both change every 500th line from line 20 (a conflict).
    /// </summary>
    public static MergeToolSession Long(int lineCount = 2000) => FromLines(
        newLine: "\n",
        lineCount,
        left: lines =>
        {
            for (var index = 2; index < lines.Length; index += 100)
            {
                lines[index] = $"left {index + 1}";
            }
            for (var index = 19; index < lines.Length; index += 500)
            {
                lines[index] = $"left conflict {index + 1}";
            }
        },
        right: lines =>
        {
            for (var index = 52; index < lines.Length; index += 100)
            {
                lines[index] = $"right {index + 1}";
            }
            for (var index = 19; index < lines.Length; index += 500)
            {
                lines[index] = $"right conflict {index + 1}";
            }
        });

    /// <summary>
    /// An empty-base (insert/insert) conflict after "c" — empty in the initial RESULT — between a LEFT change of "b"
    /// and a RIGHT change of "e", plus a modify/modify conflict on "g".
    /// </summary>
    public static MergeToolSession InsertInsert() => FromTexts(
        "a\nb\nc\nd\ne\nf\ng\nh\n",
        "a\nleft b\nc\nleft new\nd\ne\nf\nleft g\nh\n",
        "a\nb\nc\nright new\nd\nright e\nf\nright g\nh\n");

    /// <summary>Stage 1 exists but is an empty file (both sides filled an empty file): the base is real and empty.</summary>
    public static MergeToolSession EmptyStageBase() => FromTexts("", "left\n", "right\n");

    private static MergeToolSession FromTexts(string baseText, string left, string right) =>
        Block(async () =>
        {
            var fixture = await GitFixture.CreateAsync();
            await fixture.ConflictAsync(
                f => f.Write(FileName, baseText),
                f => f.Write(FileName, left),
                f => f.Write(FileName, right));
            return fixture;
        });

    private static MergeToolSession FromLines(string newLine, int lineCount, Action<string[]> left, Action<string[]> right) =>
        Block(async () =>
        {
            var baseLines = Enumerable.Range(1, lineCount).Select(number => $"line {number}").ToArray();
            string Text(Action<string[]> change)
            {
                var lines = (string[])baseLines.Clone();
                change(lines);
                return string.Concat(lines.Select(line => line + newLine));
            }
            var fixture = await GitFixture.CreateAsync();
            await fixture.ConflictAsync(
                f => f.Write(FileName, Text(_ => { })),
                f => f.Write(FileName, Text(left)),
                f => f.Write(FileName, Text(right)));
            return fixture;
        });

    /// <summary>Synchronous test methods may block on the open; the open runs on the thread pool because the UI thread's synchronisation context would deadlock the awaits.</summary>
    private static MergeToolSession Block(Func<Task<GitFixture>> conflict) => Task.Run(() => OpenAsync(conflict)).GetAwaiter().GetResult();

    private static async Task<MergeToolSession> OpenAsync(Func<Task<GitFixture>> conflict)
    {
        var fixture = await conflict();
        var arguments = await MergeToolCall.WriteStageFilesAsync(fixture, FileName);
        var outcome = await MergeToolSession.OpenAsync(fixture.Git, arguments, Fixtures.Options(), CancellationToken.None);
        return outcome is OpenOutcome.Opened opened
            ? opened.Session
            : throw new InvalidOperationException($"The session did not open: {outcome}");
    }
}
