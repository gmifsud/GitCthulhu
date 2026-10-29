using System.Collections.Immutable;
using System.Text;

namespace Resolve.Core.Text;

/// <summary>
/// How a line ends. Lines split on LF only, as git's xdiff does: a lone CR is content, so the line model
/// matches what git merges (and the oracle in the tests).
/// </summary>
public enum LineEnding : byte
{
    /// <summary>Last line of a file without a final newline.</summary>
    None,
    Lf,
    CrLf,
}

/// <summary>One line: its text without the terminator, plus the terminator it had.</summary>
public readonly record struct Line(string Content, LineEnding Ending)
{
    public string Terminator => Ending switch
    {
        LineEnding.Lf => "\n",
        LineEnding.CrLf => "\r\n",
        _ => "",
    };

    public override string ToString() => Content + Terminator;
}

public enum TextEncodingKind
{
    Utf8,
    Utf8Bom,
    Utf16LeBom,
    Utf16BeBom,
    Utf32LeBom,
    Utf32BeBom,
}

/// <summary>Decoded text that re-encodes to exactly the bytes it came from (checked at decode time).</summary>
public sealed record DecodedText(TextEncodingKind Encoding, ImmutableArray<Line> Lines)
{
    public bool HasFinalNewline => Lines.IsEmpty || Lines[^1].Ending != LineEnding.None;

    public (int Lf, int CrLf) EndingCounts =>
        (Lines.Count(line => line.Ending == LineEnding.Lf), Lines.Count(line => line.Ending == LineEnding.CrLf));

    public byte[] Encode() => TextCodec.Encode(Encoding, Lines);
}

public abstract record DecodeOutcome
{
    private DecodeOutcome() { }

    public sealed record Text(DecodedText Value) : DecodeOutcome;

    /// <summary>NUL in the first 8000 bytes with no BOM — git's own binary heuristic.</summary>
    public sealed record Binary : DecodeOutcome;

    /// <summary>Not valid in the detected encoding, or does not round-trip. Never decoded lossily.</summary>
    public sealed record Undecodable(string Reason) : DecodeOutcome;
}

public static class TextCodec
{
    /// <summary>git checks this many leading bytes for NUL (xdiff-interface.c, FIRST_FEW_BYTES).</summary>
    public const int BinarySniffLength = 8000;

    private static readonly UTF8Encoding StrictUtf8 = new(encoderShouldEmitUTF8Identifier: false, throwOnInvalidBytes: true);

    public static DecodeOutcome Decode(ReadOnlySpan<byte> bytes)
    {
        var (kind, bomLength) = DetectBom(bytes);
        if (kind == TextEncodingKind.Utf8 && bytes[..Math.Min(bytes.Length, BinarySniffLength)].Contains((byte)0))
        {
            return new DecodeOutcome.Binary();
        }
        string text;
        try
        {
            text = EncodingFor(kind).GetString(bytes[bomLength..]);
        }
        catch (DecoderFallbackException error)
        {
            return new DecodeOutcome.Undecodable($"{kind}: invalid byte sequence at {error.Index}.");
        }
        var decoded = new DecodedText(kind, SplitLines(text));
        // A decode that would not write back the same bytes is a decode we must not offer for editing.
        return decoded.Encode().AsSpan().SequenceEqual(bytes)
            ? new DecodeOutcome.Text(decoded)
            : new DecodeOutcome.Undecodable($"{kind}: content does not round-trip.");
    }

    public static ImmutableArray<Line> SplitLines(string text)
    {
        ArgumentNullException.ThrowIfNull(text);
        var lines = ImmutableArray.CreateBuilder<Line>();
        var start = 0;
        while (start < text.Length)
        {
            var newline = text.IndexOf('\n', start);
            if (newline < 0)
            {
                lines.Add(new Line(text[start..], LineEnding.None));
                break;
            }
            var crlf = newline > start && text[newline - 1] == '\r';
            lines.Add(new Line(text[start..(crlf ? newline - 1 : newline)], crlf ? LineEnding.CrLf : LineEnding.Lf));
            start = newline + 1;
        }
        return lines.ToImmutable();
    }

    public static byte[] Encode(TextEncodingKind kind, IEnumerable<Line> lines)
    {
        ArgumentNullException.ThrowIfNull(lines);
        var builder = new StringBuilder();
        foreach (var line in lines)
        {
            builder.Append(line.Content).Append(line.Terminator);
        }
        var encoding = EncodingFor(kind);
        var preamble = Preamble(kind);
        var body = encoding.GetBytes(builder.ToString());
        var result = new byte[preamble.Length + body.Length];
        preamble.CopyTo(result, 0);
        body.CopyTo(result, preamble.Length);
        return result;
    }

    private static (TextEncodingKind Kind, int BomLength) DetectBom(ReadOnlySpan<byte> bytes) => bytes switch
    {
        [0xFF, 0xFE, 0x00, 0x00, ..] => (TextEncodingKind.Utf32LeBom, 4),
        [0x00, 0x00, 0xFE, 0xFF, ..] => (TextEncodingKind.Utf32BeBom, 4),
        [0xEF, 0xBB, 0xBF, ..] => (TextEncodingKind.Utf8Bom, 3),
        [0xFF, 0xFE, ..] => (TextEncodingKind.Utf16LeBom, 2),
        [0xFE, 0xFF, ..] => (TextEncodingKind.Utf16BeBom, 2),
        _ => (TextEncodingKind.Utf8, 0),
    };

    private static Encoding EncodingFor(TextEncodingKind kind) => kind switch
    {
        TextEncodingKind.Utf8 or TextEncodingKind.Utf8Bom => StrictUtf8,
        TextEncodingKind.Utf16LeBom => new UnicodeEncoding(bigEndian: false, byteOrderMark: false, throwOnInvalidBytes: true),
        TextEncodingKind.Utf16BeBom => new UnicodeEncoding(bigEndian: true, byteOrderMark: false, throwOnInvalidBytes: true),
        TextEncodingKind.Utf32LeBom => new UTF32Encoding(bigEndian: false, byteOrderMark: false, throwOnInvalidCharacters: true),
        TextEncodingKind.Utf32BeBom => new UTF32Encoding(bigEndian: true, byteOrderMark: false, throwOnInvalidCharacters: true),
        _ => throw new ArgumentOutOfRangeException(nameof(kind)),
    };

    private static byte[] Preamble(TextEncodingKind kind) => kind switch
    {
        TextEncodingKind.Utf8 => [],
        TextEncodingKind.Utf8Bom => [0xEF, 0xBB, 0xBF],
        TextEncodingKind.Utf16LeBom => [0xFF, 0xFE],
        TextEncodingKind.Utf16BeBom => [0xFE, 0xFF],
        TextEncodingKind.Utf32LeBom => [0xFF, 0xFE, 0x00, 0x00],
        TextEncodingKind.Utf32BeBom => [0x00, 0x00, 0xFE, 0xFF],
        _ => throw new ArgumentOutOfRangeException(nameof(kind)),
    };
}
