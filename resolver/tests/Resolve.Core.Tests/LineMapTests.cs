using CsCheck;
using Resolve.Core.Merge;
using Resolve.Core.Resolution;
using static Resolve.Core.Tests.Support.Docs;

namespace Resolve.Core.Tests;

public class LineMapTests
{
    private const double Tolerance = 1e-9;

    private static (LineRange Side, LineRange Result) Block(int sideStart, int sideEnd, int resultStart, int resultEnd) =>
        (new LineRange(sideStart, sideEnd), new LineRange(resultStart, resultEnd));

    private static int LineCount(string text) =>
        text.Length == 0 ? 0 : text.Count(character => character == '\n') + (text.EndsWith('\n') ? 0 : 1);


    /// <summary>A side line past 0 that maps to the pinned result 0, which <c>ToSide</c> maps back to side 0.</summary>
    private static bool IsPinnedEnd(LineMap map, double line) => line > 0 && map.ToResult(line) == 0;

    [Fact]
    public void A_leading_collapsed_block_pins_the_start_so_the_next_block_does_not_round_trip_from_zero()
    {
        // Shrunk case [0,1,0, 0,1,1] from seed 40ybIj7opN64: side [0,1)->result [0,0) then side [1,2)->result [0,1).
        var map = LineMap.Build([Block(0, 1, 0, 0), Block(1, 2, 0, 1)], 2, 1);
        Assert.Equal(0, map.ToResult(0), Tolerance);
        Assert.Equal(0, map.ToResult(1), Tolerance);
        Assert.Equal(1, map.ToResult(2), Tolerance);
        Assert.Equal(0, map.ToSide(0), Tolerance);
        Assert.Equal(1.5, map.ToSide(0.5), Tolerance);
        Assert.Equal(2, map.ToSide(1), Tolerance);
    }
    [Fact]
    public void Equal_runs_map_one_to_one_and_blocks_proportionally()
    {
        var map = LineMap.Build([Block(10, 12, 10, 16)], 30, 34);
        Assert.Equal(5, map.ToResult(5), Tolerance);
        Assert.Equal(13, map.ToResult(11), Tolerance);
        Assert.Equal(18, map.ToResult(14), Tolerance);
        Assert.Equal(11, map.ToSide(13), Tolerance);
        Assert.Equal(12, map.ToSide(16), Tolerance);
        Assert.Equal(11.5, map.ToSide(14.5), Tolerance);
    }

    [Fact]
    public void Inputs_outside_the_range_clamp_to_the_ends()
    {
        var map = LineMap.Build([Block(10, 12, 10, 16)], 30, 34);
        Assert.Equal(0, map.ToResult(-3), Tolerance);
        Assert.Equal(34, map.ToResult(99), Tolerance);
        Assert.Equal(0, map.ToSide(-1), Tolerance);
        Assert.Equal(30, map.ToSide(100), Tolerance);
    }

    [Fact]
    public void The_map_is_monotone_and_covers_both_ends()
    {
        // Each triple: an equal run length, then a block's side and result lengths.
        Gen.Int[0, 4].Array[0, 30].Sample(numbers =>
        {
            var blocks = new List<(LineRange Side, LineRange Result)>();
            var equalRuns = new List<(int Start, int End)>();
            int side = 0, result = 0;
            for (var index = 0; index + 2 < numbers.Length; index += 3)
            {
                var equal = numbers[index];
                equalRuns.Add((side, side + equal));
                side += equal;
                result += equal;
                if (numbers[index + 1] + numbers[index + 2] == 0)
                {
                    continue;
                }
                blocks.Add(Block(side, side + numbers[index + 1], result, result + numbers[index + 2]));
                side += numbers[index + 1];
                result += numbers[index + 2];
            }
            var tail = numbers.Length > 0 ? numbers[0] : 0;
            equalRuns.Add((side, side + tail));
            var sideLines = side + tail;
            var resultLines = result + tail;

            var map = LineMap.Build(blocks, sideLines, resultLines);
            Assert.Equal(blocks, map.Blocks);
            Assert.Equal(sideLines == 0 ? resultLines : 0, map.ToResult(0), Tolerance);
            Assert.Equal(resultLines, map.ToResult(sideLines), Tolerance);
            Assert.Equal(resultLines == 0 ? sideLines : 0, map.ToSide(0), Tolerance);
            Assert.Equal(sideLines, map.ToSide(resultLines), Tolerance);

            var previous = double.NegativeInfinity;
            for (var step = 0; step <= sideLines * 4; step++)
            {
                var value = map.ToResult(step / 4.0);
                Assert.True(value >= previous, $"ToResult not monotone at {step / 4.0}");
                previous = value;
            }
            previous = double.NegativeInfinity;
            for (var step = 0; step <= resultLines * 4; step++)
            {
                var value = map.ToSide(step / 4.0);
                Assert.True(value >= previous, $"ToSide not monotone at {step / 4.0}");
                previous = value;
            }
            foreach (var (start, end) in equalRuns)
            {
                for (var step = start * 4; step < end * 4; step++)
                {
                    var line = step / 4.0;
                    if (!IsPinnedEnd(map, line))   // the ends of each pane are pinned to the ends of the other
                    {
                        Assert.Equal(line, map.ToSide(map.ToResult(line)), Tolerance);
                    }
                }
            }
            foreach (var (blockSide, _) in blocks.Where(block => block.Side.Length > 0 && block.Result.Length > 0))
            {
                for (var step = 0; step < 4; step++)
                {
                    var line = blockSide.Start + (blockSide.Length * step / 4.0);
                    if (!IsPinnedEnd(map, line))
                    {
                        Assert.Equal(line, map.ToSide(map.ToResult(line)), Tolerance);
                    }
                }
            }
        });
    }

