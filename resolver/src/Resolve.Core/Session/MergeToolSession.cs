using System.Security.Cryptography;
using System.Text;
using System.Text.RegularExpressions;
using Resolve.Core.Cli;
using Resolve.Core.Git;
using Resolve.Core.Merge;
using Resolve.Core.Persistence;
using Resolve.Core.Resolution;
using Resolve.Core.Text;

namespace Resolve.Core.Session;

/// <summary>Where backups go and which clock the document uses.</summary>
public sealed record SessionOptions
{
    private const string AppFolder = "CthulhuResolve";
    private const string BackupFolder = "backups";

    /// <summary>Backups must never land in the worktree, which is the tool's working directory: the root is always absolute.</summary>
    public SessionOptions(string backupRoot, TimeProvider clock)
    {
        ArgumentNullException.ThrowIfNull(backupRoot);
        ArgumentNullException.ThrowIfNull(clock);
        if (!Path.IsPathFullyQualified(backupRoot))
        {
            throw new ArgumentException("The backup root must be a fully qualified path.", nameof(backupRoot));
        }
        BackupRoot = backupRoot;
        Clock = clock;
    }

    public string BackupRoot { get; }

    public TimeProvider Clock { get; }

    public static SessionOptions Default
    {
        get
        {
            var data = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData, Environment.SpecialFolderOption.Create);
            var root = string.IsNullOrEmpty(data) || !Path.IsPathFullyQualified(data) ? Path.GetTempPath() : data;
            return new SessionOptions(Path.Join(root, AppFolder, BackupFolder), TimeProvider.System);
        }
    }
}

public abstract record OpenOutcome
{
    private OpenOutcome() { }

    public sealed record Opened(MergeToolSession Session) : OpenOutcome;

    public sealed record Failed(ExitCode Code, string Message) : OpenOutcome;
}

public abstract record SaveResult
{
    private SaveResult() { }

    public sealed record Saved : SaveResult;

    /// <summary>MERGED changed since we loaded (or last saved) it; nothing was written.</summary>
    public sealed record ExternalChange : SaveResult;

    public sealed record Failed(string Message) : SaveResult;
}

/// <summary>
/// One <c>git mergetool</c> invocation: the loaded inputs, the plan, the resolution document, and the only code that
/// writes MERGED. Git trusts the exit code, so every exit path here either wrote the file atomically or did not touch it.
/// </summary>
public sealed partial class MergeToolSession
{
    private const string NoRepoKey = "no-repo";
    private const int BackupKeyLength = 16;

    private readonly SessionOptions _options;
    private readonly AtomicFileWriter _writer = new();

    private FileFingerprint? _expected;
    private bool _backedUp;
    private int _savedVersion = -1;
    private TextEncodingKind _savedEncoding;
    private TextEncodingKind _resultEncoding;

    internal MergeToolSession(MergeInputs inputs, MergePlan plan, ResolutionDocument document, SessionOptions options)
    {
        Inputs = inputs;
        Plan = plan;
        Document = document;
        _options = options;
        _expected = inputs.MergedAtLoad;
        _resultEncoding = inputs.Left.Encoding;
    }

    public MergeInputs Inputs { get; }

    public MergePlan Plan { get; }

    public ResolutionDocument Document { get; }

    public bool EncodingsDiffer => Inputs.Left.Encoding != Inputs.Right.Encoding;

    /// <summary>The encoding MERGED is written in; defaults to LEFT's. Changing it makes the session unsaved.</summary>
    public TextEncodingKind ResultEncoding
    {
        get => _resultEncoding;
        set => _resultEncoding = value;
    }

    /// <summary>True when the last successful save wrote exactly the current document in the current encoding.</summary>
    public bool IsSaved => _savedVersion == Document.Version && _savedEncoding == _resultEncoding;

    public static async Task<OpenOutcome> OpenAsync(IGitRunner runner, MergeToolArguments arguments, SessionOptions options, CancellationToken ct)
    {
        ArgumentNullException.ThrowIfNull(runner);
        ArgumentNullException.ThrowIfNull(arguments);
        ArgumentNullException.ThrowIfNull(options);
        try
        {
            var loaded = await MergeInputLoader.LoadAsync(runner, arguments, ct).ConfigureAwait(false);
            if (loaded is LoadOutcome.Unreadable unreadable)
            {
                return new OpenOutcome.Failed(ExitCode.InputUnreadable, unreadable.Reason);
            }
            var inputs = ((LoadOutcome.Loaded)loaded).Inputs;

            var notUu = await CheckBothModifiedAsync(runner, inputs, ct).ConfigureAwait(false);
            if (notUu is not null)
            {
                return new OpenOutcome.Failed(ExitCode.InputUnreadable, notUu);
            }

            var plan = await MergePlanning.PlanAsync(runner, inputs, ct).ConfigureAwait(false);
            var document = ResolutionDocument.Create(plan, inputs.Base.Lines, inputs.Left.Lines, inputs.Right.Lines, options.Clock);
            return new OpenOutcome.Opened(new MergeToolSession(inputs, plan, document, options));
        }
        catch (Exception error) when (error is GitCommandException or FormatException or IOException or UnauthorizedAccessException)
        {
            return new OpenOutcome.Failed(ExitCode.InputUnreadable, error.Message);
        }
    }

