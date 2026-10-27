using System.Runtime.InteropServices;
using System.Security.Cryptography;

namespace Resolve.Core.Persistence;

/// <summary>What a file looked like when it was loaded. Compared before every save (spec §9.2).</summary>
public sealed record FileFingerprint(long Length, DateTime LastWriteUtc, string Sha256)
{
    public static FileFingerprint? Capture(string path)
    {
        var info = new FileInfo(path);
        if (!info.Exists)
        {
            return null;
        }
        using var stream = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete);
        return new FileFingerprint(info.Length, info.LastWriteTimeUtc, Convert.ToHexStringLower(SHA256.HashData(stream)));
    }

    /// <summary>Content identity: length and hash. Timestamps alone are not trusted (coarse on some filesystems).</summary>
    public bool SameContent(FileFingerprint? other) =>
        other is not null && Length == other.Length && string.Equals(Sha256, other.Sha256, StringComparison.Ordinal);
}

public enum SaveStep
{
    BeforeTempWrite,
    AfterTempWrite,
    BeforeReplace,
    AfterReplace,
}

/// <summary>Test seam: throw at a step to prove the target survives a crash or I/O error there.</summary>
public interface ISaveFaults
{
    void At(SaveStep reached);
}

public enum SaveFailure
{
    DiskFull,
    PermissionDenied,
    TargetIsSymlink,
    Other,
}

public abstract record SaveOutcome
{
    private SaveOutcome() { }

    public sealed record Saved(FileFingerprint Fingerprint, string? BackupPath) : SaveOutcome;

    /// <summary>The file changed since it was loaded; nothing was written.</summary>
    public sealed record ExternallyModified(FileFingerprint? Expected, FileFingerprint? Actual) : SaveOutcome;

    /// <summary>Nothing was replaced; the target holds its previous bytes.</summary>
    public sealed record Failed(SaveFailure Reason, string Message) : SaveOutcome;
}

/// <summary>
/// Write-temp → flush to disk → atomic replace → flush directory. A crash at any point leaves either the old
/// file or the new one, never a torn one. The temp file lives beside the target so the rename never
/// crosses a filesystem.
/// </summary>
public sealed partial class AtomicFileWriter
{
    /// <summary>Prefix of our temp files; start-up cleanup removes only names that match it.</summary>
    public const string TempMarker = ".cthulhu-";

    private readonly ISaveFaults? _faults;

    public AtomicFileWriter(ISaveFaults? faults = null) => _faults = faults;

    /// <param name="expected">Fingerprint at load. Null means the file must not exist yet.</param>
    /// <param name="backupDirectory">Where to copy the current file before replacing it; null for no backup.</param>
    public SaveOutcome Save(string target, ReadOnlySpan<byte> content, FileFingerprint? expected, string? backupDirectory)
    {
        ArgumentException.ThrowIfNullOrEmpty(target);
        var info = new FileInfo(target);
        if (info.LinkTarget is not null)
        {
            return new SaveOutcome.Failed(SaveFailure.TargetIsSymlink, $"'{target}' is a symbolic link; resolve it as a file-level conflict.");
        }
        var actual = FileFingerprint.Capture(target);
        var unchanged = expected is null ? actual is null : expected.SameContent(actual);
        if (!unchanged)
        {
            return new SaveOutcome.ExternallyModified(expected, actual);
        }

        var directory = Path.GetDirectoryName(Path.GetFullPath(target))!;
        var temp = Path.Join(directory, $".{info.Name}{TempMarker}{Convert.ToHexStringLower(RandomNumberGenerator.GetBytes(6))}.tmp");
        string? backup = null;
        try
        {
            if (actual is not null && backupDirectory is not null)
            {
                Directory.CreateDirectory(backupDirectory);
                backup = Path.Join(backupDirectory, $"{info.Name}.{DateTime.UtcNow:yyyyMMddTHHmmssfffZ}.bak");
                File.Copy(target, backup, overwrite: false);
            }

            _faults?.At(SaveStep.BeforeTempWrite);
            using (var stream = new FileStream(temp, FileMode.CreateNew, FileAccess.Write, FileShare.None))
            {
                stream.Write(content);
                stream.Flush(flushToDisk: true);
            }
            _faults?.At(SaveStep.AfterTempWrite);

            if (actual is not null && !OperatingSystem.IsWindows())
            {
                File.SetUnixFileMode(temp, File.GetUnixFileMode(target));
            }

            _faults?.At(SaveStep.BeforeReplace);
            if (actual is not null && OperatingSystem.IsWindows())
            {
                // ReplaceFile keeps the target's ACL and attributes.
                File.Replace(temp, target, destinationBackupFileName: null, ignoreMetadataErrors: false);
            }
            else
            {
                File.Move(temp, target, overwrite: true); // rename(2): atomic within one filesystem.
            }
            _faults?.At(SaveStep.AfterReplace);
            FlushDirectory(directory);
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException)
        {
            TryDelete(temp);
            return new SaveOutcome.Failed(Classify(error), error.Message);
        }
        return new SaveOutcome.Saved(FileFingerprint.Capture(target)!, backup);
    }

    /// <summary>Removes temp files of ours older than <paramref name="age"/> left by a crash. Never touches other names.</summary>
    public static int CleanOrphans(string directory, TimeSpan age)
    {
        var removed = 0;
        foreach (var path in Directory.EnumerateFiles(directory, $".*{TempMarker}*.tmp"))
        {
            if (DateTime.UtcNow - File.GetLastWriteTimeUtc(path) > age && TryDelete(path))
            {
                removed++;
            }
        }
        return removed;
    }

    private static SaveFailure Classify(Exception error) => error switch
    {
        UnauthorizedAccessException => SaveFailure.PermissionDenied,
        // ENOSPC on Unix (28), ERROR_DISK_FULL / ERROR_HANDLE_DISK_FULL on Windows.
        IOException io when (io.HResult & 0xFFFF) is 28 or 0x70 or 0x27 => SaveFailure.DiskFull,
        _ => SaveFailure.Other,
    };

    private static bool TryDelete(string path)
    {
        try
        {
            File.Delete(path);
            return true;
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException)
        {
            return false;
        }
    }

    /// <summary>POSIX: fsync the directory so the rename itself is durable. Windows has no equivalent need.</summary>
    private static void FlushDirectory(string directory)
    {
        if (OperatingSystem.IsWindows())
        {
            return;
        }
        var descriptor = Open(directory, 0 /* O_RDONLY */);
        if (descriptor < 0)
        {
            return; // Best effort: the data itself is already on disk.
        }
        _ = Fsync(descriptor);
        _ = Close(descriptor);
    }

    [LibraryImport("libc", EntryPoint = "open", StringMarshalling = StringMarshalling.Utf8, SetLastError = true)]
    private static partial int Open(string path, int flags);

    [LibraryImport("libc", EntryPoint = "fsync", SetLastError = true)]
    private static partial int Fsync(int descriptor);

    [LibraryImport("libc", EntryPoint = "close")]
    private static partial int Close(int descriptor);
}
