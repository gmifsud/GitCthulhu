using Resolve.Core.Text;

namespace Resolve.Core.Diff;

/// <summary>
/// Replace a[AStart, AEnd) with b[BStart, BEnd). Pure insertions have AStart == AEnd, pure deletions BStart == BEnd.
/// A diff is a list of these, sorted, maximal, and separated by at least one equal line.
/// </summary>
public readonly record struct Edit(int AStart, int AEnd, int BStart, int BEnd)
{
    public int Delta => (BEnd - BStart) - (AEnd - AStart);
}

public enum DiffAlgorithm
{
    /// <summary>Myers O(ND), linear space (Myers 1986, §4b). Git's default.</summary>
    Myers,

    /// <summary>Histogram (git/JGit): anchors on low-occurrence common lines, Myers where none exist.</summary>
    Histogram,
}

/// <summary>Maps lines to dense integer ids so every diff compares ints. One interner per file triple.</summary>
public sealed class LineInterner
{
    private readonly Dictionary<string, int> _ids = new(StringComparer.Ordinal);

    /// <summary>Exact equality: content and terminator. Whitespace-insensitive views use a different key, never this one.</summary>
    public int[] Intern(IReadOnlyList<Line> lines)
    {
        ArgumentNullException.ThrowIfNull(lines);
        var ids = new int[lines.Count];
        for (var index = 0; index < lines.Count; index++)
        {
            var key = lines[index].ToString();
            if (!_ids.TryGetValue(key, out var id))
            {
                id = _ids.Count;
                _ids[key] = id;
            }
            ids[index] = id;
        }
        return ids;
    }
}

public static class LineDiff
{
    public static IReadOnlyList<Edit> Compute(int[] a, int[] b, DiffAlgorithm algorithm)
    {
        ArgumentNullException.ThrowIfNull(a);
        ArgumentNullException.ThrowIfNull(b);
        var raw = new List<Edit>();
        switch (algorithm)
        {
            case DiffAlgorithm.Myers:
                Myers.Diff(a, 0, a.Length, b, 0, b.Length, raw);
                break;
            case DiffAlgorithm.Histogram:
                Histogram.Diff(a, b, raw);
                break;
            default:
                throw new ArgumentOutOfRangeException(nameof(algorithm));
        }
        return Normalise(raw);
    }

    /// <summary>Sorts and joins touching edits so the result is maximal: equal lines always separate two edits.</summary>
    internal static List<Edit> Normalise(List<Edit> raw)
    {
        raw.Sort((left, right) => left.AStart != right.AStart ? left.AStart.CompareTo(right.AStart) : left.BStart.CompareTo(right.BStart));
        var result = new List<Edit>(raw.Count);
        foreach (var edit in raw)
        {
            if (edit.AStart == edit.AEnd && edit.BStart == edit.BEnd)
            {
                continue;
            }
            if (result.Count > 0 && result[^1].AEnd == edit.AStart && result[^1].BEnd == edit.BStart)
            {
                var previous = result[^1];
                result[^1] = new Edit(previous.AStart, edit.AEnd, previous.BStart, edit.BEnd);
            }
            else
            {
                result.Add(edit);
            }
        }
        return result;
    }

    /// <summary>Applies <paramref name="edits"/> to <paramref name="a"/>; used to verify diffs in tests.</summary>
    public static int[] Apply(int[] a, int[] b, IReadOnlyList<Edit> edits)
    {
        ArgumentNullException.ThrowIfNull(a);
        ArgumentNullException.ThrowIfNull(b);
        ArgumentNullException.ThrowIfNull(edits);
        var output = new List<int>(b.Length);
        var position = 0;
        foreach (var edit in edits)
        {
            output.AddRange(a[position..edit.AStart]);
            output.AddRange(b[edit.BStart..edit.BEnd]);
            position = edit.AEnd;
        }
        output.AddRange(a[position..]);
        return [.. output];
    }
}

/// <summary>
/// Linear-space Myers with the middle-snake bisection. A fixed operation budget, not a clock, bounds
/// pathological inputs, so the same input always yields the same edits on every machine.
/// </summary>
internal static class Myers
{
    /// <summary>Snake-step budget per bisection before giving up and replacing the whole block.</summary>
    internal const long Budget = 1L << 26;

    public static void Diff(int[] a, int aLo, int aHi, int[] b, int bLo, int bHi, List<Edit> output)
    {
        while (aLo < aHi && bLo < bHi && a[aLo] == b[bLo])
        {
            aLo++;
            bLo++;
        }
        while (aLo < aHi && bLo < bHi && a[aHi - 1] == b[bHi - 1])
        {
            aHi--;
            bHi--;
        }
        if (aLo == aHi || bLo == bHi)
        {
            output.Add(new Edit(aLo, aHi, bLo, bHi));
            return;
        }
        var split = Bisect(a, aLo, aHi, b, bLo, bHi);
        if (split is not var (x, y) || (x == aLo && y == bLo) || (x == aHi && y == bHi))
        {
            output.Add(new Edit(aLo, aHi, bLo, bHi));
            return;
        }
        Diff(a, aLo, x, b, bLo, y, output);
        Diff(a, x, aHi, b, y, bHi, output);
    }

