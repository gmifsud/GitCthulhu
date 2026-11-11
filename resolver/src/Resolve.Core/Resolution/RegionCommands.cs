using Resolve.Core.Merge;
using Resolve.Core.Text;

namespace Resolve.Core.Resolution;

/// <summary>A user action on a region (spec §7.3). Applied with <see cref="ResolutionDocument.Apply"/>.</summary>
public abstract record RegionCommand
{
    private RegionCommand()
    {
    }

    /// <summary>Replace the region's text with one side (or both, or the base). <paramref name="Choice"/> is Left, Right, LeftThenRight, RightThenLeft or Base.</summary>
    public sealed record Accept(int RegionId, ResolutionChoice Choice) : RegionCommand;

    /// <summary>Restore the region's initial text and choice.</summary>
    public sealed record Reset(int RegionId) : RegionCommand;

    /// <summary>Accept a manual edit of a conflict as its resolution.</summary>
    public sealed record Acknowledge(int RegionId) : RegionCommand;

    /// <summary>Re-apply the initial side to every non-conflicting region the user has not edited.</summary>
    public sealed record ApplyAllNonConflicting : RegionCommand;
}

public abstract record CommandOutcome
{
    private CommandOutcome()
    {
    }

    /// <summary>The command ran; <paramref name="Changes"/> are the text changes to show.</summary>
    public sealed record Changed(IReadOnlyList<TextChange> Changes, IReadOnlyList<int> SkippedManual) : CommandOutcome;

    /// <summary>The command would replace the user's manual edit; repeat it with <c>confirmed: true</c> to go ahead.</summary>
    public sealed record NeedsConfirmation(int RegionId, string Prompt) : CommandOutcome;

    public sealed record NoChange : CommandOutcome;
}

public sealed partial class ResolutionDocument
{
    private const string ConfirmReplaceManualPrompt = "Replace your manual edit in this region?";

    private static readonly TimeSpan CoalesceWindow = TimeSpan.FromSeconds(1);

    private readonly Stack<UndoEntry> _undo = new();
    private readonly Stack<UndoEntry> _redo = new();

    /// <summary>The entry the next keystroke may still join; cleared by anything but typing.</summary>
    private UndoEntry? _open;

    private enum TypingKind
    {
        None,
        Insert,
        Delete,
    }

    /// <summary>The line terminator typed newlines get: the dominant one of LEFT, else RIGHT, else LF (spec §9.1).</summary>
    public string NewLine { get; }

    public CommandOutcome Apply(RegionCommand command, bool confirmed = false)
    {
        ArgumentNullException.ThrowIfNull(command);
        return command switch
        {
            RegionCommand.Accept accept => AcceptRegion(accept, confirmed),
            RegionCommand.Reset reset => ResetRegion(reset.RegionId, confirmed),
            RegionCommand.Acknowledge acknowledge => AcknowledgeRegion(acknowledge.RegionId),
            RegionCommand.ApplyAllNonConflicting => ApplyAllNonConflicting(),
            _ => throw new ArgumentException($"Unknown command {command}.", nameof(command)),
        };
    }

    /// <summary>Reverts the latest undo entry, restoring text and region states together. Null when there is nothing to undo.</summary>
    public IReadOnlyList<TextChange>? Undo()
    {
        if (!_undo.TryPop(out var entry))
        {
            return null;
        }
        _open = null;
        var shown = new TextChange[entry.Inverse.Count];
        for (var index = entry.Inverse.Count - 1; index >= 0; index--)
        {
            ReplaceText(entry.Inverse[index]);
            shown[entry.Inverse.Count - 1 - index] = entry.Inverse[index];
        }
        Restore(entry.Before);
        _redo.Push(entry);
        Version++;
        return shown;
    }

    /// <summary>Re-applies the latest undone entry. Null when there is nothing to redo.</summary>
    public IReadOnlyList<TextChange>? Redo()
    {
        if (!_redo.TryPop(out var entry))
        {
            return null;
        }
        _open = null;
        foreach (var change in entry.Forward)
        {
            ReplaceText(change);
        }
        Restore(entry.After);
        _undo.Push(entry);
        Version++;
        return [.. entry.Forward];
    }

