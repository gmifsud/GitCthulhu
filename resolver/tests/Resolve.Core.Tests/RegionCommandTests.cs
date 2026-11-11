using CsCheck;
using Microsoft.Extensions.Time.Testing;
using Resolve.Core.Merge;
using Resolve.Core.Resolution;
using Resolve.Core.Text;
using static Resolve.Core.Tests.Support.Docs;

namespace Resolve.Core.Tests;

public class RegionCommandTests
{
    private static RegionView Conflict(ResolutionDocument doc) => doc.Regions.Single(view => view.Kind == RegionKind.Conflict);

    private static string TextOf(ResolutionDocument doc, RegionView view) => doc.Text[view.Start..view.End];

    private static Line[] Crlf(params string[] contents) => [.. contents.Select(content => new Line(content, LineEnding.CrLf))];

    [Fact]
    public void Accept_left_resolves_the_conflict_and_completes_the_document()
    {
        var doc = Criterion1();
        var id = Conflict(doc).RegionId;
        Assert.IsType<CommandOutcome.Changed>(doc.Apply(new RegionCommand.Accept(id, ResolutionChoice.Left)));
        Assert.Equal("left 20", Lines(doc.Text)[19]);
        Assert.True(doc.IsComplete);
        Assert.Equal(1, doc.Version);
    }

    [Fact]
    public void A_command_replaces_the_region_range_and_updates_its_anchors_and_neighbours()
    {
        var doc = Criterion1();
        var conflict = Conflict(doc);
        var outcome = Assert.IsType<CommandOutcome.Changed>(doc.Apply(new RegionCommand.Accept(conflict.RegionId, ResolutionChoice.Right)));
        var change = Assert.Single(outcome.Changes);
        Assert.Equal(new TextChange(conflict.Start, conflict.End - conflict.Start, "right 20\n"), change);
        Assert.Empty(outcome.SkippedManual);
        var after = Conflict(doc);
        Assert.Equal(conflict.Start, after.Start);
        Assert.Equal(conflict.Start + "right 20\n".Length, after.End);
        Assert.Equal(doc.Text.Length, doc.Regions[^1].End);
        Assert.Equal(after.End, doc.Regions[after.RegionId + 1].Start);
    }

    [Fact]
    public void Base_restores_the_base_lines_into_a_conflict()
    {
        var doc = Criterion1();
        var id = Conflict(doc).RegionId;
        doc.Apply(new RegionCommand.Accept(id, ResolutionChoice.Left));
        doc.Apply(new RegionCommand.Accept(id, ResolutionChoice.Base));
        Assert.Equal("line 20", Lines(doc.Text)[19]);
        Assert.Equal(ResolutionChoice.Base, Conflict(doc).Choice);
        Assert.True(doc.IsComplete);
    }

    [Fact]
    public void Both_left_then_right_duplicates_only_the_core()
    {
        // Shared first and last lines appear once; the cores follow in left-then-right (or right-then-left) order.
        var doc = Build(Lf("p", "q"), Lf("S", "L1", "L2", "T"), Lf("S", "R1", "T"));
        var id = Conflict(doc).RegionId;
        Assert.Equal(new ConflictTrim(1, 1), doc.Plan.Regions[id].Trim);
        doc.Apply(new RegionCommand.Accept(id, ResolutionChoice.LeftThenRight));
        Assert.Equal("S\nL1\nL2\nR1\nT\n", doc.Text);
        doc.Apply(new RegionCommand.Accept(id, ResolutionChoice.RightThenLeft));
        Assert.Equal("S\nR1\nL1\nL2\nT\n", doc.Text);
        Assert.Equal(ResolutionChoice.RightThenLeft, Conflict(doc).Choice);
    }

    [Fact]
    public void Both_without_shared_lines_concatenates_whole_sides()
    {
        var doc = Build(Lf("x"), Lf("L"), Lf("R"));
        doc.Apply(new RegionCommand.Accept(Conflict(doc).RegionId, ResolutionChoice.LeftThenRight));
        Assert.Equal("L\nR\n", doc.Text);
    }