    /// <summary>Finds a point on an optimal path roughly halfway through; null if there is no common line or the budget runs out.</summary>
    private static (int X, int Y)? Bisect(int[] a, int aLo, int aHi, int[] b, int bLo, int bHi)
    {
        var n = aHi - aLo;
        var m = bHi - bLo;
        var maxD = (n + m + 1) / 2;
        var offset = maxD;
        var length = 2 * maxD + 2;
        var forward = new int[length];
        var backward = new int[length];
        Array.Fill(forward, -1);
        Array.Fill(backward, -1);
        forward[offset + 1] = 0;
        backward[offset + 1] = 0;
        var delta = n - m;
        var front = (delta & 1) != 0;
        int k1Start = 0, k1End = 0, k2Start = 0, k2End = 0;
        long steps = 0;
        for (var d = 0; d < maxD; d++)
        {
            for (var k1 = -d + k1Start; k1 <= d - k1End; k1 += 2)
            {
                var k1Offset = offset + k1;
                var x1 = k1 == -d || (k1 != d && forward[k1Offset - 1] < forward[k1Offset + 1])
                    ? forward[k1Offset + 1]
                    : forward[k1Offset - 1] + 1;
                var y1 = x1 - k1;
                while (x1 < n && y1 < m && a[aLo + x1] == b[bLo + y1])
                {
                    x1++;
                    y1++;
                }
                steps += 1 + x1;
                forward[k1Offset] = x1;
                if (x1 > n)
                {
                    k1End += 2;
                }
                else if (y1 > m)
                {
                    k1Start += 2;
                }
                else if (front)
                {
                    var k2Offset = offset + delta - k1;
                    if (k2Offset >= 0 && k2Offset < length && backward[k2Offset] != -1 && x1 >= n - backward[k2Offset])
                    {
                        return (aLo + x1, bLo + y1);
                    }
                }
            }
            for (var k2 = -d + k2Start; k2 <= d - k2End; k2 += 2)
            {
                var k2Offset = offset + k2;
                var x2 = k2 == -d || (k2 != d && backward[k2Offset - 1] < backward[k2Offset + 1])
                    ? backward[k2Offset + 1]
                    : backward[k2Offset - 1] + 1;
                var y2 = x2 - k2;
                while (x2 < n && y2 < m && a[aLo + n - x2 - 1] == b[bLo + m - y2 - 1])
                {
                    x2++;
                    y2++;
                }
                backward[k2Offset] = x2;
                if (x2 > n)
                {
                    k2End += 2;
                }
                else if (y2 > m)
                {
                    k2Start += 2;
                }
                else if (!front)
                {
                    var k1Offset = offset + delta - k2;
                    if (k1Offset >= 0 && k1Offset < length && forward[k1Offset] != -1)
                    {
                        var x1 = forward[k1Offset];
                        var y1 = offset + x1 - k1Offset;
                        if (x1 >= n - x2)
                        {
                            return (aLo + x1, bLo + y1);
                        }
                    }
                }
            }
            if (steps > Budget)
            {
                return null;
            }
        }
        return null;
    }
}

/// <summary>
/// Histogram diff: pick the common run whose rarest line occurs least often in A, split around it,
/// repeat. Blocks with no line occurring at most <see cref="MaxChain"/> times fall back to Myers.
/// An explicit work stack keeps deep splits off the call stack.
/// </summary>
internal static class Histogram
{
    internal const int MaxChain = 64;

    public static void Diff(int[] a, int[] b, List<Edit> output)
    {
        var work = new Stack<(int ALo, int AHi, int BLo, int BHi)>();
        work.Push((0, a.Length, 0, b.Length));
        while (work.Count > 0)
        {
            var (aLo, aHi, bLo, bHi) = work.Pop();
            while (aLo < aHi && bLo < bHi && a[aLo] == b[bLo])
            {
                aLo++;
                bLo++;
            }
            while (aLo < aHi && bLo < bHi && a[aHi - 1] == b[bHi - 1])
            {
                aHi--;
                bHi--;
            }
            if (aLo == aHi || bLo == bHi)
            {
                output.Add(new Edit(aLo, aHi, bLo, bHi));
                continue;
            }
            var best = FindRegion(a, aLo, aHi, b, bLo, bHi);
            if (best is not var (aStart, bStart, length))
            {
                Myers.Diff(a, aLo, aHi, b, bLo, bHi, output);
                continue;
            }
            work.Push((aStart + length, aHi, bStart + length, bHi));
            work.Push((aLo, aStart, bLo, bStart));
        }
    }

    private static (int AStart, int BStart, int Length)? FindRegion(int[] a, int aLo, int aHi, int[] b, int bLo, int bHi)
    {
        var counts = new Dictionary<int, int>();
        var positions = new Dictionary<int, List<int>>();
        for (var index = aLo; index < aHi; index++)
        {
            var id = a[index];
            counts[id] = counts.GetValueOrDefault(id) + 1;
            if (!positions.TryGetValue(id, out var list))
            {
                list = [];
                positions[id] = list;
            }
            if (list.Count <= MaxChain)
            {
                list.Add(index);
            }
        }
        (int AStart, int BStart, int Length)? best = null;
        var bestRarity = MaxChain + 1;
        for (var bi = bLo; bi < bHi; bi++)
        {
            if (!positions.TryGetValue(b[bi], out var list))
            {
                continue;
            }
            var startCount = counts[b[bi]];
            if (startCount > MaxChain || startCount > bestRarity)
            {
                continue;
            }
            foreach (var ai in list)
            {
                // Only maximal starts: a run that extends backwards was already seen from an earlier bi.
                if (ai > aLo && bi > bLo && a[ai - 1] == b[bi - 1])
                {
                    continue;
                }
                var length = 0;
                var rarity = startCount;
                while (ai + length < aHi && bi + length < bHi && a[ai + length] == b[bi + length])
                {
                    rarity = Math.Min(rarity, counts[a[ai + length]]);
                    length++;
                }
                if (rarity < bestRarity || (rarity == bestRarity && length > (best?.Length ?? 0)))
                {
                    best = (ai, bi, length);
                    bestRarity = rarity;
                }
            }
        }
        return best;
    }
}
