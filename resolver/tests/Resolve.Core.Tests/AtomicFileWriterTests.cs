using System.Text;
using Resolve.Core.Persistence;

namespace Resolve.Core.Tests;

/// <summary>Spike S4 (Linux/ext4 part): every failure point leaves the target's previous bytes and no temp file.</summary>
public sealed class AtomicFileWriterTests : IDisposable
{
    private readonly string _directory = Directory.CreateTempSubdirectory("cthulhu-save-").FullName;

    private string Target => Path.Join(_directory, "Service.cs");

    public void Dispose() => Directory.Delete(_directory, recursive: true);

    private sealed class ThrowAt(SaveStep step) : ISaveFaults
    {
        public void At(SaveStep reached)
        {
            if (reached == step)
            {
                throw new IOException($"injected at {reached}");
            }
        }
    }

    [Fact]
    public void Saves_and_reports_the_new_fingerprint()
    {
        File.WriteAllText(Target, "old\n");
        var loaded = FileFingerprint.Capture(Target);
        var outcome = new AtomicFileWriter().Save(Target, "new\n"u8, loaded, Path.Join(_directory, "backups"));
        var saved = Assert.IsType<SaveOutcome.Saved>(outcome);
        Assert.Equal("new\n", File.ReadAllText(Target));
        Assert.Equal("old\n", File.ReadAllText(saved.BackupPath!));
        Assert.True(saved.Fingerprint.SameContent(FileFingerprint.Capture(Target)));
    }

    [Fact]
    public void An_external_change_blocks_the_save()
    {
        File.WriteAllText(Target, "old\n");
        var loaded = FileFingerprint.Capture(Target);
        File.WriteAllText(Target, "formatter rewrote me\n");
        var outcome = new AtomicFileWriter().Save(Target, "ours\n"u8, loaded, null);
        Assert.IsType<SaveOutcome.ExternallyModified>(outcome);
        Assert.Equal("formatter rewrote me\n", File.ReadAllText(Target));
    }

    [Fact]
    public void A_file_that_appeared_since_load_blocks_the_save()
    {
        File.WriteAllText(Target, "someone else\n");
        Assert.IsType<SaveOutcome.ExternallyModified>(new AtomicFileWriter().Save(Target, "ours\n"u8, expected: null, null));
    }

    [Theory]
    [InlineData(SaveStep.BeforeTempWrite)]
    [InlineData(SaveStep.AfterTempWrite)]
    [InlineData(SaveStep.BeforeReplace)]
    public void A_failure_before_replace_leaves_the_target_and_no_temp(SaveStep step)
    {
        File.WriteAllText(Target, "old\n");
        var outcome = new AtomicFileWriter(new ThrowAt(step)).Save(Target, "new\n"u8, FileFingerprint.Capture(Target), null);
        Assert.IsType<SaveOutcome.Failed>(outcome);
        Assert.Equal("old\n", File.ReadAllText(Target));
        Assert.Equal(["Service.cs"], Directory.GetFiles(_directory).Select(Path.GetFileName));
    }

    [Fact]
    public void A_failure_after_replace_still_leaves_a_whole_new_file()
    {
        File.WriteAllText(Target, "old\n");
        var outcome = new AtomicFileWriter(new ThrowAt(SaveStep.AfterReplace)).Save(Target, "new\n"u8, FileFingerprint.Capture(Target), null);
        Assert.IsType<SaveOutcome.Failed>(outcome);
        Assert.Equal("new\n", File.ReadAllText(Target));
    }

    [Fact]
    public void Unix_mode_bits_survive_the_replace()
    {
        if (OperatingSystem.IsWindows())
        {
            return;
        }
        File.WriteAllText(Target, "#!/bin/sh\n");
        const UnixFileMode executable = UnixFileMode.UserRead | UnixFileMode.UserWrite | UnixFileMode.UserExecute | UnixFileMode.GroupRead;
        File.SetUnixFileMode(Target, executable);
        new AtomicFileWriter().Save(Target, "#!/bin/sh\necho\n"u8, FileFingerprint.Capture(Target), null);
        Assert.Equal(executable, File.GetUnixFileMode(Target));
    }

    [Fact]
    public void A_symlink_target_is_refused()
    {
        File.WriteAllText(Path.Join(_directory, "real.txt"), "real\n");
        File.CreateSymbolicLink(Target, "real.txt");
        var outcome = new AtomicFileWriter().Save(Target, "x\n"u8, FileFingerprint.Capture(Target), null);
        Assert.Equal(SaveFailure.TargetIsSymlink, Assert.IsType<SaveOutcome.Failed>(outcome).Reason);
        Assert.Equal("real\n", File.ReadAllText(Path.Join(_directory, "real.txt")));
    }

    [Fact]
    public void Bytes_are_written_exactly()
    {
        var bytes = new byte[] { 0xEF, 0xBB, 0xBF }.Concat(Encoding.UTF8.GetBytes("a\r\nb\nc")).ToArray();
        new AtomicFileWriter().Save(Target, bytes, expected: null, null);
        Assert.Equal(bytes, File.ReadAllBytes(Target));
    }

    [Fact]
    public void Orphan_cleanup_touches_only_our_temp_files()
    {
        var ours = Path.Join(_directory, $".Service.cs{AtomicFileWriter.TempMarker}abc.tmp");
        var theirs = Path.Join(_directory, ".Service.cs.swp");
        File.WriteAllText(ours, "");
        File.WriteAllText(theirs, "");
        File.SetLastWriteTimeUtc(ours, DateTime.UtcNow.AddHours(-2));
        File.SetLastWriteTimeUtc(theirs, DateTime.UtcNow.AddHours(-2));
        Assert.Equal(1, AtomicFileWriter.CleanOrphans(_directory, TimeSpan.FromHours(1)));
        Assert.True(File.Exists(theirs));
    }
}
