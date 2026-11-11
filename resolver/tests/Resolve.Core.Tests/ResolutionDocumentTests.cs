using CsCheck;
using Resolve.Core.Merge;
using Resolve.Core.Resolution;
using static Resolve.Core.Tests.Support.Docs;

namespace Resolve.Core.Tests;

public class ResolutionDocumentTests
{
    private static RegionView ConflictOf(ResolutionDocument doc) => doc.Regions.Single(view => view.Kind == RegionKind.Conflict);

    [Fact]
    public void The_initial_result_applies_non_conflicting_changes_and_keeps_base_in_conflicts()
    {
        var doc = Criterion1();
        Assert.Equal(new DocumentCounts(1, 0, 2), doc.Counts);
        Assert.Equal("left 3", Lines(doc.Text)[2]);
        Assert.Equal("right 40", Lines(doc.Text)[39]);
        Assert.Equal("line 20", Lines(doc.Text)[19]);
        Assert.False(doc.IsComplete);
        Assert.Equal(0, doc.Version);
    }

    [Fact]
    public void Regions_cover_every_plan_region_in_order_with_their_initial_choices()
    {
        var doc = Criterion1();
        Assert.Equal([RegionKind.Equal, RegionKind.LeftOnly, RegionKind.Equal, RegionKind.Conflict, RegionKind.Equal, RegionKind.RightOnly, RegionKind.Equal],
            doc.Regions.Select(view => view.Kind));
        Assert.Equal([0, 1, 2, 3, 4, 5, 6], doc.Regions.Select(view => view.RegionId));
        Assert.Equal(ResolutionChoice.Left, doc.Regions[1].Choice);
        Assert.Equal(ResolutionChoice.Unresolved, ConflictOf(doc).Choice);
        Assert.Equal(ResolutionChoice.Right, doc.Regions[5].Choice);
    }

    [Fact]
    public void Result_lines_locate_each_region_in_the_text()
    {
        var doc = Criterion1();
        Assert.Equal(new LineRange(2, 3), doc.Regions[1].ResultLines);
        Assert.Equal(new LineRange(19, 20), ConflictOf(doc).ResultLines);
        Assert.Equal(new LineRange(39, 40), doc.Regions[5].ResultLines);
        Assert.Equal(doc.Text.Length, doc.Regions[^1].End);
    }

    [Fact]
    public void An_empty_region_has_an_empty_range_at_the_line_where_it_sits()
    {
        var doc = Criterion1();
        var conflict = ConflictOf(doc);
        doc.Edit(new TextChange(conflict.Start, conflict.End - conflict.Start, ""));
        Assert.Equal(new LineRange(19, 19), ConflictOf(doc).ResultLines);
        Assert.Equal(19, doc.Regions[4].ResultLines.Start);
    }

    [Fact]
    public void A_line_of_offset_counts_newlines_before_it()
    {
        var doc = Build(Lf("a", "b"), Lf("a", "b"), Lf("a", "b"));
        Assert.Equal(0, doc.LineOfOffset(0));
        Assert.Equal(0, doc.LineOfOffset(1));
        Assert.Equal(1, doc.LineOfOffset(2));
        Assert.Equal(2, doc.LineOfOffset(4));
    }

    [Fact]
    public void A_missing_final_newline_on_a_side_is_not_in_the_initial_result()
    {
        var doc = NoFinalNewline();
        Assert.Equal("a\nb\nc\n", doc.Text);
        Assert.Equal(new DocumentCounts(1, 0, 0), doc.Counts);
    }

    [Fact]
    public void Typing_inside_a_conflict_makes_it_manual_and_unacknowledged()
    {
        var doc = Criterion1();
        var conflict = ConflictOf(doc);
        var change = new TextChange(conflict.Start + 1, 0, "x");
        Assert.Equal([change], doc.Edit(change));
        var after = ConflictOf(doc);
        Assert.Equal(ResolutionChoice.Manual, after.Choice);
        Assert.False(after.Acknowledged);
        Assert.Equal(new DocumentCounts(0, 1, 2), doc.Counts);
        Assert.False(doc.IsComplete);
        Assert.Equal(1, doc.Version);
        Assert.Equal("lxine 20", Lines(doc.Text)[19]);
    }