    private static ResolutionChoice InitialChoice(RegionKind kind) => kind switch
    {
        RegionKind.Equal => ResolutionChoice.Base,
        RegionKind.LeftOnly or RegionKind.BothSame => ResolutionChoice.Left,
        RegionKind.RightOnly => ResolutionChoice.Right,
        _ => ResolutionChoice.Unresolved,
    };

    private static string DominantNewLine(IReadOnlyList<Line> left, IReadOnlyList<Line> right)
    {
        foreach (var side in new[] { left, right })
        {
            int lf = 0, crlf = 0;
            foreach (var line in side)
            {
                lf += line.Ending == LineEnding.Lf ? 1 : 0;
                crlf += line.Ending == LineEnding.CrLf ? 1 : 0;
            }
            if (lf != crlf)
            {
                return lf > crlf ? "\n" : "\r\n";
            }
        }
        return "\n";
    }

    /// <summary>
    /// The text a region holds for a choice (spec §10.2). Both-sides uses the conflict's zdiff3 trim so lines the
    /// sides share are not duplicated. A line without a terminator that is followed by more text gets
    /// <paramref name="newLine"/>, so two lines never fuse.
    /// </summary>
    private static string RegionText(PlanRegion region, ResolutionChoice choice, IReadOnlyList<Line> baseLines, IReadOnlyList<Line> left, IReadOnlyList<Line> right, string newLine)
    {
        LineRange b = region.Base, l = region.Left, r = region.Right;
        var (prefix, suffix) = (region.Trim.CommonPrefix, region.Trim.CommonSuffix);
        (IReadOnlyList<Line> Lines, int Start, int End)[] parts = choice switch
        {
            ResolutionChoice.Left => [(left, l.Start, l.End)],
            ResolutionChoice.Right => [(right, r.Start, r.End)],
            ResolutionChoice.Base or ResolutionChoice.Unresolved => [(baseLines, b.Start, b.End)],
            ResolutionChoice.LeftThenRight => [(left, l.Start, l.End - suffix), (right, r.Start + prefix, r.End - suffix), (left, l.End - suffix, l.End)],
            ResolutionChoice.RightThenLeft => [(right, r.Start, r.End - suffix), (left, l.Start + prefix, l.End - suffix), (right, r.End - suffix, r.End)],
            _ => throw new ArgumentOutOfRangeException(nameof(choice), choice, "Not a text-producing choice."),
        };
        var lines = new List<Line>();
        foreach (var (source, start, end) in parts)
        {
            for (var index = start; index < end; index++)
            {
                lines.Add(source[index]);
            }
        }
        var text = new System.Text.StringBuilder();
        for (var index = 0; index < lines.Count; index++)
        {
            var line = lines[index];
            text.Append(line.Content).Append(line.Ending == LineEnding.None && index < lines.Count - 1 ? newLine : line.Terminator);
        }
        return text.ToString();
    }

    private string ApplyNewLine(string inserted)
    {
        if (!inserted.Contains('\n', StringComparison.Ordinal))
        {
            return inserted;
        }
        var unified = inserted.Replace("\r\n", "\n", StringComparison.Ordinal);
        return NewLine == "\n" ? unified : unified.Replace("\n", NewLine, StringComparison.Ordinal);
    }

    private static TypingKind TypingKindOf(TextChange change) => change switch
    {
        { RemovedLength: 0, InsertedText.Length: 1 } => TypingKind.Insert,
        { RemovedLength: 1, InsertedText.Length: 0 } => TypingKind.Delete,
        _ => TypingKind.None,
    };

    /// <summary>The region a single-character edit belongs to, judged before it is applied.</summary>
    private int TypingRegionId(TextChange change, TypingKind kind)
    {
        if (kind == TypingKind.Insert)
        {
            return _regions[ChooseForInsertion(change.Offset)].Id;
        }
        foreach (var region in _regions)
        {
            if (region.Start <= change.Offset && change.Offset < region.End)
            {
                return region.Id;
            }
        }
        return -1;
    }