    [Fact]
    public void Criterion_2_accept_after_manual_edit_asks_and_one_undo_restores_text_and_state()
    {
        var doc = Criterion1();
        var c = Conflict(doc);
        doc.Edit(new TextChange(c.Start, 0, "mine "));
        var manualText = doc.Text;
        var manualRegions = doc.Regions.ToArray();
        var version = doc.Version;
        var asked = Assert.IsType<CommandOutcome.NeedsConfirmation>(doc.Apply(new RegionCommand.Accept(c.RegionId, ResolutionChoice.Left)));
        Assert.Equal(c.RegionId, asked.RegionId);
        Assert.Equal("Replace your manual edit in this region?", asked.Prompt);
        Assert.Equal(manualText, doc.Text);
        Assert.Equal(version, doc.Version);
        doc.Apply(new RegionCommand.Accept(c.RegionId, ResolutionChoice.Left), confirmed: true);
        Assert.Equal(ResolutionChoice.Left, Conflict(doc).Choice);
        doc.Undo();
        Assert.Equal(manualText, doc.Text);
        Assert.Equal(ResolutionChoice.Manual, Conflict(doc).Choice);
        Assert.Equal(manualRegions, doc.Regions);
    }

    [Fact]
    public void Accept_on_an_equal_region_changes_nothing()
    {
        var doc = Criterion1();
        Assert.IsType<CommandOutcome.NoChange>(doc.Apply(new RegionCommand.Accept(0, ResolutionChoice.Left)));
        Assert.Equal(0, doc.Version);
        Assert.Null(doc.Undo());
    }

    [Fact]
    public void Accept_works_on_a_non_conflicting_region_too()
    {
        var doc = Criterion1();
        Assert.IsType<CommandOutcome.Changed>(doc.Apply(new RegionCommand.Accept(1, ResolutionChoice.Base)));
        Assert.Equal("line 3", Lines(doc.Text)[2]);
        Assert.Equal(ResolutionChoice.Base, doc.Regions[1].Choice);
    }

    [Fact]
    public void Accept_rejects_choices_that_are_not_commands_and_unknown_regions()
    {
        var doc = Criterion1();
        Assert.Throws<ArgumentOutOfRangeException>(() => doc.Apply(new RegionCommand.Accept(Conflict(doc).RegionId, ResolutionChoice.Manual)));
        Assert.Throws<ArgumentOutOfRangeException>(() => doc.Apply(new RegionCommand.Accept(99, ResolutionChoice.Left)));
    }

    [Fact]
    public void Reset_restores_the_initial_text_and_choice()
    {
        var doc = Criterion1();
        var initial = doc.Text;
        var id = Conflict(doc).RegionId;
        doc.Apply(new RegionCommand.Accept(id, ResolutionChoice.Left));
        Assert.IsType<CommandOutcome.Changed>(doc.Apply(new RegionCommand.Reset(id)));
        Assert.Equal(initial, doc.Text);
        Assert.Equal(ResolutionChoice.Unresolved, Conflict(doc).Choice);

        doc.Apply(new RegionCommand.Accept(1, ResolutionChoice.Right));
        doc.Apply(new RegionCommand.Reset(1));
        Assert.Equal(initial, doc.Text);
        Assert.Equal(ResolutionChoice.Left, doc.Regions[1].Choice);
    }

    [Fact]
    public void Reset_of_a_region_already_in_its_initial_state_changes_nothing()
    {
        var doc = Criterion1();
        Assert.IsType<CommandOutcome.NoChange>(doc.Apply(new RegionCommand.Reset(Conflict(doc).RegionId)));
        Assert.IsType<CommandOutcome.NoChange>(doc.Apply(new RegionCommand.Reset(1)));
        Assert.Equal(0, doc.Version);
    }