    public SaveResult Save(bool overwriteExternalChange = false)
    {
        if (IsCancelled)
        {
            return new SaveResult.Failed("the session was cancelled");
        }
        var version = Document.Version;
        var encoding = _resultEncoding;
        byte[] bytes;
        try
        {
            bytes = TextCodec.Encode(encoding, TextCodec.SplitLines(Document.Text));
        }
        catch (EncoderFallbackException error)
        {
            // Ruling R32: text the encoding cannot represent (a lone surrogate) is a failed write, never a crash.
            return new SaveResult.Failed($"the result cannot be encoded: {error.Message}");
        }
        var backupDirectory = _backedUp ? null : BackupDirectory();
        try
        {
            var outcome = _writer.Save(Inputs.MergedPath, bytes, _expected, backupDirectory);
            if (outcome is SaveOutcome.ExternallyModified modified)
            {
                if (!overwriteExternalChange)
                {
                    return new SaveResult.ExternalChange();
                }
                // The user confirmed overwriting what is there now: expect exactly that, so a further change still blocks.
                // The bytes being replaced are someone else's work: always keep them, even after the first-save backup.
                outcome = _writer.Save(Inputs.MergedPath, bytes, modified.Actual, BackupDirectory());
            }
            switch (outcome)
            {
                case SaveOutcome.Saved saved:
                    _expected = saved.Fingerprint;
                    _backedUp = true; // the first successful save backs up; later saves would only copy our own output
                    _savedVersion = version;
                    _savedEncoding = encoding;
                    return new SaveResult.Saved();
                case SaveOutcome.ExternallyModified:
                    return new SaveResult.ExternalChange();
                case SaveOutcome.Failed failed:
                    return new SaveResult.Failed(failed.Message);
                default:
                    throw new InvalidOperationException("Unknown save outcome.");
            }
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException)
        {
            return new SaveResult.Failed(error.Message);
        }
    }

    /// <summary>Writes only a complete document; otherwise returns <see cref="ExitCode.Unresolved"/> without touching MERGED.</summary>
    public ExitCode SaveAndExit(bool overwriteExternalChange = false) =>
        Document.IsComplete && !IsCancelled ? Finish(overwriteExternalChange, ExitCode.Resolved) : ExitCode.Unresolved;

    /// <summary>Saves what RESULT shows, conflicts and all, and tells git the file is not resolved.</summary>
    public ExitCode ExitUnresolved(bool overwriteExternalChange = false) =>
        IsCancelled ? ExitCode.Unresolved : Finish(overwriteExternalChange, ExitCode.Unresolved);

    /// <summary>True once <see cref="Cancel"/> has been called; cancelling is final and no later call writes.</summary>
    public bool IsCancelled { get; private set; }

    /// <summary>Never touches the disk.</summary>
    public ExitCode Cancel()
    {
        IsCancelled = true;
        return ExitCode.Unresolved;
    }

    /// <summary>One warning per 1-based line of RESULT that looks like a conflict marker.</summary>
    public IReadOnlyList<string> MarkerWarnings()
    {
        var warnings = new List<string>();
        var lines = TextCodec.SplitLines(Document.Text);
        for (var index = 0; index < lines.Length; index++)
        {
            if (MarkerLike().IsMatch(lines[index].Content))
            {
                warnings.Add($"conflict-marker-like line at {index + 1}");
            }
        }
        return warnings;
    }

    [GeneratedRegex(@"^(<{7}|={7}|>{7}|\|{7})( |$)", RegexOptions.CultureInvariant)]
    private static partial Regex MarkerLike();

    private ExitCode Finish(bool overwriteExternalChange, ExitCode onSaved) => Save(overwriteExternalChange) switch
    {
        SaveResult.Saved => onSaved,
        SaveResult.ExternalChange => ExitCode.ExternalChangeDeclined,
        _ => ExitCode.WriteFailed,
    };

    private string BackupDirectory()
    {
        var worktree = Inputs.Repo?.Worktree ?? NoRepoKey;
        var key = Convert.ToHexStringLower(SHA256.HashData(Encoding.UTF8.GetBytes(worktree)))[..BackupKeyLength];
        var name = (Inputs.RelativePath?.Value ?? Path.GetFileName(Inputs.MergedPath)).Replace('/', '_');
        return Path.Join(_options.BackupRoot, key, name);
    }

    /// <summary>Returns a refusal message unless the path is a both-modified conflict; no repository context means no check.</summary>
    private static async Task<string?> CheckBothModifiedAsync(IGitRunner runner, MergeInputs inputs, CancellationToken ct)
    {
        if (inputs.Repo is null || inputs.RelativePath is not { } relative)
        {
            return null;
        }
        var unmerged = await UnmergedIndex.ListAsync(runner, inputs.Repo, ct).ConfigureAwait(false);
        var entry = unmerged.FirstOrDefault(path => path.Path == relative);
        return entry is { Kind: UnmergedKind.BothModified }
            ? null
            : $"{relative.Value} is not a both-modified conflict (Phase 1 handles UU only)";
    }
}
