using Avalonia;
using Avalonia.Automation;
using Avalonia.Controls;
using Avalonia.Headless;
using Avalonia.Headless.XUnit;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.Threading;
using Avalonia.VisualTree;
using AvaloniaEdit.Editing;
using Resolve.App.Editor;
using Resolve.App.Shell;
using Resolve.App.Tests.Support;
using Resolve.Core.Cli;
using Resolve.Core.Merge;
using Resolve.Core.Resolution;
using Resolve.Core.Session;

namespace Resolve.App.Tests;

public class MergeToolWindowTests
{
    private const string SkipReason = "An exclusive handle on MERGED blocks the write only on Windows; Unix locks are advisory and permission bits are not reliable under root.";
    private const string ManualPrompt = "Replace your manual edit in this region?";
    private const string DiscardPrompt = "Discard your changes and leave the file untouched?";
    private const string ExternalPrompt = "The file changed on disk since it was opened. Overwrite it?";

    private static void Click(Button button) => button.RaiseEvent(new RoutedEventArgs(Button.ClickEvent));

    private static string Status(MergeToolWindow window)
    {
        Dispatcher.UIThread.RunJobs();
        return window.Navigator.Status.Text ?? string.Empty;
    }

    private static AvaloniaEditSurface Result(MergeToolWindow window) => (AvaloniaEditSurface)window.View.Result;

    private static RegionView Conflict(MergeToolWindow window) => window.Session.Document.Regions.Single(r => r.Kind == RegionKind.Conflict);

    private static void TypeIntoConflict(MergeToolWindow window, string text)
    {
        window.Run(WindowCommand.NextConflict);
        Result(window).Focus();
        window.KeyTextInput(text);
    }

    [AvaloniaFact]
    public void Criterion_1_counter_navigation_and_gates()
    {
        var w = MergeWindows.Criterion1(confirm: _ => Task.FromResult(true));
        Assert.Equal("1 conflict · 2 changes", w.CounterText);
        w.Run(WindowCommand.NextConflict);
        Assert.Equal(19, w.ResultCaretLine());
        Assert.True(w.SaveButton.IsEnabled);
        Assert.False(w.SaveAndExitButton.IsEnabled);
    }

    [AvaloniaFact]
    public void Criterion_2_in_the_window_asks_before_replacing_a_manual_edit()
    {
        var asked = new List<string>();
        var w = MergeWindows.Criterion1(MergeWindows.Answering(true, asked));
        var before = Result(w).Text;
        TypeIntoConflict(w, "x");
        var typed = Result(w).Text;
        Assert.NotEqual(before, typed);
        Assert.Equal(ResolutionChoice.Manual, Conflict(w).Choice);

        w.KeyPressQwerty(PhysicalKey.ArrowLeft, RawInputModifiers.Control | RawInputModifiers.Alt);
        Assert.Equal([ManualPrompt], asked);
        Assert.Equal(ResolutionChoice.Left, Conflict(w).Choice);
        Assert.NotEqual(typed, Result(w).Text);

        w.KeyPressQwerty(PhysicalKey.Z, Keys.Command);
        Assert.Equal(typed, Result(w).Text);
        Assert.Equal(ResolutionChoice.Manual, Conflict(w).Choice);
        Assert.Equal("0 conflicts · 2 changes · 1 manual", w.CounterText);
    }

    [AvaloniaFact]
    public void Declining_the_manual_edit_prompt_leaves_the_text_alone()
    {
        var asked = new List<string>();
        var w = MergeWindows.Criterion1(MergeWindows.Answering(false, asked));
        TypeIntoConflict(w, "x");
        var typed = Result(w).Text;

        w.Run(WindowCommand.AcceptRight);

        Assert.Equal([ManualPrompt], asked);
        Assert.Equal(typed, Result(w).Text);
        Assert.Equal(ResolutionChoice.Manual, Conflict(w).Choice);
    }

    [AvaloniaFact]
    public void Resolving_then_save_and_exit_closes_with_0()
    {
        var warnings = new StringWriter();
        var w = MergeWindows.Criterion1(_ => Task.FromResult(true), warnings);
        w.Run(WindowCommand.NextConflict);
        w.Run(WindowCommand.AcceptLeft);
        Assert.Equal("0 conflicts · 2 changes", w.CounterText);
        Assert.True(w.SaveAndExitButton.IsEnabled);

        Click(w.SaveAndExitButton);

        Assert.Equal(ExitCode.Resolved, w.Result);
        Assert.Equal(string.Empty, warnings.ToString());
        Assert.Contains("left 20", File.ReadAllText(w.Session.Inputs.MergedPath), StringComparison.Ordinal);
    }

    [AvaloniaFact]
    public void Save_and_exit_prints_marker_warnings_to_the_warnings_writer()
    {
        var warnings = new StringWriter();
        var w = MergeWindows.Criterion1(_ => Task.FromResult(true), warnings);
        w.Run(WindowCommand.NextConflict);
        w.Run(WindowCommand.AcceptLeft);
        Result(w).Focus();
        w.KeyTextInput("<<<<<<< ");
        w.Run(WindowCommand.AcknowledgeRegion);
        Assert.True(w.SaveAndExitButton.IsEnabled);

        Click(w.SaveAndExitButton);

        Assert.Equal(ExitCode.Resolved, w.Result);
        Assert.Contains("conflict-marker-like line at 20", warnings.ToString(), StringComparison.Ordinal);
    }

