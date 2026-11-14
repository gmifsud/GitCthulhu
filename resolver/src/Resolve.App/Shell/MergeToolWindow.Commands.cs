using System.Globalization;
using Avalonia;
using Avalonia.Automation;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Threading;
using Avalonia.VisualTree;
using Resolve.App.Editor;
using Resolve.Core.Cli;
using Resolve.Core.Merge;
using Resolve.Core.Resolution;
using Resolve.Core.Session;

namespace Resolve.App.Shell;

/// <summary>What the window's keys and buttons do: navigation, region commands, save and the exits, each confirmed where the spec says so.</summary>
public sealed partial class MergeToolWindow
{
    private const string DiscardPrompt = "Discard your changes and leave the file untouched?";
    private const string ExternalChangePrompt = "The file changed on disk since it was opened. Overwrite it?";

    /// <summary>Ruling R32: what the user hears when a command fails unexpectedly.</summary>
    internal static string FailureMessage(string reason) => $"Something went wrong: {Sentence(reason)}. Your changes are still here.";

    /// <summary>
    /// After an edit in RESULT could not be applied: that one keystroke may be lost, so "your changes are still
    /// here" would not be true. RESULT has been reset to the document, which is what will be saved.
    /// </summary>
    internal static string EditFailureMessage(string reason) =>
        $"Something went wrong: {Sentence(reason)}. Your last edit may not have been applied: the result shows what will be saved.";

    /// <summary>A reason ready to sit before a full stop: exception messages often end with one already.</summary>
    private static string Sentence(string reason) => reason.TrimEnd().TrimEnd('.');

    /// <summary>
    /// Runs a command. Region commands act on the region at the RESULT caret when RESULT has focus, else on the
    /// navigator's current region (ruling R30); the outcome is announced.
    /// </summary>
    public void Run(WindowCommand command) => Observe(RunAsync(command));

    internal Task RunAsync(WindowCommand command) => Guarded(() => Dispatch(command));

    private Task Dispatch(WindowCommand command) => command switch
    {
        WindowCommand.NextConflict => Navigate(conflictsOnly: true, forward: true),
        WindowCommand.PreviousConflict => Navigate(conflictsOnly: true, forward: false),
        WindowCommand.NextChange => Navigate(conflictsOnly: false, forward: true),
        WindowCommand.PreviousChange => Navigate(conflictsOnly: false, forward: false),
        WindowCommand.AcceptLeft => OnCurrent(id => new RegionCommand.Accept(id, ResolutionChoice.Left)),
        WindowCommand.AcceptRight => OnCurrent(id => new RegionCommand.Accept(id, ResolutionChoice.Right)),
        WindowCommand.BothLeftFirst => OnCurrent(id => new RegionCommand.Accept(id, ResolutionChoice.LeftThenRight)),
        WindowCommand.BothRightFirst => OnCurrent(id => new RegionCommand.Accept(id, ResolutionChoice.RightThenLeft)),
        WindowCommand.KeepBase => OnCurrent(id => new RegionCommand.Accept(id, ResolutionChoice.Base)),
        WindowCommand.ResetRegion => OnCurrent(id => new RegionCommand.Reset(id)),
        WindowCommand.AcknowledgeRegion => OnCurrent(id => new RegionCommand.Acknowledge(id)),
        WindowCommand.ApplyAllNonConflicting => ApplyAsync(new RegionCommand.ApplyAllNonConflicting(), regionId: null),
        WindowCommand.Undo => HistoryAsync(undo: true),
        WindowCommand.Redo => HistoryAsync(undo: false),
        WindowCommand.Save => SaveAsync(),
        WindowCommand.SaveAndExit => SaveAndExitAsync(),
        WindowCommand.ExitUnresolved => ExitUnresolvedAsync(),
        WindowCommand.Cancel => CancelAsync(),
        _ => throw new ArgumentOutOfRangeException(nameof(command), command, "Unknown window command."),
    };

    /// <summary>
    /// Runs one command at a time and nothing once the window has a result, so a pending confirmation cannot be
    /// overtaken. Ruling R32: a command that fails unexpectedly is announced and the window stays, with the resolution.
    /// </summary>
    private async Task Guarded(Func<Task> body)
    {
        if (_busy || Result is not null)
        {
            return;
        }
        _busy = true;
        try
        {
            await body();
        }
        catch (Exception error)
        {
            AnnounceFailure(error);
        }
        finally
        {
            _busy = false;
        }
    }

    /// <summary>Awaits a fire-and-forget task on the UI thread; nothing it throws may end the session.</summary>
    private async void Observe(Task task)
    {
        try
        {
            await task;
        }
        catch (Exception error)
        {
            AnnounceFailure(error);
        }
    }