    [Fact]
    public void Editing_a_clean_change_makes_it_manual_and_acknowledged()
    {
        var doc = Criterion1();
        var region = doc.Regions[1];
        doc.Edit(new TextChange(region.Start + 1, 0, "x"));
        Assert.Equal(ResolutionChoice.Manual, doc.Regions[1].Choice);
        Assert.True(doc.Regions[1].Acknowledged);
        Assert.Equal(new DocumentCounts(1, 0, 2), doc.Counts);
    }

    [Fact]
    public void An_edit_straddling_two_regions_marks_both_manual()
    {
        var doc = Criterion1();
        var conflict = ConflictOf(doc);
        doc.Edit(new TextChange(conflict.Start - 2, 4, "zz"));
        Assert.Equal([2, 3], doc.Regions.Where(view => view.Choice == ResolutionChoice.Manual).Select(view => view.RegionId));
    }

    [Fact]
    public void Editing_back_to_the_produced_text_restores_the_choice()
    {
        var doc = Criterion1();
        var conflict = ConflictOf(doc);
        doc.Edit(new TextChange(conflict.Start + 1, 0, "x"));
        doc.Edit(new TextChange(conflict.Start + 1, 1, ""));
        Assert.Equal(ResolutionChoice.Unresolved, ConflictOf(doc).Choice);
        Assert.Equal(new DocumentCounts(1, 0, 2), doc.Counts);
        Assert.Equal(2, doc.Version);
    }

    [Fact]
    public void Deleting_a_conflicts_text_leaves_it_manual_not_resolved()
    {
        var doc = Criterion1();
        var conflict = ConflictOf(doc);
        doc.Edit(new TextChange(conflict.Start, conflict.End - conflict.Start, ""));
        var after = ConflictOf(doc);
        Assert.Equal(ResolutionChoice.Manual, after.Choice);
        Assert.False(after.Acknowledged);
        Assert.Equal(after.Start, after.End);
        Assert.Equal(new DocumentCounts(0, 1, 2), doc.Counts);
        Assert.Equal(ResolutionChoice.Base, doc.Regions[2].Choice);
        Assert.Equal(ResolutionChoice.Base, doc.Regions[4].Choice);
    }

    [Fact]
    public void An_edit_outside_a_region_leaves_it_untouched_but_shifts_its_anchors()
    {
        var doc = Criterion1();
        var conflict = ConflictOf(doc);
        doc.Edit(new TextChange(0, 0, "abc"));
        Assert.Equal(conflict.Start + 3, ConflictOf(doc).Start);
        Assert.Equal(ResolutionChoice.Unresolved, ConflictOf(doc).Choice);
    }

    [Theory]
    [InlineData(-1, 0)]
    [InlineData(0, -1)]
    [InlineData(1000000, 0)]
    [InlineData(0, 1000000)]
    public void An_edit_outside_the_text_is_a_programming_error(int offset, int removed) =>
        Assert.Throws<ArgumentOutOfRangeException>(() => Criterion1().Edit(new TextChange(offset, removed, "")));

    [Fact]
    public void Anchors_stay_ordered_under_random_edits() =>
        Gen.Int[0, 400].SelectMany(o => Gen.Int[0, 20].Select(r => (o, r))).Array[1, 40].Sample(edits =>
        {
            var doc = Criterion1();
            foreach (var (o, r) in edits)
            {
                var at = o % (doc.Text.Length + 1);
                doc.Edit(new TextChange(at, Math.Min(r, doc.Text.Length - at), "x\n"));
            }
            var views = doc.Regions;
            Assert.All(views, view => Assert.True(view.Start <= view.End));
            Assert.Equal(0, views[0].Start);
            Assert.Equal(doc.Text.Length, views[^1].End);
            Assert.True(views.Zip(views.Skip(1)).All(pair => pair.First.End == pair.Second.Start));
        });
}