    [AvaloniaFact]
    public void Save_and_exit_does_nothing_while_the_document_is_incomplete()
    {
        var w = MergeWindows.Criterion1(_ => Task.FromResult(true));
        var merged = File.ReadAllBytes(w.Session.Inputs.MergedPath);

        Click(w.SaveAndExitButton);

        Assert.Null(w.Result);
        Assert.Equal(merged, File.ReadAllBytes(w.Session.Inputs.MergedPath));
    }

    [AvaloniaFact]
    public void Closing_the_window_after_edits_asks_then_cancels()
    {
        var asked = new List<string>();
        var w = MergeWindows.Criterion1(MergeWindows.Answering(true, asked));
        var merged = File.ReadAllBytes(w.Session.Inputs.MergedPath);
        w.Run(WindowCommand.NextConflict);
        w.Run(WindowCommand.AcceptLeft);

        w.Close();
        Dispatcher.UIThread.RunJobs();

        Assert.Equal([DiscardPrompt], asked);
        Assert.Equal(ExitCode.Unresolved, w.Result);
        Assert.True(w.Session.IsCancelled);
        Assert.Equal(merged, File.ReadAllBytes(w.Session.Inputs.MergedPath));
    }

    [AvaloniaFact]
    public void Closing_the_window_and_declining_keeps_it_open()
    {
        var asked = new List<string>();
        var w = MergeWindows.Criterion1(MergeWindows.Answering(false, asked));
        w.Run(WindowCommand.NextConflict);
        w.Run(WindowCommand.AcceptLeft);

        w.Close();
        Dispatcher.UIThread.RunJobs();

        Assert.Equal([DiscardPrompt], asked);
        Assert.Null(w.Result);
        Assert.False(w.Session.IsCancelled);
        Assert.True(w.IsVisible);
    }

    [AvaloniaFact]
    public void Closing_without_edits_cancels_without_asking()
    {
        var asked = new List<string>();
        var w = MergeWindows.Criterion1(MergeWindows.Answering(true, asked));

        w.Close();
        Dispatcher.UIThread.RunJobs();

        Assert.Empty(asked);
        Assert.Equal(ExitCode.Unresolved, w.Result);
        Assert.True(w.Session.IsCancelled);
    }

    [AvaloniaFact]
    public void The_cancel_button_asks_after_edits_and_never_writes()
    {
        var asked = new List<string>();
        var w = MergeWindows.Criterion1(MergeWindows.Answering(true, asked));
        var merged = File.ReadAllBytes(w.Session.Inputs.MergedPath);
        w.Run(WindowCommand.AcceptLeft);

        Click(w.CancelButton);

        Assert.Equal([DiscardPrompt], asked);
        Assert.Equal(ExitCode.Unresolved, w.Result);
        Assert.Equal(merged, File.ReadAllBytes(w.Session.Inputs.MergedPath));
    }

    [AvaloniaFact]
    public void Exit_unresolved_asks_with_the_count_then_saves_what_result_shows()
    {
        var asked = new List<string>();
        var w = MergeWindows.Criterion1(MergeWindows.Answering(true, asked));

        Click(w.ExitUnresolvedButton);

        Assert.Equal(["Saves exactly what RESULT shows; 1 region still unresolved."], asked);
        Assert.Equal(ExitCode.Unresolved, w.Result);
        Assert.True(w.Session.IsSaved);
    }

    [AvaloniaFact]
    public void Exit_unresolved_declined_stays_open_and_writes_nothing()
    {
        var asked = new List<string>();
        var w = MergeWindows.Criterion1(MergeWindows.Answering(false, asked));
        var merged = File.ReadAllBytes(w.Session.Inputs.MergedPath);

        Click(w.ExitUnresolvedButton);

        Assert.Single(asked);
        Assert.Null(w.Result);
        Assert.Equal(merged, File.ReadAllBytes(w.Session.Inputs.MergedPath));
    }

    [AvaloniaFact]
    public void Ctrl_S_saves_after_confirming_and_keeps_the_window_open()
    {
        var asked = new List<string>();
        var w = MergeWindows.Criterion1(MergeWindows.Answering(true, asked));

        w.KeyPressQwerty(PhysicalKey.S, Keys.Command);

        Assert.Single(asked);
        Assert.StartsWith("Saves exactly what RESULT shows;", asked[0], StringComparison.Ordinal);
        Assert.True(w.Session.IsSaved);
        Assert.Null(w.Result);
    }

    [AvaloniaFact]
    public void Save_and_exit_on_an_external_change_asks_then_retries()
    {
        var asked = new List<string>();
        var w = MergeWindows.Criterion1(MergeWindows.Answering(true, asked));
        w.Run(WindowCommand.AcceptLeft);
        File.WriteAllText(w.Session.Inputs.MergedPath, "changed by someone else\n");

        Click(w.SaveAndExitButton);

        Assert.Equal([ExternalPrompt], asked);
        Assert.Equal(ExitCode.Resolved, w.Result);
        Assert.Contains("left 20", File.ReadAllText(w.Session.Inputs.MergedPath), StringComparison.Ordinal);
    }