    /// <summary>Runs an event handler's work; nothing it throws may escape into Avalonia's input handling or the editor.</summary>
    private void Safely(Action action)
    {
        try
        {
            action();
        }
        catch (Exception error)
        {
            AnnounceFailure(error);
        }
    }

    /// <summary>A safety net caught <paramref name="error"/>: put RESULT back in line with the document, then say so.</summary>
    internal void AnnounceFailure(Exception error)
    {
        View?.ShowDocument();
        Announce(FailureMessage(error.Message));
    }

    private void OnKeyDown(object? sender, KeyEventArgs e)
    {
        if (FocusMoveFor(e) is { } forward)
        {
            e.Handled = true;
            Safely(() => MoveFocus(forward));
            return;
        }
        if (Keymap.For(e.Key, e.KeyModifiers) is { } command)
        {
            e.Handled = true;
            Run(command);
        }
    }

    // ---- focus (ruling R29) ----

    /// <summary>
    /// F6 / Shift+F6 cycle the panes and bars from anywhere; Tab / Shift+Tab do the same out of the read-only panes,
    /// whose editors would otherwise keep focus. RESULT keeps Tab as a character. Null when the key is not a focus move.
    /// </summary>
    private bool? FocusMoveFor(KeyEventArgs e)
    {
        var shift = e.KeyModifiers == KeyModifiers.Shift;
        if (e.KeyModifiers != KeyModifiers.None && !shift)
        {
            return null;
        }
        if (e.Key == Key.F6)
        {
            return !shift;
        }
        return e.Key == Key.Tab && FocusedGroup() is 0 or 2 ? !shift : null;
    }

    /// <summary>LEFT, RESULT, RIGHT, the navigator, the action bar, the footer: the F6 order.</summary>
    private Control?[] FocusGroups() => [View.Left.View, View.Result.View, View.Right.View, Navigator, _actionBar, _footer];

    /// <summary>The index in the F6 order (LEFT 0 … footer 5) of the group holding keyboard focus, or -1.</summary>
    internal int FocusedGroup()
    {
        if (FocusManager?.GetFocusedElement() is not Visual focused)
        {
            return -1;
        }
        var groups = FocusGroups();
        for (var index = 0; index < groups.Length; index++)
        {
            if (groups[index] is { } group && (group == focused || group.IsVisualAncestorOf(focused)))
            {
                return index;
            }
        }
        return -1;
    }

    private void MoveFocus(bool forward)
    {
        var count = FocusGroups().Length;
        var current = FocusedGroup();
        var start = current < 0 ? (forward ? count - 1 : 0) : current;
        // Skip a group that cannot take focus (say, every footer button disabled) rather than stopping on it.
        for (var step = 1; step <= count; step++)
        {
            var next = (((start + (forward ? step : -step)) % count) + count) % count;
            if (FocusGroup(next))
            {
                return;
            }
        }
    }

    private bool FocusGroup(int index) => index switch
    {
        0 => ((AvaloniaEditSurface)View.Left).Focus(),
        1 => ((AvaloniaEditSurface)View.Result).Focus(),
        2 => ((AvaloniaEditSurface)View.Right).Focus(),
        3 => Navigator.FocusCurrent(),
        4 => FirstEnabled(_actionBar)?.Focus(NavigationMethod.Tab) ?? false,
        _ => FirstEnabled(_footer)?.Focus(NavigationMethod.Tab) ?? false,
    };

    private static Button? FirstEnabled(Panel? bar) => bar?.Children.OfType<Button>().FirstOrDefault(button => button.IsEffectivelyEnabled);

    // ---- navigation ----

    private Task Navigate(bool conflictsOnly, bool forward)
    {
        var regions = _session.Document.Regions;
        var from = Navigator.CurrentRegionId is { } current ? IndexOf(regions, current) : -1;
        // Spec §4.6: F8 visits the conflicts still to resolve (unresolved, or edited by hand and not yet marked
        // resolved); F7 every non-equal region.
        var candidates = Enumerable.Range(0, regions.Count)
            .Where(index => conflictsOnly ? IsUnresolvedConflict(regions[index]) : regions[index].Kind != RegionKind.Equal)
            .ToList();
        if (candidates.Count == 0)
        {
            Announce(conflictsOnly ? "No unresolved conflicts" : "No changes");
            return Task.CompletedTask;
        }
        // The next candidate after the current region, or the previous one before it; past either end, wrap.
        var target = forward
            ? candidates.FirstOrDefault(index => index > from, candidates[0])
            : candidates.LastOrDefault(index => index < from, candidates[^1]);
        GoTo(regions[target].RegionId, announce: true);
        return Task.CompletedTask;
    }

