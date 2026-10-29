using System.Text;
using CsCheck;
using Resolve.Core.Text;

namespace Resolve.Core.Tests;

public class TextCodecTests
{
    [Theory]
    [InlineData("a\nb\n")]
    [InlineData("a\r\nb\r\n")]
    [InlineData("mixed\r\nends\nhere")]
    [InlineData("lone\rcr\n")]
    [InlineData("")]
    [InlineData("no final newline")]
    public void Utf8_round_trips_byte_for_byte(string text)
    {
        var bytes = Encoding.UTF8.GetBytes(text);
        var decoded = Assert.IsType<DecodeOutcome.Text>(TextCodec.Decode(bytes)).Value;
        Assert.Equal(bytes, decoded.Encode());
    }

    [Fact]
    public void Lines_split_on_lf_only_like_git()
    {
        var lines = TextCodec.SplitLines("a\rb\r\nc\nd");
        Line[] expected = [new("a\rb", LineEnding.CrLf), new("c", LineEnding.Lf), new("d", LineEnding.None)];
        Assert.Equal(expected.Length, lines.Length);
        for (var index = 0; index < expected.Length; index++)
        {
            Assert.Equal(expected[index].Content, lines[index].Content);
            Assert.Equal(expected[index].Ending, lines[index].Ending);
        }
    }

    [Fact]
    public void Bom_kinds_are_kept()
    {
        var utf8Bom = new byte[] { 0xEF, 0xBB, 0xBF, (byte)'x', (byte)'\n' };
        Assert.Equal(TextEncodingKind.Utf8Bom, Assert.IsType<DecodeOutcome.Text>(TextCodec.Decode(utf8Bom)).Value.Encoding);

        var utf16 = new byte[] { 0xFF, 0xFE }.Concat(Encoding.Unicode.GetBytes("hé\r\n")).ToArray();
        var decoded = Assert.IsType<DecodeOutcome.Text>(TextCodec.Decode(utf16)).Value;
        Assert.Equal(TextEncodingKind.Utf16LeBom, decoded.Encoding);
        Assert.Equal(utf16, decoded.Encode());
    }

    [Fact]
    public void Nul_without_bom_is_binary() =>
        Assert.IsType<DecodeOutcome.Binary>(TextCodec.Decode([(byte)'a', 0, (byte)'b']));

    [Fact]
    public void Invalid_utf8_is_refused_not_replaced() =>
        Assert.IsType<DecodeOutcome.Undecodable>(TextCodec.Decode([(byte)'a', 0xC3, (byte)'(']));

    [Fact]
    public void Any_lines_round_trip_through_bytes()
    {
        var line = Gen.Select(
            Gen.String[Gen.Char.Where(c => c != '\n' && !char.IsSurrogate(c)), 0, 12],
            Gen.Enum<LineEnding>(),
            (content, ending) => new Line(content, ending));
        var lines = line.Array[0, 20].Select(array =>
        {
            // Only the last line may lack a terminator.
            for (var index = 0; index < array.Length - 1; index++)
            {
                if (array[index].Ending == LineEnding.None)
                {
                    array[index] = array[index] with { Ending = LineEnding.Lf };
                }
            }
            // "x\r" + LF reads back as CRLF: a content-final CR before LF is not representable separately.
            return array.Where(l => !(l.Content.EndsWith('\r') && l.Ending == LineEnding.Lf)).ToArray();
        });
        lines.Sample(sample =>
        {
            var bytes = TextCodec.Encode(TextEncodingKind.Utf8, sample);
            var decoded = Assert.IsType<DecodeOutcome.Text>(TextCodec.Decode(bytes)).Value;
            return decoded.Lines.SequenceEqual(sample.Where((l, index) => !(l.Content.Length == 0 && l.Ending == LineEnding.None && index == sample.Length - 1)));
        });
    }
}