    [Fact]
    public void Reset_of_a_manual_region_asks_first_and_clears_acknowledgement()
    {
        var doc = Criterion1();
        var c = Conflict(doc);
        doc.Edit(new TextChange(c.Start, 0, "mine "));
        doc.Apply(new RegionCommand.Acknowledge(c.RegionId));
        var asked = Assert.IsType<CommandOutcome.NeedsConfirmation>(doc.Apply(new RegionCommand.Reset(c.RegionId)));
        Assert.Equal("Replace your manual edit in this region?", asked.Prompt);
        doc.Apply(new RegionCommand.Reset(c.RegionId), confirmed: true);
        Assert.Equal(ResolutionChoice.Unresolved, Conflict(doc).Choice);
        Assert.False(Conflict(doc).Acknowledged);
        Assert.Equal("line 20\n", TextOf(doc, Conflict(doc)));
    }

    [Fact]
    public void Acknowledge_completes_a_manual_conflict()
    {
        var doc = Criterion1();
        var c = Conflict(doc);
        doc.Edit(new TextChange(c.Start, 0, "mine "));
        Assert.False(doc.IsComplete);
        var outcome = Assert.IsType<CommandOutcome.Changed>(doc.Apply(new RegionCommand.Acknowledge(c.RegionId)));
        Assert.Empty(outcome.Changes);
        Assert.True(doc.IsComplete);
        Assert.True(Conflict(doc).Acknowledged);
        Assert.Equal(ResolutionChoice.Manual, Conflict(doc).Choice);
    }

    [Fact]
    public void Acknowledge_is_undoable_and_only_applies_to_an_unacknowledged_manual_conflict()
    {
        var doc = Criterion1();
        var c = Conflict(doc);
        Assert.IsType<CommandOutcome.NoChange>(doc.Apply(new RegionCommand.Acknowledge(c.RegionId)));
        Assert.IsType<CommandOutcome.NoChange>(doc.Apply(new RegionCommand.Acknowledge(1)));
        doc.Edit(new TextChange(c.Start, 0, "mine "));
        doc.Apply(new RegionCommand.Acknowledge(c.RegionId));
        Assert.IsType<CommandOutcome.NoChange>(doc.Apply(new RegionCommand.Acknowledge(c.RegionId)));
        Assert.Empty(doc.Undo()!);
        Assert.False(Conflict(doc).Acknowledged);
        Assert.False(doc.IsComplete);
    }

    [Fact]
    public void Apply_all_non_conflicting_skips_manual_regions_and_lists_them()
    {
        var doc = Criterion1();
        var initial = doc.Text;
        doc.Apply(new RegionCommand.Accept(1, ResolutionChoice.Base));
        doc.Apply(new RegionCommand.Accept(5, ResolutionChoice.Base));
        var manual = doc.Regions[5];
        doc.Edit(new TextChange(manual.Start, 0, "mine "));
        var version = doc.Version;
        var outcome = Assert.IsType<CommandOutcome.Changed>(doc.Apply(new RegionCommand.ApplyAllNonConflicting()));
        Assert.Equal([5], outcome.SkippedManual);
        Assert.Equal(ResolutionChoice.Left, doc.Regions[1].Choice);
        Assert.Equal("left 3", Lines(doc.Text)[2]);
        Assert.Equal(ResolutionChoice.Manual, doc.Regions[5].Choice);
        Assert.StartsWith("mine ", Lines(doc.Text)[39]);
        Assert.Equal(version + 1, doc.Version);
        Assert.Equal(initial.Length + "mine ".Length - 1, doc.Text.Length);   // region 5 is "line 40" plus the typed prefix
    }

