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

    [Fact]
    public void Typing_at_the_start_of_a_conflict_goes_into_the_conflict()
    {
        var doc = Criterion1();
        var conflict = ConflictOf(doc);
        var before = doc.Regions[2];
        var beforeText = doc.Text[before.Start..before.End];
        doc.Edit(new TextChange(conflict.Start, 0, "x"));
        Assert.Equal(beforeText, doc.Text[doc.Regions[2].Start..doc.Regions[2].End]);
        Assert.Equal(ResolutionChoice.Base, doc.Regions[2].Choice);
        var after = ConflictOf(doc);
        Assert.StartsWith("xline 20", doc.Text[after.Start..after.End]);
        Assert.Equal(ResolutionChoice.Manual, after.Choice);
        Assert.False(after.Acknowledged);
    }

    [Fact]
    public void Typing_at_the_end_of_a_conflict_goes_into_the_conflict()
    {
        var doc = Criterion1();
        var conflict = ConflictOf(doc);
        doc.Edit(new TextChange(conflict.End, 0, "x"));
        var after = ConflictOf(doc);
        Assert.Equal("line 20\nx", doc.Text[after.Start..after.End]);
        Assert.Equal(ResolutionChoice.Manual, after.Choice);
        Assert.Equal(ResolutionChoice.Base, doc.Regions[4].Choice);
        Assert.Equal(ResolutionChoice.Base, doc.Regions[2].Choice);
    }

    [Fact]
    public void Text_typed_into_an_emptied_conflict_refills_it()
    {
        var doc = Criterion1();
        var conflict = ConflictOf(doc);
        doc.Edit(new TextChange(conflict.Start, conflict.End - conflict.Start, ""));
        var neighbours = new[] { doc.Regions[2], doc.Regions[4] }.Select(view => doc.Text[view.Start..view.End]).ToArray();
        doc.Edit(new TextChange(conflict.Start, 0, "typed\n"));
        var after = ConflictOf(doc);
        Assert.Equal("typed\n", doc.Text[after.Start..after.End]);
        Assert.Equal(neighbours, new[] { doc.Regions[2], doc.Regions[4] }.Select(view => doc.Text[view.Start..view.End]));
        Assert.Equal(ResolutionChoice.Base, doc.Regions[2].Choice);
        Assert.Equal(ResolutionChoice.Base, doc.Regions[4].Choice);
        Assert.Equal(ResolutionChoice.Manual, after.Choice);
    }

    [Fact]
    public void A_boundary_insertion_between_an_equal_and_a_change_goes_to_the_change()
    {
        // Two Equal regions are never adjacent in a plan (rule 4 is unreachable), so this pins rule 2 both ways.
        var doc = Criterion1();
        var change = doc.Regions[1];
        doc.Edit(new TextChange(change.Start, 0, "x"));
        Assert.Equal(ResolutionChoice.Base, doc.Regions[0].Choice);
        Assert.Equal(ResolutionChoice.Manual, doc.Regions[1].Choice);
        Assert.StartsWith("xleft 3", doc.Text[doc.Regions[1].Start..doc.Regions[1].End]);
        doc.Edit(new TextChange(doc.Regions[1].End, 0, "y"));
        Assert.Equal(ResolutionChoice.Base, doc.Regions[2].Choice);
        Assert.EndsWith("y", doc.Text[doc.Regions[1].Start..doc.Regions[1].End]);
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

    [Fact]
    public void Replacing_a_conflicts_whole_text_from_its_first_character_keeps_it_in_the_conflict()   // ruling R31
    {
        var doc = Criterion1();
        var conflict = ConflictOf(doc);
        var before = doc.Regions[2];
        var beforeText = doc.Text[before.Start..before.End];

        doc.Edit(new TextChange(conflict.Start, conflict.End - conflict.Start, "mine\n"));

        var after = ConflictOf(doc);
        Assert.Equal("mine\n", doc.Text[after.Start..after.End]);
        Assert.Equal(ResolutionChoice.Manual, after.Choice);
        Assert.False(after.Acknowledged);
        Assert.Equal(beforeText, doc.Text[doc.Regions[2].Start..doc.Regions[2].End]);
        Assert.Equal(ResolutionChoice.Base, doc.Regions[2].Choice);
    }

    [Fact]
    public void Accepting_left_over_a_whole_text_replacement_replaces_exactly_that_text()   // ruling R31
    {
        var expected = Criterion1();
        expected.Apply(new RegionCommand.Accept(ConflictOf(expected).RegionId, ResolutionChoice.Left));
        var doc = Criterion1();
        var conflict = ConflictOf(doc);
        doc.Edit(new TextChange(conflict.Start, conflict.End - conflict.Start, "mine\n"));

        Assert.IsType<CommandOutcome.NeedsConfirmation>(doc.Apply(new RegionCommand.Accept(conflict.RegionId, ResolutionChoice.Left)));
        Assert.IsType<CommandOutcome.Changed>(doc.Apply(new RegionCommand.Accept(conflict.RegionId, ResolutionChoice.Left), confirmed: true));

        Assert.Equal(expected.Text, doc.Text);
        Assert.DoesNotContain("mine", doc.Text, StringComparison.Ordinal);
        Assert.Equal(ResolutionChoice.Left, ConflictOf(doc).Choice);
    }

    [Fact]
    public void A_replacement_straddling_a_boundary_still_goes_to_the_earlier_region()
    {
        var doc = Criterion1();
        var conflict = ConflictOf(doc);
        // From the middle of the line before the conflict to the middle of the conflict's line.
        doc.Edit(new TextChange(conflict.Start - 2, 4, "Q"));
        Assert.EndsWith("Q", doc.Text[doc.Regions[2].Start..doc.Regions[2].End], StringComparison.Ordinal);
        Assert.Equal(ResolutionChoice.Manual, doc.Regions[2].Choice);
        Assert.Equal(ResolutionChoice.Manual, ConflictOf(doc).Choice);
    }
}