    private static bool IsUnresolvedConflict(RegionView region) =>
        region.Kind == RegionKind.Conflict
        && (region.Choice == ResolutionChoice.Unresolved || region is { Choice: ResolutionChoice.Manual, Acknowledged: false });

    private static int IndexOf(IReadOnlyList<RegionView> regions, int regionId)
    {
        for (var index = 0; index < regions.Count; index++)
        {
            if (regions[index].RegionId == regionId)
            {
                return index;
            }
        }
        return -1;
    }

    private void GoTo(int regionId, bool announce)
    {
        var region = _session.Document.Regions.FirstOrDefault(r => r.RegionId == regionId);
        if (region is null)
        {
            return;
        }
        Navigator.Select(regionId);
        View.RevealLine(region.ResultLines.Start);
        View.Result.MoveCaretToLine(region.ResultLines.Start);
        if (announce && Navigator.NameOf(regionId) is { } name)
        {
            Announce(name);
        }
    }

    // ---- region commands, undo, redo ----

    /// <summary>
    /// Ruling R30: the region at the RESULT caret when RESULT has focus and the caret is in a non-equal region — the
    /// navigator follows it — else the navigator's current region, else the first conflict, which becomes current.
    /// The target is revealed before the command runs, so a resolution is never applied out of sight.
    /// </summary>
    private async Task OnCurrent(Func<int, RegionCommand> make)
    {
        int id;
        if (CaretRegion() is { } atCaret)
        {
            id = atCaret;
            if (Navigator.CurrentRegionId != id)
            {
                Navigator.Select(id);
                if (Navigator.NameOf(id) is { } name)
                {
                    Announce(name);
                }
            }
        }
        else if (Navigator.CurrentRegionId is { } current)
        {
            id = current;
        }
        else
        {
            var first = _session.Document.Regions.FirstOrDefault(r => r.Kind == RegionKind.Conflict);
            if (first is null)
            {
                Announce("No conflict selected");
                return;
            }
            id = first.RegionId;
            GoTo(id, announce: false);
        }
        if (_session.Document.Regions.FirstOrDefault(r => r.RegionId == id) is { } target)
        {
            View.RevealLine(target.ResultLines.Start);
        }
        await ApplyAsync(make(id), id);
    }

    /// <summary>
    /// The non-equal region holding the RESULT caret, when RESULT has keyboard focus. A caret exactly at a region's end
    /// belongs to it only when that end is not a line start (an empty region, or text without a final newline).
    /// </summary>
    private int? CaretRegion()
    {
        var result = (AvaloniaEditSurface)View.Result;
        if (!result.HasFocus)
        {
            return null;
        }
        var caret = result.CaretOffset;
        var text = _session.Document.Text;
        foreach (var region in _session.Document.Regions)
        {
            if (region.Kind == RegionKind.Equal || caret < region.Start || caret > region.End)
            {
                continue;
            }
            if (caret < region.End || region.Start == region.End || text[region.End - 1] != '\n')
            {
                return region.RegionId;
            }
        }
        return null;
    }

    private async Task ApplyAsync(RegionCommand command, int? regionId)
    {
        var document = _session.Document;
        var outcome = document.Apply(command);
        if (outcome is CommandOutcome.NeedsConfirmation needs)
        {
            if (!await _confirm(needs.Prompt))
            {
                Announce("Left unchanged");
                return;
            }
            outcome = document.Apply(command, confirmed: true);
        }
        switch (outcome)
        {
            case CommandOutcome.Changed changed:
                Applied(changed.Changes);
                Announce(Outcome(regionId, changed.SkippedManual.Count));
                break;
            case CommandOutcome.NoChange:
                Announce("No change");
                break;
            default:
                throw new InvalidOperationException($"Unexpected outcome {outcome}.");
        }
    }

    private string Outcome(int? regionId, int skippedManual)
    {
        if (regionId is { } id)
        {
            return Navigator.SummaryOf(id) ?? "Done";
        }
        return skippedManual == 0
            ? "Applied all non-conflicting changes"
            : string.Create(CultureInfo.InvariantCulture, $"Applied all non-conflicting changes; skipped {skippedManual} edited by hand");
    }

    private Task HistoryAsync(bool undo)
    {
        var changes = undo ? _session.Document.Undo() : _session.Document.Redo();
        if (changes is null)
        {
            Announce(undo ? "Nothing to undo" : "Nothing to redo");
        }
        else
        {
            Applied(changes);
            Announce(undo ? "Undone" : "Redone");
        }
        return Task.CompletedTask;
    }