    [Fact]
    public void Apply_all_non_conflicting_is_one_undo_entry_and_a_no_op_when_nothing_differs()
    {
        var doc = Criterion1();
        Assert.IsType<CommandOutcome.NoChange>(doc.Apply(new RegionCommand.ApplyAllNonConflicting()));
        Assert.Equal(0, doc.Version);
        doc.Apply(new RegionCommand.Accept(1, ResolutionChoice.Base));
        doc.Apply(new RegionCommand.Accept(5, ResolutionChoice.Base));
        var changed = doc.Text;
        var regions = doc.Regions.ToArray();
        var outcome = Assert.IsType<CommandOutcome.Changed>(doc.Apply(new RegionCommand.ApplyAllNonConflicting()));
        Assert.Equal(2, outcome.Changes.Count);
        doc.Undo();
        Assert.Equal(changed, doc.Text);
        Assert.Equal(regions, doc.Regions);
        Assert.Equal(ResolutionChoice.Base, doc.Regions[5].Choice);
    }

    [Fact]
    public void Criterion_2_apply_all_lists_a_manual_conflict_as_skipped_without_changing_anything()
    {
        var doc = Criterion1();
        var c = Conflict(doc);
        doc.Edit(new TextChange(c.Start, 0, "mine "));
        var text = doc.Text;
        var version = doc.Version;
        var outcome = Assert.IsType<CommandOutcome.Changed>(doc.Apply(new RegionCommand.ApplyAllNonConflicting()));
        Assert.Equal([c.RegionId], outcome.SkippedManual);
        Assert.Empty(outcome.Changes);
        Assert.Equal(text, doc.Text);
        Assert.Equal(version, doc.Version);
        doc.Undo();
        Assert.Equal(ResolutionChoice.Unresolved, Conflict(doc).Choice);
        Assert.Equal("line 20", Lines(doc.Text)[19]);
        Assert.Null(doc.Undo());
    }

    [Fact]
    public void Apply_all_reapplies_a_reset_region_and_lists_a_manual_one_in_the_same_outcome()
    {
        var doc = Criterion1();
        doc.Apply(new RegionCommand.Accept(5, ResolutionChoice.Base));
        doc.Edit(new TextChange(doc.Regions[1].Start, 0, "mine "));
        var outcome = Assert.IsType<CommandOutcome.Changed>(doc.Apply(new RegionCommand.ApplyAllNonConflicting()));
        Assert.Equal([1], outcome.SkippedManual);
        Assert.Single(outcome.Changes);
        Assert.Equal(ResolutionChoice.Right, doc.Regions[5].Choice);
        Assert.Equal("right 40", Lines(doc.Text)[39]);
        Assert.Equal(ResolutionChoice.Manual, doc.Regions[1].Choice);
        doc.Undo();
        Assert.Equal(ResolutionChoice.Base, doc.Regions[5].Choice);
        Assert.Equal(ResolutionChoice.Manual, doc.Regions[1].Choice);
    }

    [Fact]
    public void Typed_newline_uses_the_dominant_eol_of_left()
    {
        var doc = Build(Lf("a", "b"), Crlf("a", "b", "c"), Lf("a", "b"));
        Assert.Equal("\r\n", doc.NewLine);
        var change = Assert.Single(doc.Edit(new TextChange(1, 0, "\n")));
        Assert.Equal("\r\n", change.InsertedText);
        Assert.StartsWith("a\r\n\r\n", doc.Text.Replace("a\r\r\n", "a\r\n"));
        doc.Edit(new TextChange(0, 0, "x\r\ny\rz\n"));
        Assert.StartsWith("x\r\ny\rz\r\na", doc.Text);
    }

    [Fact]
    public void Newline_falls_back_to_right_on_a_tie_then_to_lf()
    {
        Assert.Equal("\r\n", Build(Lf("a", "b"), [.. Crlf("a"), .. Lf("b")], Crlf("a", "b")).NewLine);
        Assert.Equal("\n", Build(Lf("a"), Lf("a"), Crlf("a")).NewLine);
        Assert.Equal("\r\n", Build(Lf("a"), [new Line("a", LineEnding.None)], Crlf("a")).NewLine);
        Assert.Equal("\n", Build(Lf("a"), [new Line("a", LineEnding.None)], [new Line("a", LineEnding.None)]).NewLine);
        Assert.Equal("\n", Build([], [], []).NewLine);
    }

