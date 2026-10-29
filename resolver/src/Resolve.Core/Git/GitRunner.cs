using System.Diagnostics;
using System.Text;

namespace Resolve.Core.Git;

/// <summary>One git process call. Arguments are an argv vector; nothing is ever passed through a shell.</summary>
public sealed record GitInvocation(string WorkingDirectory, IReadOnlyList<string> Arguments)
{
    /// <summary>Extra environment for this call only. A null value removes the variable.</summary>
    public IReadOnlyDictionary<string, string?> Environment { get; init; } = new Dictionary<string, string?>();

    /// <summary>Bytes written to stdin, then stdin is closed.</summary>
    public ReadOnlyMemory<byte>? StandardInput { get; init; }

    /// <summary>Read-only calls set <c>GIT_OPTIONAL_LOCKS=0</c> so a status refresh never takes index.lock.</summary>
    public bool ReadOnly { get; init; }

    public TimeSpan Timeout { get; init; } = TimeSpan.FromSeconds(60);
}

/// <summary>Raw process outcome. Output stays bytes: paths and content are not assumed to be valid text.</summary>
public sealed record GitResult(int ExitCode, byte[] StandardOutput, byte[] StandardError)
{
    public bool Succeeded => ExitCode == 0;

    /// <summary>stderr decoded for display only; never parsed for control flow.</summary>
    public string ErrorText => Encoding.UTF8.GetString(StandardError).TrimEnd();
}

public interface IGitRunner
{
    Task<GitResult> RunAsync(GitInvocation invocation, CancellationToken cancellationToken);
}

/// <summary>
/// Runs the user's git. Every call gets the same base environment, so behaviour does not depend on the
/// caller's shell: no credential prompts, literal pathspecs, unquoted paths.
/// </summary>
public sealed class GitRunner : IGitRunner
{
    /// <summary>Applied to every call. Literal pathspecs mean a path like <c>*.cs</c> or <c>:(top)</c> is a file name.</summary>
    public static readonly IReadOnlyDictionary<string, string> BaseEnvironment = new Dictionary<string, string>
    {
        ["GIT_TERMINAL_PROMPT"] = "0",
        ["GIT_LITERAL_PATHSPECS"] = "1",
        ["GIT_ASKPASS"] = "",
        ["SSH_ASKPASS"] = "",
    };

    /// <summary>Prepended to every argv: raw UTF-8 paths instead of C-quoted ones.</summary>
    public static readonly IReadOnlyList<string> BaseArguments = ["-c", "core.quotepath=off"];

    private readonly string _executable;

    public GitRunner(string executable = "git") => _executable = executable;

    public async Task<GitResult> RunAsync(GitInvocation invocation, CancellationToken cancellationToken)
    {
        ArgumentNullException.ThrowIfNull(invocation);
        var start = new ProcessStartInfo(_executable)
        {
            WorkingDirectory = invocation.WorkingDirectory,
            RedirectStandardInput = true,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            UseShellExecute = false,
            CreateNoWindow = true,
        };
        foreach (var argument in BaseArguments)
        {
            start.ArgumentList.Add(argument);
        }
        foreach (var argument in invocation.Arguments)
        {
            start.ArgumentList.Add(argument);
        }
        foreach (var (key, value) in BaseEnvironment)
        {
            start.Environment[key] = value;
        }
        if (invocation.ReadOnly)
        {
            start.Environment["GIT_OPTIONAL_LOCKS"] = "0";
        }
        foreach (var (key, value) in invocation.Environment)
        {
            if (value is null)
            {
                start.Environment.Remove(key);
            }
            else
            {
                start.Environment[key] = value;
            }
        }

        using var process = new Process { StartInfo = start };
        process.Start();
        using var timeout = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        timeout.CancelAfter(invocation.Timeout);
        try
        {
            // Read both pipes concurrently: a full stderr pipe must never block stdout.
            var stdout = ReadAllAsync(process.StandardOutput.BaseStream, timeout.Token);
            var stderr = ReadAllAsync(process.StandardError.BaseStream, timeout.Token);
            if (invocation.StandardInput is { } input)
            {
                await process.StandardInput.BaseStream.WriteAsync(input, timeout.Token).ConfigureAwait(false);
            }
            process.StandardInput.Close();
            await process.WaitForExitAsync(timeout.Token).ConfigureAwait(false);
            return new GitResult(process.ExitCode, await stdout.ConfigureAwait(false), await stderr.ConfigureAwait(false));
        }
        catch (OperationCanceledException)
        {
            KillQuietly(process);
            if (cancellationToken.IsCancellationRequested)
            {
                throw;
            }
            throw new TimeoutException($"git {string.Join(' ', invocation.Arguments)} exceeded {invocation.Timeout}.");
        }
    }

    private static async Task<byte[]> ReadAllAsync(Stream stream, CancellationToken cancellationToken)
    {
        using var buffer = new MemoryStream();
        await stream.CopyToAsync(buffer, cancellationToken).ConfigureAwait(false);
        return buffer.ToArray();
    }

    private static void KillQuietly(Process process)
    {
        try
        {
            if (!process.HasExited)
            {
                process.Kill(entireProcessTree: true);
            }
        }
        catch (InvalidOperationException)
        {
            // Already exited between the check and the kill.
        }
    }
}

/// <summary>Raised when git exits non-zero where success was required. Carries git's own message.</summary>
public sealed class GitCommandException : Exception
{
    public GitCommandException(IReadOnlyList<string> arguments, GitResult result)
        : base($"git {string.Join(' ', arguments)} failed ({result.ExitCode}): {result.ErrorText}")
    {
        Arguments = arguments;
        Result = result;
    }

    public GitCommandException() { }

    public GitCommandException(string message) : base(message) { }

    public GitCommandException(string message, Exception innerException) : base(message, innerException) { }

    public IReadOnlyList<string> Arguments { get; } = [];

    public GitResult? Result { get; }
}

public static class GitRunnerExtensions
{
    /// <summary>Runs and requires exit 0.</summary>
    public static async Task<GitResult> RunCheckedAsync(this IGitRunner runner, GitInvocation invocation, CancellationToken cancellationToken)
    {
        ArgumentNullException.ThrowIfNull(runner);
        ArgumentNullException.ThrowIfNull(invocation);
        var result = await runner.RunAsync(invocation, cancellationToken).ConfigureAwait(false);
        return result.Succeeded ? result : throw new GitCommandException(invocation.Arguments, result);
    }
}