    private void Applied(IReadOnlyList<TextChange> changes)
    {
        View.Refresh(changes);
        Update(refreshNavigator: true);
    }

    /// <summary>Brings the counter, the item names and the Save &amp; exit gate in line with the document.</summary>
    private void Update(bool refreshNavigator)
    {
        if (refreshNavigator)
        {
            Navigator.Refresh();
        }
        _counter.Text = CounterText;
        AutomationProperties.SetName(_counter, $"Remaining: {CounterText}");
        SaveAndExitButton.IsEnabled = _session.Document.IsComplete;
    }

    private void Announce(string text) => Navigator.Announce(text);

    // ---- save and the exits ----

    private bool IncompleteConfirmationNeeded(out string prompt)
    {
        var counts = _session.Document.Counts;
        var remaining = counts.UnresolvedConflicts + counts.ManualUnacknowledged;
        prompt = remaining == 1
            ? "Saves exactly what RESULT shows; 1 region still unresolved."
            : string.Create(CultureInfo.InvariantCulture, $"Saves exactly what RESULT shows; {remaining} regions still unresolved.");
        return !_session.Document.IsComplete;
    }

    private async Task SaveAsync()
    {
        if (IncompleteConfirmationNeeded(out var prompt) && !await _confirm(prompt))
        {
            return;
        }
        var result = _session.Save();
        if (result is SaveResult.ExternalChange)
        {
            if (!await _confirm(ExternalChangePrompt))
            {
                Announce("Not saved: the file changed on disk");
                return;
            }
            result = _session.Save(overwriteExternalChange: true);
        }
        Announce(result switch
        {
            SaveResult.Saved => "Saved",
            SaveResult.Failed failed => $"Save failed: {failed.Message}",
            _ => "Not saved: the file changed on disk again",
        });
    }

    private async Task SaveAndExitAsync()
    {
        if (!_session.Document.IsComplete)
        {
            Announce("Resolve every conflict before Save and exit");
            return;
        }
        await ExitAsync(overwrite => _session.SaveAndExit(overwrite));
    }

    private async Task ExitUnresolvedAsync()
    {
        if (IncompleteConfirmationNeeded(out var prompt) && !await _confirm(prompt))
        {
            return;
        }
        await ExitAsync(overwrite => _session.ExitUnresolved(overwrite));
    }

    /// <summary>
    /// Runs a session exit; on an external change asks once and retries over it, or leaves with
    /// <see cref="ExitCode.ExternalChangeDeclined"/>. A failed write keeps the window open so nothing is lost.
    /// </summary>
    internal async Task ExitAsync(Func<bool, ExitCode> exit)
    {
        var code = exit(false);
        var overwrote = false;
        if (code == ExitCode.ExternalChangeDeclined && await _confirm(ExternalChangePrompt))
        {
            overwrote = true;
            code = exit(true);
        }
        if (code == ExitCode.WriteFailed)
        {
            Announce("Could not write the file. Your changes are still here.");
            return;
        }
        if (overwrote && code == ExitCode.ExternalChangeDeclined)
        {
            // The user agreed to overwrite, but the file moved again: nothing was written, so keep the resolution.
            Announce("The file changed on disk again; nothing was written.");
            return;
        }
        if (code == ExitCode.Resolved)
        {
            if (!_session.Document.IsComplete)
            {
                throw new InvalidOperationException("The session reported resolved for an incomplete document.");
            }
            foreach (var warning in _session.MarkerWarnings())
            {
                _warnings.WriteLine(warning);
            }
        }
        Result = code;
        Close();
    }

    private async Task CancelAsync()
    {
        if (!await ConfirmDiscard())
        {
            return;
        }
        _session.Cancel();
        Result = ExitCode.Unresolved;
        Close();
    }

    /// <summary>Asks only when there are unsaved changes: edits since the last save, or edits never saved.</summary>
    private async Task<bool> ConfirmDiscard() =>
        _session.IsSaved || _session.Document.Version == 0 || await _confirm(DiscardPrompt);

    /// <summary>The title-bar close: never saves, and closes only after the user agrees to discard edits.</summary>
    private void OnClosing(object? sender, WindowClosingEventArgs e)
    {
        if (Result is not null)
        {
            return;
        }
        e.Cancel = true;
        if (!_closing)
        {
            _closing = true;
            Observe(CloseWithoutSavingAsync());
        }
    }

    private async Task CloseWithoutSavingAsync()
    {
        try
        {
            if (!await ConfirmDiscard())
            {
                return;
            }
            _session.Cancel();
            Result = ExitCode.Unresolved;
            Dispatcher.UIThread.Post(Close);
        }
        finally
        {
            _closing = false;
        }
    }
}