    [Fact]
    public void An_empty_side_range_collapses_to_a_point()
    {
        var map = LineMap.Build([Block(10, 10, 10, 14)], 20, 24);
        Assert.Equal(10, map.ToSide(10), Tolerance);
        Assert.Equal(10, map.ToSide(12), Tolerance);
        Assert.Equal(10, map.ToSide(13.99), Tolerance);
        Assert.Equal(24, map.ToResult(20), Tolerance);
        Assert.Equal(18, map.ToResult(14), Tolerance);
        Assert.Equal(9, map.ToResult(9), Tolerance);
    }

    [Fact]
    public void An_empty_result_range_collapses_to_a_point()
    {
        var map = LineMap.Build([Block(10, 14, 10, 10)], 20, 16);
        Assert.Equal(10, map.ToResult(10), Tolerance);
        Assert.Equal(10, map.ToResult(12), Tolerance);
        Assert.Equal(16, map.ToResult(20), Tolerance);
        Assert.Equal(18, map.ToSide(14), Tolerance);
    }

    [Fact]
    public void Build_rejects_blocks_that_are_out_of_order_overlapping_or_out_of_bounds()
    {
        Assert.Throws<ArgumentException>(() => LineMap.Build([Block(10, 12, 10, 12), Block(5, 6, 5, 6)], 30, 30));
        Assert.Throws<ArgumentException>(() => LineMap.Build([Block(10, 12, 10, 12), Block(11, 13, 14, 15)], 30, 30));
        Assert.Throws<ArgumentException>(() => LineMap.Build([Block(10, 12, 10, 12), Block(12, 14, 11, 14)], 30, 30));
        Assert.Throws<ArgumentException>(() => LineMap.Build([Block(10, 40, 10, 12)], 30, 30));
        Assert.Throws<ArgumentException>(() => LineMap.Build([Block(10, 12, 10, 40)], 30, 30));
    }

    [Fact]
    public void Pane_maps_pair_each_side_with_the_result_through_the_plan()
    {
        var doc = Criterion1();
        var (left, right) = PaneMaps.For(doc.Plan, doc);
        var resultLines = LineCount(doc.Text);
        Assert.Equal(0, left.ToResult(0), Tolerance);
        Assert.Equal(resultLines, left.ToResult(50), Tolerance);
        Assert.Equal(resultLines, right.ToResult(50), Tolerance);
        var changed = doc.Plan.Regions.Count(region => region.Kind != RegionKind.Equal);
        Assert.Equal(changed, left.Blocks.Count);
        Assert.Equal(changed, right.Blocks.Count);
    }

    [Fact]
    public void Maps_follow_the_document_after_an_edit()
    {
        var doc = Criterion1();
        var conflict = doc.Regions.Single(view => view.Kind == RegionKind.Conflict);
        var before = PaneMaps.For(doc.Plan, doc);
        var planned = doc.Plan.Regions.Single(region => region.Id == conflict.RegionId);

        doc.Edit(new TextChange(conflict.Start, 0, "x\ny\nz\n"));
        var after = PaneMaps.For(doc.Plan, doc);

        var beforeBlock = before.Left.Blocks.Single(block => block.Side == planned.Left);
        var afterBlock = after.Left.Blocks.Single(block => block.Side == planned.Left);
        Assert.Equal(beforeBlock.Result.Start, afterBlock.Result.Start);
        Assert.Equal(beforeBlock.Result.End + 3, afterBlock.Result.End);
        var rightBlock = after.Right.Blocks.Single(block => block.Side == planned.Right);
        Assert.Equal(afterBlock.Result, rightBlock.Result);
        Assert.Equal(before.Left.ToResult(30) + 3, after.Left.ToResult(30), Tolerance);
    }

    [Fact]
    public void An_edited_equal_region_becomes_a_block_and_a_final_line_without_terminator_counts()
    {
        var doc = NoFinalNewline();
        doc.Edit(new TextChange(doc.Text.Length - 1, 1, ""));
        Assert.False(doc.Text.EndsWith('\n'));
        var untouched = PaneMaps.For(doc.Plan, doc).Left;
        Assert.Equal(LineCount(doc.Text), untouched.ToResult(3), Tolerance);

        doc.Edit(new TextChange(0, 0, "new\n"));
        var edited = PaneMaps.For(doc.Plan, doc).Left;
        Assert.Equal(LineCount(doc.Text), edited.ToResult(3), Tolerance);
        var block = Assert.Single(edited.Blocks, candidate => candidate.Side.Start == 0);
        Assert.Equal(block.Side.Length + 1, block.Result.Length);
    }
}
