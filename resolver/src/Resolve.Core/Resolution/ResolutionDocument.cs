using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using Resolve.Core.Merge;
using Resolve.Core.Text;

namespace Resolve.Core.Resolution;

/// <summary>How a region's current text was produced (spec §6.4).</summary>
public enum ResolutionChoice
{
    Unresolved,
    Base,
    Left,
    Right,
    LeftThenRight,
    RightThenLeft,
    Manual,
}

/// <summary>A region as the UI sees it. <c>Start</c>/<c>End</c> are char offsets in the document text.</summary>
public sealed record RegionView(int RegionId, RegionKind Kind, int Start, int End, LineRange ResultLines, ResolutionChoice Choice, bool Acknowledged);

public readonly record struct DocumentCounts(int UnresolvedConflicts, int ManualUnacknowledged, int Changes);

/// <summary>
/// The single writer of the merge result (ADR 0006): the editors only display <see cref="Text"/>. Every plan region
/// owns a pair of anchors into the text (start sticks left, end sticks right, spec §10.4); an edit moves them and
/// re-checks the touched regions against the hash of the text their last action produced (spec §7.2).
/// </summary>
public sealed partial class ResolutionDocument
{
    private readonly StringBuilder _builder = new();
    private readonly RegionState[] _regions;
    private string? _text;
    private RegionView[]? _views;

    private ResolutionDocument(MergePlan plan, TimeProvider clock, RegionState[] regions)
    {
        Plan = plan;
        Clock = clock;
        _regions = regions;
    }

    internal MergePlan Plan { get; }

    internal TimeProvider Clock { get; }

    /// <summary>Starts at 0; incremented by every successful mutation.</summary>
    public int Version { get; private set; }

    public string Text => _text ??= _builder.ToString();

    /// <summary>Every plan region, <c>Equal</c> ones included, in plan order.</summary>
    public IReadOnlyList<RegionView> Regions => _views ??= BuildViews();

    public DocumentCounts Counts
    {
        get
        {
            int unresolved = 0, manual = 0, changes = 0;
            foreach (var region in _regions)
            {
                if (region.Kind == RegionKind.Conflict)
                {
                    unresolved += region.Choice == ResolutionChoice.Unresolved ? 1 : 0;
                    manual += region is { Choice: ResolutionChoice.Manual, Acknowledged: false } ? 1 : 0;
                }
                else if (region.Kind is RegionKind.LeftOnly or RegionKind.RightOnly or RegionKind.BothSame)
                {
                    changes++;
                }
            }
            return new DocumentCounts(unresolved, manual, changes);
        }
    }

    public bool IsComplete => Counts is { UnresolvedConflicts: 0, ManualUnacknowledged: 0 };

    public static ResolutionDocument Create(MergePlan plan, IReadOnlyList<Line> @base, IReadOnlyList<Line> left, IReadOnlyList<Line> right, TimeProvider clock)
    {
        ArgumentNullException.ThrowIfNull(plan);
        ArgumentNullException.ThrowIfNull(@base);
        ArgumentNullException.ThrowIfNull(left);
        ArgumentNullException.ThrowIfNull(right);
        ArgumentNullException.ThrowIfNull(clock);
        var builder = new StringBuilder();
        var regions = new RegionState[plan.Regions.Length];
        for (var index = 0; index < regions.Length; index++)
        {
            var planned = plan.Regions[index];
            var (lines, range, choice) = planned.Kind switch
            {
                RegionKind.Equal => (@base, planned.Base, ResolutionChoice.Base),
                RegionKind.LeftOnly or RegionKind.BothSame => (left, planned.Left, ResolutionChoice.Left),
                RegionKind.RightOnly => (right, planned.Right, ResolutionChoice.Right),
                _ => (@base, planned.Base, ResolutionChoice.Unresolved),
            };
            var start = builder.Length;
            for (var line = range.Start; line < range.End; line++)
            {
                builder.Append(lines[line].ToString());
            }
            regions[index] = new RegionState(planned.Id, planned.Kind)
            {
                Start = start,
                End = builder.Length,
                Choice = choice,
                StoredChoice = choice,
                ExpectedHash = Hash(builder.ToString(start, builder.Length - start)),
            };
        }
        var document = new ResolutionDocument(plan, clock, regions);
        document._builder.Append(builder);
        return document;
    }