    [AvaloniaFact]
    public void Save_and_exit_on_an_external_change_declined_exits_with_5_and_keeps_the_file()
    {
        var asked = new List<string>();
        var w = MergeWindows.Criterion1(MergeWindows.Answering(false, asked));
        w.Run(WindowCommand.AcceptLeft);
        File.WriteAllText(w.Session.Inputs.MergedPath, "changed by someone else\n");

        Click(w.SaveAndExitButton);

        Assert.Equal([ExternalPrompt], asked);
        Assert.Equal(ExitCode.ExternalChangeDeclined, w.Result);
        Assert.Equal("changed by someone else\n", File.ReadAllText(w.Session.Inputs.MergedPath));
    }

    [AvaloniaFact]
    public void Navigation_wraps_and_reveals_the_region()
    {
        var w = MergeWindows.Criterion1(_ => Task.FromResult(true));
        foreach (var line in new[] { 2, 19, 39, 2 })
        {
            w.Run(WindowCommand.NextChange);
            Assert.Equal(line, w.ResultCaretLine());
        }
        w.Run(WindowCommand.PreviousChange);
        Assert.Equal(39, w.ResultCaretLine());

        w.Run(WindowCommand.NextConflict);
        Assert.Equal(19, w.ResultCaretLine());
        Assert.Equal(Conflict(w).RegionId, w.Navigator.CurrentRegionId);
        w.Run(WindowCommand.NextConflict);
        Assert.Equal(19, w.ResultCaretLine());
        w.Run(WindowCommand.PreviousConflict);
        Assert.Equal(19, w.ResultCaretLine());
    }

    [AvaloniaFact]
    public void Navigation_announces_the_region_name()
    {
        var w = MergeWindows.Criterion1(_ => Task.FromResult(true));
        w.Run(WindowCommand.NextConflict);
        Assert.Equal("Conflict 1 of 1, lines 20–20, unresolved", Status(w));
    }

    [AvaloniaFact]
    public void A_region_action_with_no_current_region_selects_the_first_conflict_and_acts()
    {
        var w = MergeWindows.Criterion1(_ => Task.FromResult(true));
        Assert.Null(w.Navigator.CurrentRegionId);

        w.Run(WindowCommand.AcceptRight);

        Assert.Equal(ResolutionChoice.Right, Conflict(w).Choice);
        Assert.Equal(Conflict(w).RegionId, w.Navigator.CurrentRegionId);
        Assert.Equal("Conflict 1 of 1: taken from Incoming branch", Status(w));
        Assert.Equal("0 conflicts · 2 changes", w.CounterText);
    }

    [AvaloniaFact]
    public void Every_command_updates_result_and_the_announcement()
    {
        var w = MergeWindows.Criterion1(_ => Task.FromResult(true));
        var original = Result(w).Text;
        w.Run(WindowCommand.NextConflict);

        w.Run(WindowCommand.BothLeftFirst);
        Assert.Equal(ResolutionChoice.LeftThenRight, Conflict(w).Choice);
        Assert.Equal("Conflict 1 of 1: both, Current branch first", Status(w));
        w.Run(WindowCommand.BothRightFirst);
        Assert.Equal(ResolutionChoice.RightThenLeft, Conflict(w).Choice);
        w.Run(WindowCommand.KeepBase);
        Assert.Equal(ResolutionChoice.Base, Conflict(w).Choice);
        w.Run(WindowCommand.ResetRegion);
        Assert.Equal(ResolutionChoice.Unresolved, Conflict(w).Choice);
        Assert.Equal(original, Result(w).Text);

        w.Run(WindowCommand.Undo);
        Assert.Equal(ResolutionChoice.Base, Conflict(w).Choice);
        Assert.Equal("Undone", Status(w));
        w.Run(WindowCommand.Redo);
        Assert.Equal(ResolutionChoice.Unresolved, Conflict(w).Choice);
        Assert.Equal("Redone", Status(w));
    }

    [AvaloniaFact]
    public void Ctrl_Y_and_Ctrl_Shift_Z_redo()
    {
        var w = MergeWindows.Criterion1(_ => Task.FromResult(true));
        w.Run(WindowCommand.AcceptLeft);
        w.KeyPressQwerty(PhysicalKey.Z, Keys.Command);
        Assert.Equal(ResolutionChoice.Unresolved, Conflict(w).Choice);
        w.KeyPressQwerty(PhysicalKey.Y, Keys.Command);
        Assert.Equal(ResolutionChoice.Left, Conflict(w).Choice);
        w.KeyPressQwerty(PhysicalKey.Z, Keys.Command);
        w.KeyPressQwerty(PhysicalKey.Z, Keys.Command | RawInputModifiers.Shift);
        Assert.Equal(ResolutionChoice.Left, Conflict(w).Choice);
    }

    [AvaloniaFact]
    public void Undo_and_redo_with_nothing_to_do_say_so()
    {
        var w = MergeWindows.Criterion1(_ => Task.FromResult(true));
        w.Run(WindowCommand.Undo);
        Assert.Equal("Nothing to undo", Status(w));
        w.Run(WindowCommand.Redo);
        Assert.Equal("Nothing to redo", Status(w));
    }

    [AvaloniaFact]
    public void Apply_all_non_conflicting_reports_through_the_announcement()
    {
        var w = MergeWindows.Criterion1(_ => Task.FromResult(true));
        w.Run(WindowCommand.ApplyAllNonConflicting);
        Assert.False(string.IsNullOrEmpty(Status(w)));
    }

    [AvaloniaFact]
    public void The_header_names_the_file_and_both_sides()
    {
        var w = MergeWindows.Criterion1(_ => Task.FromResult(true));
        Assert.Equal("Merging a.txt · LEFT Current branch · RIGHT Incoming branch", w.HeaderText);
    }