    [Fact]
    public void Accepting_right_on_a_last_line_without_newline_keeps_no_newline()
    {
        var doc = NoFinalNewline();
        Assert.EndsWith("c\n", doc.Text);
        doc.Apply(new RegionCommand.Accept(Conflict(doc).RegionId, ResolutionChoice.Right));
        Assert.Equal("a\nb\nright c", doc.Text);
        doc.Apply(new RegionCommand.Accept(Conflict(doc).RegionId, ResolutionChoice.Left));
        Assert.Equal("a\nb\nleft c\n", doc.Text);
    }

    [Fact]
    public void Both_keeps_lines_apart_when_the_first_side_has_no_final_newline()
    {
        var doc = NoFinalNewline();
        doc.Apply(new RegionCommand.Accept(Conflict(doc).RegionId, ResolutionChoice.RightThenLeft));
        Assert.Equal("a\nb\nright c\nleft c\n", doc.Text);
    }

    [Fact]
    public void Typing_coalesces_within_a_second_and_commands_never_do()
    {
        var clock = new FakeTimeProvider();
        var doc = Criterion1(clock);
        var c = Conflict(doc);
        var original = doc.Text;
        for (var i = 0; i < 3; i++)
        {
            doc.Edit(new TextChange(c.Start + i, 0, "k"));
            clock.Advance(TimeSpan.FromMilliseconds(100));
        }
        Assert.Equal(3, doc.Undo()!.Count);
        Assert.Equal(original, doc.Text);

        doc.Redo();
        doc.Apply(new RegionCommand.Accept(c.RegionId, ResolutionChoice.Left), confirmed: true);
        doc.Apply(new RegionCommand.Accept(c.RegionId, ResolutionChoice.Right));
        Assert.Equal("right 20", Lines(doc.Text)[19]);
        doc.Undo();
        Assert.Equal("left 20", Lines(doc.Text)[19]);
        doc.Undo();
        Assert.StartsWith("kkk", Lines(doc.Text)[19]);
    }

    [Fact]
    public void Typing_does_not_coalesce_after_a_pause_a_jump_a_region_change_or_a_kind_change()
    {
        var clock = new FakeTimeProvider();
        var doc = Criterion1(clock);
        var c = Conflict(doc);

        doc.Edit(new TextChange(c.Start, 0, "a"));
        clock.Advance(TimeSpan.FromMilliseconds(1001));
        doc.Edit(new TextChange(c.Start + 1, 0, "b"));
        doc.Undo();
        Assert.Equal("aline 20", Lines(doc.Text)[19]);

        doc.Edit(new TextChange(c.Start + 4, 0, "c"));
        doc.Edit(new TextChange(c.Start + 1, 0, "d"));
        doc.Undo();
        Assert.Equal("alince 20", Lines(doc.Text)[19]);

        doc.Edit(new TextChange(c.Start + 1, 1, ""));
        doc.Edit(new TextChange(c.Start + 1, 0, "z"));
        doc.Undo();
        Assert.Equal("aince 20", Lines(doc.Text)[19]);

        // Typing into a different region does not join the previous entry.
        var other = doc.Regions[1];
        doc.Edit(new TextChange(other.Start, 0, "p"));
        doc.Edit(new TextChange(other.Start + 1, 0, "q"));
        doc.Edit(new TextChange(doc.Regions[3].End - 1, 0, "r"));
        doc.Undo();
        Assert.StartsWith("pq", Lines(doc.Text)[2]);
        doc.Undo();
        Assert.Equal("left 3", Lines(doc.Text)[2]);
    }