    private int IndexOfRegion(int regionId)
    {
        for (var index = 0; index < _regions.Length; index++)
        {
            if (_regions[index].Id == regionId)
            {
                return index;
            }
        }
        throw new ArgumentOutOfRangeException(nameof(regionId), regionId, "No such region.");
    }

    private CommandOutcome AcceptRegion(RegionCommand.Accept accept, bool confirmed)
    {
        if (accept.Choice is not (ResolutionChoice.Left or ResolutionChoice.Right or ResolutionChoice.LeftThenRight or ResolutionChoice.RightThenLeft or ResolutionChoice.Base))
        {
            throw new ArgumentOutOfRangeException(nameof(accept), accept.Choice, "Not an accept choice.");
        }
        var index = IndexOfRegion(accept.RegionId);
        if (_regions[index].Kind == RegionKind.Equal)
        {
            return new CommandOutcome.NoChange();
        }
        return _regions[index].Choice == ResolutionChoice.Manual && !confirmed
            ? new CommandOutcome.NeedsConfirmation(accept.RegionId, ConfirmReplaceManualPrompt)
            : RunCommand(entry => ReplaceRegion(index, RegionText(Plan.Regions[index], accept.Choice, _base, _left, _right, NewLine), accept.Choice, entry));
    }

    private CommandOutcome ResetRegion(int regionId, bool confirmed)
    {
        var index = IndexOfRegion(regionId);
        if (IsInitial(index))
        {
            return new CommandOutcome.NoChange();
        }
        return _regions[index].Choice == ResolutionChoice.Manual && !confirmed
            ? new CommandOutcome.NeedsConfirmation(regionId, ConfirmReplaceManualPrompt)
            : RunCommand(entry => ReplaceInitial(index, entry));
    }

    private CommandOutcome AcknowledgeRegion(int regionId)
    {
        var index = IndexOfRegion(regionId);
        var region = _regions[index];
        if (region is not { Kind: RegionKind.Conflict, Choice: ResolutionChoice.Manual, Acknowledged: false })
        {
            return new CommandOutcome.NoChange();
        }
        return RunCommand(_ => region.Acknowledged = true);
    }

    private CommandOutcome ApplyAllNonConflicting()
    {
        var skipped = new List<int>();
        var outcome = RunCommand(entry =>
        {
            for (var index = 0; index < _regions.Length; index++)
            {
                var region = _regions[index];
                if (region.Kind is RegionKind.Equal or RegionKind.Conflict)
                {
                    continue;
                }
                if (region.Choice == ResolutionChoice.Manual)
                {
                    skipped.Add(region.Id);
                }
                else if (!IsInitial(index))
                {
                    ReplaceInitial(index, entry);
                }
            }
        });
        return outcome is CommandOutcome.Changed changed ? changed with { SkippedManual = skipped } : outcome;
    }

    /// <summary>True when the region holds its initial text with its initial choice.</summary>
    private bool IsInitial(int index)
    {
        var region = _regions[index];
        return region.Choice == InitialChoice(region.Kind)
            && Text.AsSpan(region.Start, region.End - region.Start).SequenceEqual(InitialText(index));
    }

    private string InitialText(int index) =>
        RegionText(Plan.Regions[index], InitialChoice(_regions[index].Kind), _base, _left, _right, NewLine);

    private void ReplaceInitial(int index, UndoEntry entry) =>
        ReplaceRegion(index, InitialText(index), InitialChoice(_regions[index].Kind), entry);

    /// <summary>
    /// Runs <paramref name="action"/> as one undo entry (one version bump); NoChange when it touched nothing.
    /// An acknowledgement changes state without text, so it counts as a change too.
    /// </summary>
    private CommandOutcome RunCommand(Action<UndoEntry> action)
    {
        var entry = new UndoEntry(Snapshot());
        action(entry);
        entry.After = Snapshot();
        if (entry.Forward.Count == 0 && SameState(entry.Before, entry.After))
        {
            return new CommandOutcome.NoChange();
        }
        _views = null;
        Record(entry);
        _open = null;
        Version++;
        return new CommandOutcome.Changed([.. entry.Forward], []);
    }