    [AvaloniaFact]
    public void Buttons_carry_the_role_labels()
    {
        var w = MergeWindows.Criterion1(_ => Task.FromResult(true));
        var buttons = w.GetVisualDescendants().OfType<Button>().ToList();
        // A footer button's content carries its access-key marker; its accessible name is the plain label.
        var labels = buttons.Select(b => AutomationProperties.GetName(b) is { Length: > 0 } name ? name : b.Content as string).ToList();
        // macOS has no Alt access keys: no underline marker, and the exits are Cmd+Option chords.
        var mac = Keymap.Current == KeyStyle.Mac;
        Assert.Equal(mac ? "Save & exit (0)" : "_Save & exit (0)", w.SaveAndExitButton.Content);
        Assert.Equal(mac ? "Exit unresolved (1)" : "Exit _unresolved (1)", w.ExitUnresolvedButton.Content);
        Assert.Equal(mac ? "Cancel — leave file untouched (1)" : "_Cancel — leave file untouched (1)", w.CancelButton.Content);
        Assert.Equal(mac ? "Cmd+Option+S" : "Alt+S", AutomationProperties.GetAccessKey(w.SaveAndExitButton));
        Assert.Contains("Accept LEFT · Current branch", labels);
        Assert.Contains("Accept RIGHT · Incoming branch", labels);
        Assert.Contains("Both, LEFT first", labels);
        Assert.Contains("Both, RIGHT first", labels);
        Assert.Contains("Keep base", labels);
        Assert.Contains("Mark resolved", labels);
        Assert.Contains("Apply all non-conflicting", labels);
        Assert.Contains("Save & exit (0)", labels);
        Assert.Contains("Exit unresolved (1)", labels);
        Assert.Contains("Cancel — leave file untouched (1)", labels);
    }

    [AvaloniaFact]
    public void The_counter_name_carries_the_counts()
    {
        var w = MergeWindows.Criterion1(_ => Task.FromResult(true));
        Assert.Equal("Remaining: 1 conflict · 2 changes", AutomationProperties.GetName(w.Counter));

        w.Run(WindowCommand.NextConflict);
        w.Run(WindowCommand.AcceptLeft);

        Assert.Equal("0 conflicts · 2 changes", w.Counter.Text);
        Assert.Equal("Remaining: 0 conflicts · 2 changes", AutomationProperties.GetName(w.Counter));
    }

    [AvaloniaFact]
    public void The_unresolved_prompt_is_singular_for_one_region_and_plural_otherwise()
    {
        var asked = new List<string>();
        var w = MergeWindows.Criterion1(MergeWindows.Answering(false, asked));
        Click(w.ExitUnresolvedButton);
        Assert.Equal(["Saves exactly what RESULT shows; 1 region still unresolved."], asked);

        var many = MergeWindows.Show(Sessions.Long(), MergeWindows.Answering(false, asked));
        var conflicts = many.Session.Document.Counts.UnresolvedConflicts;
        Assert.True(conflicts > 1);
        asked.Clear();
        Click(many.ExitUnresolvedButton);
        Assert.Equal([$"Saves exactly what RESULT shows; {conflicts} regions still unresolved."], asked);
    }

    [AvaloniaFact]
    public void Cancelling_after_a_save_with_no_later_edits_does_not_ask()
    {
        var asked = new List<string>();
        var w = MergeWindows.Criterion1(MergeWindows.Answering(true, asked));
        w.Run(WindowCommand.NextConflict);
        w.Run(WindowCommand.AcceptLeft);
        w.KeyPressQwerty(PhysicalKey.S, Keys.Command);
        Assert.True(w.Session.IsSaved);
        var saved = File.ReadAllBytes(w.Session.Inputs.MergedPath);

        Click(w.CancelButton);

        Assert.Empty(asked);
        Assert.Equal(ExitCode.Unresolved, w.Result);
        Assert.True(w.Session.IsCancelled);
        Assert.Equal(saved, File.ReadAllBytes(w.Session.Inputs.MergedPath));
    }

    [AvaloniaFact]
    public void Cancelling_after_a_save_and_a_later_edit_still_asks()
    {
        var asked = new List<string>();
        var w = MergeWindows.Criterion1(MergeWindows.Answering(true, asked));
        w.Run(WindowCommand.NextConflict);
        w.Run(WindowCommand.AcceptLeft);
        w.KeyPressQwerty(PhysicalKey.S, Keys.Command);
        w.Run(WindowCommand.AcceptRight);
        asked.Clear();

        Click(w.CancelButton);

        Assert.Equal([DiscardPrompt], asked);
        Assert.Equal(ExitCode.Unresolved, w.Result);
    }

    [AvaloniaFact]
    public async Task A_retry_after_an_agreed_overwrite_that_meets_another_change_keeps_the_window_open()
    {
        var asked = new List<string>();
        var w = MergeWindows.Criterion1(MergeWindows.Answering(true, asked));
        w.Run(WindowCommand.AcceptLeft);
        var calls = new List<bool>();

        await w.ExitAsync(overwrite =>
        {
            calls.Add(overwrite);
            return ExitCode.ExternalChangeDeclined;
        });

        Assert.Equal([false, true], calls);
        Assert.Equal([ExternalPrompt], asked);
        Assert.Null(w.Result);
        Assert.False(w.Session.IsCancelled);
        Assert.Equal("The file changed on disk again; nothing was written.", Status(w));
    }