    [Fact]
    public void Multi_character_inserts_never_coalesce()
    {
        var doc = Criterion1(new FakeTimeProvider());
        var c = Conflict(doc);
        doc.Edit(new TextChange(c.Start, 0, "multi"));
        doc.Edit(new TextChange(c.Start + 5, 0, "m"));
        doc.Undo();
        Assert.Equal("multiline 20", Lines(doc.Text)[19]);
    }

    [Fact]
    public void Consecutive_single_character_deletes_coalesce_for_backspace_and_forward_delete()
    {
        var doc = Criterion1(new FakeTimeProvider());
        var c = Conflict(doc);
        var original = doc.Text;
        doc.Edit(new TextChange(c.Start + 3, 1, ""));
        doc.Edit(new TextChange(c.Start + 2, 1, ""));
        doc.Edit(new TextChange(c.Start + 1, 1, ""));
        Assert.Equal(3, doc.Undo()!.Count);
        Assert.Equal(original, doc.Text);
        doc.Edit(new TextChange(c.Start, 1, ""));
        doc.Edit(new TextChange(c.Start, 1, ""));
        Assert.Equal(2, doc.Undo()!.Count);
        Assert.Equal(original, doc.Text);
    }

    [Fact]
    public void Typing_after_an_undo_starts_a_new_entry_and_clears_redo()
    {
        var doc = Criterion1(new FakeTimeProvider());
        var c = Conflict(doc);
        doc.Edit(new TextChange(c.Start, 0, "a"));
        doc.Edit(new TextChange(c.Start + 1, 0, "b"));
        doc.Undo();
        doc.Edit(new TextChange(c.Start, 0, "x"));
        Assert.Null(doc.Redo());
        doc.Edit(new TextChange(c.Start + 1, 0, "y"));
        Assert.Equal(2, doc.Undo()!.Count);
        Assert.Equal("line 20", Lines(doc.Text)[19]);
    }

    [Fact]
    public void Redo_replays_the_undone_change_with_the_same_region_states()
    {
        var doc = Criterion1();
        var id = Conflict(doc).RegionId;
        doc.Apply(new RegionCommand.Accept(id, ResolutionChoice.Left));
        var text = doc.Text;
        var regions = doc.Regions.ToArray();
        var version = doc.Version;
        Assert.NotNull(doc.Undo());
        Assert.Equal(version + 1, doc.Version);
        Assert.NotNull(doc.Redo());
        Assert.Equal(text, doc.Text);
        Assert.Equal(regions, doc.Regions);
        Assert.Null(doc.Redo());
        doc.Undo();
        doc.Apply(new RegionCommand.Accept(id, ResolutionChoice.Right));
        Assert.Null(doc.Redo());
    }

    [Fact]
    public void Undo_returns_null_when_there_is_nothing_to_undo()
    {
        var doc = Criterion1();
        Assert.Null(doc.Undo());
        Assert.Null(doc.Redo());
    }

    [Fact]
    public void Undo_n_after_n_operations_is_identity() =>   // P2
        Gen.Int[0, 5].Array[1, 30].Sample(ops =>
        {
            var doc = Criterion1();
            var text = doc.Text;
            var states = doc.Regions.ToArray();
            foreach (var op in ops)
            {
                RandomOp(doc, op);
            }
            while (doc.Undo() is not null)
            {
            }
            Assert.Equal(text, doc.Text);
            Assert.Equal(states, doc.Regions);
        });

    [Fact]
    public void Redo_all_after_undo_all_reproduces_the_final_state() =>
        Gen.Int[0, 5].Array[1, 30].Sample(ops =>
        {
            var doc = Criterion1();
            foreach (var op in ops)
            {
                RandomOp(doc, op);
            }
            var text = doc.Text;
            var states = doc.Regions.ToArray();
            while (doc.Undo() is not null)
            {
            }
            while (doc.Redo() is not null)
            {
            }
            Assert.Equal(text, doc.Text);
            Assert.Equal(states, doc.Regions);
        });
}