    private static bool SameState(RegionSnapshot[] first, RegionSnapshot[] second) =>
        first.AsSpan().SequenceEqual(second);

    /// <summary>Replaces the region's whole range; its anchors become that range and its hash that of the new text.</summary>
    private void ReplaceRegion(int index, string text, ResolutionChoice choice, UndoEntry entry)
    {
        var region = _regions[index];
        var length = region.End - region.Start;
        var forward = new TextChange(region.Start, length, text);
        entry.Forward.Add(forward);
        entry.Inverse.Add(new TextChange(region.Start, text.Length, _builder.ToString(region.Start, length)));
        ReplaceText(forward);
        var delta = text.Length - length;
        region.End = region.Start + text.Length;
        for (var next = index + 1; next < _regions.Length; next++)
        {
            _regions[next].Start += delta;
            _regions[next].End += delta;
        }
        region.Choice = choice;
        region.StoredChoice = choice;
        region.Acknowledged = false;
        region.ExpectedHash = Hash(text);
    }

    private void ReplaceText(TextChange change)
    {
        _builder.Remove(change.Offset, change.RemovedLength).Insert(change.Offset, change.InsertedText);
        _text = null;
        _views = null;
    }

    private void Record(UndoEntry entry)
    {
        _undo.Push(entry);
        _redo.Clear();
    }

    private RegionSnapshot[] Snapshot()
    {
        var snapshot = new RegionSnapshot[_regions.Length];
        for (var index = 0; index < snapshot.Length; index++)
        {
            var region = _regions[index];
            snapshot[index] = new RegionSnapshot(region.Start, region.End, region.Choice, region.StoredChoice, region.Acknowledged, region.ExpectedHash);
        }
        return snapshot;
    }

    private void Restore(RegionSnapshot[] snapshot)
    {
        for (var index = 0; index < snapshot.Length; index++)
        {
            var (start, end, choice, stored, acknowledged, hash) = snapshot[index];
            var region = _regions[index];
            (region.Start, region.End, region.Choice, region.StoredChoice, region.Acknowledged, region.ExpectedHash, region.Touched) =
                (start, end, choice, stored, acknowledged, hash, false);
        }
        _views = null;
    }

    /// <summary>A region's complete state at one moment; undo restores whole arrays of these (ruling R10).</summary>
    private readonly record struct RegionSnapshot(int Start, int End, ResolutionChoice Choice, ResolutionChoice StoredChoice, bool Acknowledged, byte[] ExpectedHash);

    /// <summary>One undoable step: its text changes (forward and inverse, in application order) and the region states around it.</summary>
    private sealed class UndoEntry(RegionSnapshot[] before)
    {
        private TypingKind _typing;
        private int _regionId;
        private int _cursor;
        private DateTimeOffset _at;

        public RegionSnapshot[] Before { get; } = before;

        public RegionSnapshot[] After { get; set; } = before;

        public List<TextChange> Forward { get; } = [];

        public List<TextChange> Inverse { get; } = [];

        public void StartTyping(TypingKind typing, int regionId, TextChange change, DateTimeOffset now)
        {
            _typing = typing;
            _regionId = regionId;
            _cursor = CursorAfter(change, typing);
            _at = now;
        }

        /// <summary>Whether a keystroke continues this entry: same kind, same region, adjacent, within the window.</summary>
        public bool Accepts(TextChange change, TypingKind typing, int regionId, DateTimeOffset now)
        {
            if (typing == TypingKind.None || typing != _typing || regionId != _regionId || now - _at > CoalesceWindow)
            {
                return false;
            }
            return typing == TypingKind.Insert
                ? change.Offset == _cursor
                : change.Offset == _cursor || change.Offset == _cursor - 1;
        }

        public void Extend(TextChange change, TextChange inverse, RegionSnapshot[] after, DateTimeOffset now)
        {
            Forward.Add(change);
            Inverse.Add(inverse);
            After = after;
            _cursor = CursorAfter(change, _typing);
            _at = now;
        }

        private static int CursorAfter(TextChange change, TypingKind typing) =>
            typing == TypingKind.Insert ? change.Offset + change.InsertedText.Length : change.Offset;
    }
}