    [AvaloniaFact]
    public void A_failed_save_and_exit_keeps_the_window_open_and_a_retry_succeeds()
    {
        Assert.SkipUnless(OperatingSystem.IsWindows(), SkipReason);
        var w = MergeWindows.Criterion1(_ => Task.FromResult(true));
        w.Run(WindowCommand.NextConflict);
        w.Run(WindowCommand.AcceptLeft);
        var path = w.Session.Inputs.MergedPath;
        var before = File.ReadAllBytes(path);

        using (new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.None))
        {
            Click(w.SaveAndExitButton);
            Assert.Null(w.Result);
            Assert.True(w.IsVisible);
            Assert.Equal("Could not write the file. Your changes are still here.", Status(w));
        }
        Assert.Equal(before, File.ReadAllBytes(path));

        Click(w.SaveAndExitButton);

        Assert.Equal(ExitCode.Resolved, w.Result);
        Assert.Contains("left 20", File.ReadAllText(path), StringComparison.Ordinal);
    }

    [AvaloniaFact]
    public void A_failed_exit_unresolved_keeps_the_window_open_and_a_retry_succeeds()
    {
        Assert.SkipUnless(OperatingSystem.IsWindows(), SkipReason);
        var w = MergeWindows.Criterion1(_ => Task.FromResult(true));
        var path = w.Session.Inputs.MergedPath;
        var before = File.ReadAllBytes(path);

        using (new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.None))
        {
            Click(w.ExitUnresolvedButton);
            Assert.Null(w.Result);
            Assert.True(w.IsVisible);
            Assert.Equal("Could not write the file. Your changes are still here.", Status(w));
        }
        Assert.Equal(before, File.ReadAllBytes(path));

        Click(w.ExitUnresolvedButton);

        Assert.Equal(ExitCode.Unresolved, w.Result);
        Assert.True(w.Session.IsSaved);
    }

    [AvaloniaFact]
    public void Ctrl_S_with_the_file_locked_stays_open_announces_the_failure_and_writes_nothing()
    {
        Assert.SkipUnless(OperatingSystem.IsWindows(), SkipReason);
        var w = MergeWindows.Criterion1(_ => Task.FromResult(true));
        w.Run(WindowCommand.NextConflict);
        w.Run(WindowCommand.AcceptLeft);
        var path = w.Session.Inputs.MergedPath;
        var before = File.ReadAllBytes(path);

        using (new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.None))
        {
            w.KeyPressQwerty(PhysicalKey.S, Keys.Command);
            Assert.StartsWith("Save failed:", Status(w), StringComparison.Ordinal);
        }

        Assert.Null(w.Result);
        Assert.True(w.IsVisible);
        Assert.False(w.Session.IsSaved);
        Assert.Equal(before, File.ReadAllBytes(path));
    }

    [AvaloniaFact]
    public void The_encoding_choice_appears_only_when_the_encodings_differ()
    {
        var w = MergeWindows.Criterion1(_ => Task.FromResult(true));
        Assert.False(w.Session.EncodingsDiffer);
        Assert.Empty(w.GetVisualDescendants().OfType<RadioButton>());
    }

    [Fact]
    public void Keymap_matches_the_spec_table()
    {
        const KeyModifiers ctrlAlt = KeyModifiers.Control | KeyModifiers.Alt;
        (Key Key, KeyModifiers Mods, WindowCommand Command)[] rows =
        [
            (Key.F8, KeyModifiers.None, WindowCommand.NextConflict),
            (Key.F8, KeyModifiers.Shift, WindowCommand.PreviousConflict),
            (Key.F7, KeyModifiers.None, WindowCommand.NextChange),
            (Key.F7, KeyModifiers.Shift, WindowCommand.PreviousChange),
            (Key.Left, ctrlAlt, WindowCommand.AcceptLeft),
            (Key.Right, ctrlAlt, WindowCommand.AcceptRight),
            (Key.Down, ctrlAlt, WindowCommand.BothLeftFirst),
            (Key.Up, ctrlAlt, WindowCommand.BothRightFirst),
            (Key.Back, ctrlAlt, WindowCommand.KeepBase),
            (Key.R, ctrlAlt, WindowCommand.ResetRegion),
            (Key.Enter, KeyModifiers.Control, WindowCommand.AcknowledgeRegion),
            (Key.Z, KeyModifiers.Control, WindowCommand.Undo),
            (Key.Y, KeyModifiers.Control, WindowCommand.Redo),
            (Key.Z, KeyModifiers.Control | KeyModifiers.Shift, WindowCommand.Redo),
            (Key.S, KeyModifiers.Control, WindowCommand.Save),
            (Key.S, KeyModifiers.Alt, WindowCommand.SaveAndExit),
            (Key.U, KeyModifiers.Alt, WindowCommand.ExitUnresolved),
            (Key.C, KeyModifiers.Alt, WindowCommand.Cancel),
        ];
        foreach (var (key, mods, command) in rows)
        {
            Assert.Equal(command, Keymap.For(key, mods, KeyStyle.Standard));
        }
        Assert.Null(Keymap.For(Key.F8, KeyModifiers.Control, KeyStyle.Standard));
        Assert.Null(Keymap.For(Key.Left, KeyModifiers.Alt, KeyStyle.Standard));
        Assert.Null(Keymap.For(Key.Left, KeyModifiers.None, KeyStyle.Standard));
        Assert.Null(Keymap.For(Key.S, KeyModifiers.None, KeyStyle.Standard));
        Assert.Null(Keymap.For(Key.Enter, KeyModifiers.None, KeyStyle.Standard));
        Assert.Null(Keymap.For(Key.Z, KeyModifiers.None, KeyStyle.Standard));
    }

    [Fact]
    public void On_macOS_Cmd_stands_for_Ctrl_and_Option_letters_stay_typing()
    {
        const KeyModifiers cmd = KeyModifiers.Meta;
        (Key Key, KeyModifiers Mods, WindowCommand Command)[] rows =
        [
            (Key.F8, KeyModifiers.None, WindowCommand.NextConflict),
            (Key.F8, KeyModifiers.Shift, WindowCommand.PreviousConflict),
            (Key.Left, KeyModifiers.Control | KeyModifiers.Alt, WindowCommand.AcceptLeft),
            (Key.R, KeyModifiers.Control | KeyModifiers.Alt, WindowCommand.ResetRegion),
            (Key.Enter, cmd, WindowCommand.AcknowledgeRegion),
            (Key.Z, cmd, WindowCommand.Undo),
            (Key.Z, cmd | KeyModifiers.Shift, WindowCommand.Redo),
            (Key.Y, cmd, WindowCommand.Redo),
            (Key.S, cmd, WindowCommand.Save),
            (Key.S, cmd | KeyModifiers.Alt, WindowCommand.SaveAndExit),
            (Key.U, cmd | KeyModifiers.Alt, WindowCommand.ExitUnresolved),
            (Key.C, cmd | KeyModifiers.Alt, WindowCommand.Cancel),
        ];
        foreach (var (key, mods, command) in rows)
        {
            Assert.Equal(command, Keymap.For(key, mods, KeyStyle.Mac));
        }
        // Option+S, Option+U and Option+C type ß, ¨ and ç: they must never end the session.
        Assert.Null(Keymap.For(Key.S, KeyModifiers.Alt, KeyStyle.Mac));
        Assert.Null(Keymap.For(Key.U, KeyModifiers.Alt, KeyStyle.Mac));
        Assert.Null(Keymap.For(Key.C, KeyModifiers.Alt, KeyStyle.Mac));
        Assert.Null(Keymap.For(Key.Z, KeyModifiers.Control, KeyStyle.Mac));   // Ctrl+Z is not Undo on macOS
        Assert.Null(Keymap.For(Key.Left, cmd | KeyModifiers.Alt, KeyStyle.Mac));   // not a region command
        Assert.Equal("Cmd+Option+S", Keymap.ExitKeyName('s', KeyStyle.Mac));
        Assert.Equal("Alt+U", Keymap.ExitKeyName('U', KeyStyle.Standard));
    }

    [AvaloniaFact]
    public void A_safety_net_puts_RESULT_back_in_line_with_the_document()
    {
        var w = MergeWindows.Criterion1(_ => Task.FromResult(true));
        Result(w).Resync("drifted\n");   // as a command failing part-way might leave it

        w.AnnounceFailure(new InvalidOperationException("boom."));

        Assert.Equal(w.Session.Document.Text, Result(w).Text);
        Assert.Equal("Something went wrong: boom. Your changes are still here.", Status(w));
    }

    private static object? Focused(MergeToolWindow window) => window.FocusManager?.GetFocusedElement();

    private static AvaloniaEditSurface Pane(IEditorSurface surface) => (AvaloniaEditSurface)surface;

    /// <summary>A real key press and release; no release once the press closed the window (an exit key).</summary>
    private static void Press(MergeToolWindow window, PhysicalKey key, RawInputModifiers modifiers = RawInputModifiers.None)
    {
        window.KeyPressQwerty(key, modifiers);
        if (window.Result is null)
        {
            window.KeyReleaseQwerty(key, modifiers);
        }
    }

    private static List<RegionView> Conflicts(MergeToolWindow window) =>
        [.. window.Session.Document.Regions.Where(region => region.Kind == RegionKind.Conflict)];

    private static RegionView Region(MergeToolWindow window, int regionId) =>
        window.Session.Document.Regions.Single(region => region.RegionId == regionId);

    [AvaloniaFact]
    public void F6_and_Shift_F6_cycle_focus_through_the_panes_navigator_actions_and_footer()   // ruling R29
    {
        var w = MergeWindows.Criterion1(_ => Task.FromResult(true));
        Pane(w.View.Left).Focus();
        Assert.Equal(0, w.FocusedGroup());

        // LEFT → RESULT → RIGHT → navigator → action bar → footer, wrapping; Shift+F6 runs it backwards.
        foreach (var expected in new[] { 1, 2, 3, 4, 5, 0 })
        {
            Press(w, PhysicalKey.F6);
            Assert.Equal(expected, w.FocusedGroup());
        }
        Assert.True(Pane(w.View.Left).HasFocus);
        foreach (var expected in new[] { 5, 4, 3, 2, 1, 0 })
        {
            Press(w, PhysicalKey.F6, RawInputModifiers.Shift);
            Assert.Equal(expected, w.FocusedGroup());
        }

        // Where each stop lands: the navigator's item, the first action, the first footer button.
        Press(w, PhysicalKey.F6);
        Press(w, PhysicalKey.F6);
        Press(w, PhysicalKey.F6);
        Assert.IsType<ListBoxItem>(Focused(w));
        Press(w, PhysicalKey.F6);
        Assert.Same(w.AcceptLeftButton, Focused(w));
        Press(w, PhysicalKey.F6);
        Assert.Same(w.SaveButton, Focused(w));
    }

    [AvaloniaFact]
    public void Tab_leaves_the_read_only_panes_and_result_keeps_tab_as_a_character()   // ruling R29
    {
        var w = MergeWindows.Criterion1(_ => Task.FromResult(true));
        Pane(w.View.Left).Focus();
        Press(w, PhysicalKey.Tab);
        Assert.Equal(1, w.FocusedGroup());   // out of LEFT, into RESULT

        var before = Result(w).Text;
        Press(w, PhysicalKey.Tab);
        Assert.Equal(1, w.FocusedGroup());   // RESULT keeps focus and takes the Tab as text
        Assert.NotEqual(before, Result(w).Text);
        Assert.Equal(w.Session.Document.Text, Result(w).Text);

        Pane(w.View.Right).Focus();
        Press(w, PhysicalKey.Tab);
        Assert.Equal(3, w.FocusedGroup());   // out of RIGHT, into the navigator
        Pane(w.View.Right).Focus();
        Press(w, PhysicalKey.Tab, RawInputModifiers.Shift);
        Assert.Equal(1, w.FocusedGroup());   // back to RESULT
        Pane(w.View.Left).Focus();
        Press(w, PhysicalKey.Tab, RawInputModifiers.Shift);
        Assert.Equal(5, w.FocusedGroup());   // wraps to the footer
    }

    [AvaloniaFact]
    public void Alt_S_on_a_complete_document_saves_and_exits_with_0()   // ruling R29
    {
        var w = MergeWindows.Criterion1(_ => Task.FromResult(true));
        w.Run(WindowCommand.AcceptLeft);
        Assert.True(w.Session.Document.IsComplete);
        Pane(w.View.Result).Focus();

        Press(w, PhysicalKey.S, Keys.Exit);

        Assert.Equal(ExitCode.Resolved, w.Result);
        Assert.Equal(w.Session.Document.Text, File.ReadAllText(w.Session.Inputs.MergedPath));
    }

    [AvaloniaFact]
    public void Alt_U_and_Alt_C_reach_exit_unresolved_and_cancel()   // ruling R29
    {
        var asked = new List<string>();
        var w = MergeWindows.Criterion1(MergeWindows.Answering(false, asked));
        var before = File.ReadAllBytes(w.Session.Inputs.MergedPath);

        Press(w, PhysicalKey.U, Keys.Exit);
        Assert.Equal(["Saves exactly what RESULT shows; 1 region still unresolved."], asked);
        Assert.Null(w.Result);

        Press(w, PhysicalKey.C, Keys.Exit);
        Assert.Equal(ExitCode.Unresolved, w.Result);   // no edits, so Cancel asks nothing
        Assert.True(w.Session.IsCancelled);
        Assert.Equal(before, File.ReadAllBytes(w.Session.Inputs.MergedPath));
    }

    [AvaloniaFact]
    public void A_command_keeps_focus_on_the_navigator_item_and_renames_it_in_place()   // final review I1
    {
        var w = MergeWindows.Criterion1(_ => Task.FromResult(true));
        w.Run(WindowCommand.NextConflict);
        Assert.True(w.Navigator.FocusCurrent());
        var item = Assert.IsType<ListBoxItem>(Focused(w));

        Press(w, PhysicalKey.ArrowLeft, RawInputModifiers.Control | RawInputModifiers.Alt);

        Assert.Equal(ResolutionChoice.Left, Conflict(w).Choice);
        Assert.Same(item, Focused(w));
        Assert.Same(item, w.Navigator.Items()[0]);
        Assert.Equal("Conflict 1 of 1, lines 20–20, taken from Current branch", AutomationProperties.GetName(item));
    }

    [AvaloniaFact]
    public void A_region_command_acts_on_the_conflict_at_the_result_caret()   // ruling R30 (final review I2)
    {
        var w = MergeWindows.Show(Sessions.Long(), _ => Task.FromResult(true));
        var conflicts = Conflicts(w);
        Assert.Equal(4, conflicts.Count);
        w.Run(WindowCommand.NextConflict);
        Assert.Equal(conflicts[0].RegionId, w.Navigator.CurrentRegionId);

        // The caret is in conflict 3, which is scrolled out of sight; the navigator is still on conflict 1.
        Pane(w.View.Result).Focus();
        Pane(w.View.Result).CaretOffset = conflicts[2].Start + 2;
        w.View.Result.ScrollTo(0);
        w.UpdateLayout();

        Press(w, PhysicalKey.ArrowRight, RawInputModifiers.Control | RawInputModifiers.Alt);
        w.UpdateLayout();

        Assert.Equal(ResolutionChoice.Right, Region(w, conflicts[2].RegionId).Choice);
        Assert.Equal(ResolutionChoice.Unresolved, Region(w, conflicts[0].RegionId).Choice);
        Assert.Equal(conflicts[2].RegionId, w.Navigator.CurrentRegionId);
        Assert.Equal("Conflict 3 of 4: taken from Incoming branch", Status(w));
        var line = Region(w, conflicts[2].RegionId).ResultLines.Start;
        var first = w.View.Result.YToLine(w.View.Result.VerticalOffset);
        var last = w.View.Result.YToLine(w.View.Result.VerticalOffset + w.View.Result.ViewportHeight);
        Assert.InRange(line, first, last);
    }

    [AvaloniaFact]
    public void With_the_result_caret_in_an_equal_region_a_command_acts_on_the_navigators_region()   // ruling R30
    {
        var w = MergeWindows.Show(Sessions.Long(), _ => Task.FromResult(true));
        var conflicts = Conflicts(w);
        w.Run(WindowCommand.NextConflict);
        Pane(w.View.Result).Focus();
        Pane(w.View.Result).CaretOffset = 0;   // line 1 is unchanged everywhere

        Press(w, PhysicalKey.ArrowRight, RawInputModifiers.Control | RawInputModifiers.Alt);

        Assert.Equal(ResolutionChoice.Right, Region(w, conflicts[0].RegionId).Choice);
        Assert.All(conflicts.Skip(1), conflict => Assert.Equal(ResolutionChoice.Unresolved, Region(w, conflict.RegionId).Choice));
    }

    [AvaloniaFact]
    public void F8_visits_only_unresolved_conflicts_and_F7_every_change()   // spec §4.6
    {
        var w = MergeWindows.Show(Sessions.Long(), _ => Task.FromResult(true));
        var conflicts = Conflicts(w);
        w.Run(WindowCommand.NextConflict);
        w.Run(WindowCommand.NextConflict);
        Assert.Equal(conflicts[1].RegionId, w.Navigator.CurrentRegionId);
        w.Run(WindowCommand.AcceptLeft);

        Press(w, PhysicalKey.F8);
        Assert.Equal(conflicts[2].RegionId, w.Navigator.CurrentRegionId);
        Press(w, PhysicalKey.F8, RawInputModifiers.Shift);
        Assert.Equal(conflicts[0].RegionId, w.Navigator.CurrentRegionId);   // skips the resolved conflict 2

        var visited = new HashSet<int>();
        for (var step = 0; step < w.Session.Document.Regions.Count; step++)
        {
            Press(w, PhysicalKey.F7);
            visited.Add(w.Navigator.CurrentRegionId!.Value);
        }
        Assert.Contains(conflicts[1].RegionId, visited);
    }

    [AvaloniaFact]
    public void F8_with_every_conflict_resolved_says_so()
    {
        var w = MergeWindows.Criterion1(_ => Task.FromResult(true));
        w.Run(WindowCommand.AcceptLeft);
        Press(w, PhysicalKey.F8);
        Assert.Equal("No unresolved conflicts", Status(w));
    }

    [AvaloniaFact]
    public void An_unexpected_failure_in_a_command_is_announced_and_the_window_stays()   // ruling R32 (final review I4)
    {
        var w = MergeWindows.Criterion1(_ => throw new InvalidOperationException("boom"));
        var before = File.ReadAllBytes(w.Session.Inputs.MergedPath);

        Press(w, PhysicalKey.U, Keys.Exit);   // Exit unresolved asks first, and asking throws

        Assert.Equal("Something went wrong: boom. Your changes are still here.", Status(w));
        Assert.Null(w.Result);
        Assert.False(w.Session.IsCancelled);
        Assert.Equal(before, File.ReadAllBytes(w.Session.Inputs.MergedPath));
        w.Run(WindowCommand.NextConflict);   // and the window still takes commands
        Assert.Equal("Conflict 1 of 1, lines 20–20, unresolved", Status(w));
    }

    [AvaloniaFact]
    public void A_result_that_cannot_be_encoded_keeps_the_window_open_on_save_and_exit()   // ruling R32
    {
        var w = MergeWindows.Criterion1(_ => Task.FromResult(true));
        w.Run(WindowCommand.AcceptLeft);
        w.View.Refresh(w.Session.Document.Edit(new TextChange(0, 0, "\uD800")));   // a lone surrogate
        Assert.True(w.Session.Document.IsComplete);
        var before = File.ReadAllBytes(w.Session.Inputs.MergedPath);

        Press(w, PhysicalKey.S, Keys.Exit);

        Assert.Null(w.Result);
        Assert.Equal("Could not write the file. Your changes are still here.", Status(w));
        Assert.Equal(before, File.ReadAllBytes(w.Session.Inputs.MergedPath));
    }

    [AvaloniaFact]
    public void An_empty_stage_base_is_a_real_ancestor_so_there_is_no_no_common_ancestor_banner()
    {
        var w = MergeWindows.Show(Sessions.EmptyStageBase(), _ => Task.FromResult(true));
        Assert.Equal(BaseKind.EmptyStage, w.Session.Inputs.BaseKind);
        Assert.DoesNotContain(w.GetVisualDescendants().OfType<TextBlock>(), block => block.Text?.StartsWith("No common ancestor", StringComparison.Ordinal) == true);
    }

    [AvaloniaFact]
    public void No_button_reads_ours_or_theirs()
    {
        var w = MergeWindows.Criterion1(_ => Task.FromResult(true));
        var buttons = w.GetVisualDescendants().OfType<Button>().ToList();
        Assert.NotEmpty(buttons);
        foreach (var button in buttons)
        {
            var text = $"{button.Content} {AutomationProperties.GetName(button)}";
            Assert.DoesNotContain("ours", text, StringComparison.OrdinalIgnoreCase);
            Assert.DoesNotContain("theirs", text, StringComparison.OrdinalIgnoreCase);
        }
    }
}