    /// <summary>A manual edit. Returns the changes to show: here, the change itself.</summary>
    public IReadOnlyList<TextChange> Edit(TextChange change)
    {
        var (offset, removed, inserted) = change;
        ArgumentNullException.ThrowIfNull(inserted, nameof(change));
        ArgumentOutOfRangeException.ThrowIfNegative(offset);
        ArgumentOutOfRangeException.ThrowIfNegative(removed);
        ArgumentOutOfRangeException.ThrowIfGreaterThan(offset, _builder.Length);
        ArgumentOutOfRangeException.ThrowIfGreaterThan(removed, _builder.Length - offset);

        var end = offset + removed;
        _builder.Remove(offset, removed).Insert(offset, inserted);
        _text = null;
        _views = null;

        var previousEnd = 0;
        for (var index = 0; index < _regions.Length; index++)
        {
            var region = _regions[index];
            region.Touched = offset <= region.End && end >= region.Start;
            // Anchors strictly inside the replaced range collapse onto it; clamping keeps the regions a
            // gap-free, ordered tiling (the earlier region keeps the inserted text).
            // The first region always starts the document: text inserted at 0 belongs to it.
            region.Start = index == 0 ? 0 : Math.Max(MapStart(region.Start, offset, end, inserted.Length), previousEnd);
            region.End = Math.Max(MapEnd(region.End, offset, end, inserted.Length), region.Start);
            previousEnd = region.End;
        }

        foreach (var region in _regions)
        {
            if (region.Touched)
            {
                Reconcile(region);
            }
        }
        Version++;
        return [change];
    }

    /// <summary>Zero-based line holding <paramref name="offset"/>: the count of newlines before it.</summary>
    public int LineOfOffset(int offset)
    {
        ArgumentOutOfRangeException.ThrowIfNegative(offset);
        ArgumentOutOfRangeException.ThrowIfGreaterThan(offset, _builder.Length);
        return Text.AsSpan(0, offset).Count('\n');
    }

    internal static byte[] Hash(string text) => SHA256.HashData(MemoryMarshal.AsBytes(text.AsSpan()));

    private static int MapStart(int x, int a, int b, int length) =>
        x < a ? x : x >= b ? x + length - (b - a) : a;

    private static int MapEnd(int x, int a, int b, int length) =>
        x < a ? x : x >= b ? x + length - (b - a) : a + length;

    private void Reconcile(RegionState region)
    {
        var matches = Hash(Text.Substring(region.Start, region.End - region.Start)).AsSpan().SequenceEqual(region.ExpectedHash);
        if (matches)
        {
            region.Choice = region.StoredChoice;
            region.Acknowledged = false;
        }
        else if (region.Choice != ResolutionChoice.Manual)
        {
            region.Choice = ResolutionChoice.Manual;
            region.Acknowledged = region.Kind != RegionKind.Conflict;
        }
    }

    private RegionView[] BuildViews()
    {
        var text = Text;
        var views = new RegionView[_regions.Length];
        var lineStarts = new List<int> { 0 };
        for (var index = text.IndexOf('\n'); index >= 0; index = text.IndexOf('\n', index + 1))
        {
            lineStarts.Add(index + 1);
        }

        // The line of an offset is the number of newlines strictly before it.
        int LineAt(int offset)
        {
            var found = lineStarts.BinarySearch(offset);
            return found >= 0 ? found : ~found - 1;
        }

        for (var index = 0; index < views.Length; index++)
        {
            var region = _regions[index];
            var firstLine = LineAt(region.Start);
            var endLine = region.End == region.Start ? firstLine : LineAt(region.End - 1) + 1;
            views[index] = new RegionView(region.Id, region.Kind, region.Start, region.End, new LineRange(firstLine, endLine), region.Choice, region.Acknowledged);
        }
        return views;
    }

    /// <summary>Mutable per-region storage. Task 5 extends this with transition records for undo.</summary>
    private sealed class RegionState(int id, RegionKind kind)
    {
        public int Id { get; } = id;

        public RegionKind Kind { get; } = kind;

        public int Start { get; set; }

        public int End { get; set; }

        public ResolutionChoice Choice { get; set; }

        public bool Acknowledged { get; set; }

        /// <summary>Hash of the text this region's last action produced.</summary>
        public byte[] ExpectedHash { get; set; } = [];

        /// <summary>The choice to return to when the text matches <see cref="ExpectedHash"/> again.</summary>
        public ResolutionChoice StoredChoice { get; set; }

        public bool Touched { get; set; }
    }
}
