using System.Globalization;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Threading;
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

    /// <summary>Runs a command. Region commands act on the navigator's current region; the outcome is announced.</summary>
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
        _ => throw new ArgumentOutOfRangeException(nameof(command), command, "Unknown window command."),
    };

    /// <summary>Runs one command at a time and nothing once the window has a result, so a pending confirmation cannot be overtaken.</summary>
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
        finally
        {
            _busy = false;
        }
    }

    private void Fire(Func<Task> body) => Observe(Guarded(body));

    /// <summary>Surfaces a failure on the UI thread instead of losing it in a discarded task.</summary>
    private static async void Observe(Task task) => await task;

    private void OnKeyDown(object? sender, KeyEventArgs e)
    {
        if (Keymap.For(e.Key, e.KeyModifiers) is { } command)
        {
            e.Handled = true;
            Run(command);
        }
    }

    // ---- navigation ----

    private Task Navigate(bool conflictsOnly, bool forward)
    {
        var regions = _session.Document.Regions;
        var from = Navigator.CurrentRegionId is { } current ? IndexOf(regions, current) : -1;
        var candidates = Enumerable.Range(0, regions.Count)
            .Where(index => conflictsOnly ? regions[index].Kind == RegionKind.Conflict : regions[index].Kind != RegionKind.Equal)
            .ToList();
        if (candidates.Count == 0)
        {
            Announce(conflictsOnly ? "No conflicts" : "No changes");
            return Task.CompletedTask;
        }
        // The next candidate after the current region, or the previous one before it; past either end, wrap.
        var target = forward
            ? candidates.FirstOrDefault(index => index > from, candidates[0])
            : candidates.LastOrDefault(index => index < from, candidates[^1]);
        GoTo(regions[target].RegionId, announce: true);
        return Task.CompletedTask;
    }

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

    /// <summary>The current region, or — when the navigator is on none — the first conflict, which becomes current.</summary>
    private async Task OnCurrent(Func<int, RegionCommand> make)
    {
        var id = Navigator.CurrentRegionId;
        if (id is null)
        {
            var first = _session.Document.Regions.FirstOrDefault(r => r.Kind == RegionKind.Conflict);
            if (first is null)
            {
                Announce("No conflict selected");
                return;
            }
            id = first.RegionId;
            GoTo(id.Value, announce: false);
        }
        await ApplyAsync(make(id.Value), id);
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
        SaveAndExitButton.IsEnabled = _session.Document.IsComplete;
    }

    private void Announce(string text) => Navigator.Announce(text);

    // ---- save and the exits ----

    private bool IncompleteConfirmationNeeded(out string prompt)
    {
        var counts = _session.Document.Counts;
        var remaining = counts.UnresolvedConflicts + counts.ManualUnacknowledged;
        prompt = string.Create(CultureInfo.InvariantCulture, $"Saves exactly what RESULT shows; {remaining} regions still unresolved.");
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
        await Exit(overwrite => _session.SaveAndExit(overwrite));
    }

    private async Task ExitUnresolvedAsync()
    {
        if (IncompleteConfirmationNeeded(out var prompt) && !await _confirm(prompt))
        {
            return;
        }
        await Exit(overwrite => _session.ExitUnresolved(overwrite));
    }

    /// <summary>
    /// Runs a session exit; on an external change asks once and retries over it, or leaves with
    /// <see cref="ExitCode.ExternalChangeDeclined"/>. A failed write keeps the window open so nothing is lost.
    /// </summary>
    private async Task Exit(Func<bool, ExitCode> exit)
    {
        var code = exit(false);
        if (code == ExitCode.ExternalChangeDeclined && await _confirm(ExternalChangePrompt))
        {
            code = exit(true);
        }
        if (code == ExitCode.WriteFailed)
        {
            Announce("Could not write the file. Your changes are still here.");
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

    private async Task<bool> ConfirmDiscard() => _session.Document.Version == 0 || await _confirm(DiscardPrompt);

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
