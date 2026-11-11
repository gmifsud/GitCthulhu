using System.Globalization;
using Avalonia;
using Avalonia.Automation;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.Threading;
using Avalonia.VisualTree;
using Resolve.Core.Merge;
using Resolve.Core.Resolution;
using Resolve.Core.Session;

namespace Resolve.App.Shell;

/// <summary>
/// The screen-reader path through a merge (decision O1-A): the editors expose nothing to assistive technology,
/// so each non-equal region gets one focusable item with an exact accessible name, and a polite live status line
/// carries announcements. Built from standard controls only.
/// </summary>
public sealed class ConflictNavigator : UserControl
{
    private readonly MergeToolSession _session;
    private readonly ListBox _list = new();
    private readonly List<int> _regionIds = [];
    private readonly Dictionary<int, string> _names = [];
    private readonly Dictionary<int, string> _summaries = [];
    private List<ListBoxItem> _items = [];

    public ConflictNavigator(MergeToolSession session)
    {
        ArgumentNullException.ThrowIfNull(session);
        _session = session;

        AutomationProperties.SetName(_list, "Conflicts and changes");
        _list.AddHandler(KeyDownEvent, OnKeyDown, RoutingStrategies.Tunnel | RoutingStrategies.Bubble);
        // Only the left button selects: a right or middle press is swallowed before the ListBox can move the selection,
        // which would silently retarget the region commands.
        _list.AddHandler(PointerPressedEvent, OnPointerPressedTunnel, RoutingStrategies.Tunnel);

        Status = new TextBlock { TextWrapping = Avalonia.Media.TextWrapping.Wrap };
        AutomationProperties.SetLiveSetting(Status, AutomationLiveSetting.Polite);

        var layout = new DockPanel();
        DockPanel.SetDock(Status, Dock.Bottom);
        layout.Children.Add(Status);
        layout.Children.Add(_list);
        Content = layout;
        Refresh();
    }

    /// <summary>The region the navigator is on, or null when there are no listed regions.</summary>
    public int? CurrentRegionId => _list.SelectedIndex is var index and >= 0 && index < _regionIds.Count ? _regionIds[index] : null;

    /// <summary>Raised when the user presses Enter or Space on, or clicks, an item.</summary>
    public event Action<int>? RegionChosen;

    /// <summary>The polite live region holding the last announcement.</summary>
    public TextBlock Status { get; }

    /// <summary>Selects the region and scrolls it into view. Does not raise <see cref="RegionChosen"/>.</summary>
    public void Select(int regionId)
    {
        var index = _regionIds.IndexOf(regionId);
        if (index < 0)
        {
            return;
        }
        _list.SelectedIndex = index;
        _list.ScrollIntoView(index);
    }

    /// <summary>
    /// Sets the live status line, which assistive technology announces politely. The line is cleared first and set on
    /// the next dispatcher turn, so repeating the same message still changes the text and is read again.
    /// </summary>
    public void Announce(string text)
    {
        ArgumentNullException.ThrowIfNull(text);
        Status.Text = string.Empty;
        Dispatcher.UIThread.Post(() => Status.Text = text);
    }

    /// <summary>The accessible name of the region's item, or null when it is not listed.</summary>
    internal string? NameOf(int regionId) => _names.GetValueOrDefault(regionId);

    /// <summary>A short "Conflict 1 of 1: taken from Current branch" for announcing the outcome of a command.</summary>
    internal string? SummaryOf(int regionId) => _summaries.GetValueOrDefault(regionId);

    /// <summary>
    /// Brings every item name in line with the current document. Items are keyed by region id and updated in place,
    /// so selection and keyboard focus stay where they are; the item set is rebuilt only if the regions changed.
    /// </summary>
    public void Refresh()
    {
        var labels = _session.Inputs.Labels;
        var regions = _session.Document.Regions.Where(region => region.Kind != RegionKind.Equal).ToList();
        var entries = new List<(int RegionId, string Name, string Summary)>(regions.Count);
        AddEntries(regions.Where(region => region.Kind == RegionKind.Conflict).ToList(), "Conflict", labels, entries);
        AddEntries(regions.Where(region => region.Kind != RegionKind.Conflict).ToList(), "Change", labels, entries);

        _names.Clear();
        _summaries.Clear();
        foreach (var (regionId, name, summary) in entries)
        {
            _names[regionId] = name;
            _summaries[regionId] = summary;
        }

        if (entries.Select(entry => entry.RegionId).SequenceEqual(_regionIds))
        {
            for (var index = 0; index < entries.Count; index++)
            {
                Rename(_items[index], entries[index].Name);
            }
            return;
        }

        var current = CurrentRegionId;
        _regionIds.Clear();
        _items = new List<ListBoxItem>(entries.Count);
        foreach (var (regionId, name, _) in entries)
        {
            _items.Add(NewItem(regionId, name));
            _regionIds.Add(regionId);
        }
        _list.ItemsSource = _items;
        _list.SelectedIndex = current is { } id ? _regionIds.IndexOf(id) : -1;
    }

    /// <summary>Moves keyboard focus to the current item (the first one when none is current), or the list when it is empty.</summary>
    public bool FocusCurrent()
    {
        var index = _list.SelectedIndex >= 0 ? _list.SelectedIndex : 0;
        return index < _items.Count ? _items[index].Focus(NavigationMethod.Tab) : _list.Focus(NavigationMethod.Tab);
    }

    internal IReadOnlyList<Control> Items() => [.. _items];

    private static void AddEntries(List<RegionView> regions, string noun, SideLabels labels, List<(int RegionId, string Name, string Summary)> entries)
    {
        for (var position = 0; position < regions.Count; position++)
        {
            var region = regions[position];
            var heading = string.Create(CultureInfo.InvariantCulture, $"{noun} {position + 1} of {regions.Count}");
            var state = State(region, labels);
            entries.Add((region.RegionId, $"{heading}, {Lines(region)}, {state}", $"{heading}: {state}"));
        }
    }

    private static void Rename(ListBoxItem item, string name)
    {
        if (item.Content is TextBlock text && text.Text != name)
        {
            text.Text = name;
            AutomationProperties.SetName(item, name);
        }
    }

    private ListBoxItem NewItem(int regionId, string name)
    {
        var item = new ListBoxItem { Content = new TextBlock { Text = name }, Focusable = true };
        AutomationProperties.SetName(item, name);
        // ListBoxItem marks the press handled when it selects itself, so listen to handled events too.
        item.AddHandler(PointerPressedEvent, (_, e) =>
        {
            if (e.GetCurrentPoint(item).Properties.IsLeftButtonPressed)
            {
                Choose(regionId);
            }
        }, RoutingStrategies.Bubble, handledEventsToo: true);
        return item;
    }

    private static string Lines(RegionView region)
    {
        var first = region.ResultLines.Start + 1;
        var last = Math.Max(first, region.ResultLines.End);
        return string.Create(CultureInfo.InvariantCulture, $"lines {first}–{last}");
    }

    private static string State(RegionView region, SideLabels labels) => region.Choice switch
    {
        ResolutionChoice.Unresolved => region.Kind == RegionKind.Conflict ? "unresolved" : $"taken from {SideOf(region, labels)}",
        ResolutionChoice.Left => $"taken from {labels.LeftRole}",
        ResolutionChoice.Right => $"taken from {labels.RightRole}",
        ResolutionChoice.LeftThenRight => $"both, {labels.LeftRole} first",
        ResolutionChoice.RightThenLeft => $"both, {labels.RightRole} first",
        ResolutionChoice.Base => "kept the base",
        ResolutionChoice.Manual when region.Kind == RegionKind.Conflict && !region.Acknowledged => "edited by hand, not yet marked resolved",
        _ => "edited by hand",
    };

    private static string SideOf(RegionView region, SideLabels labels) => region.Kind == RegionKind.RightOnly ? labels.RightRole : labels.LeftRole;

    private static void OnPointerPressedTunnel(object? sender, PointerPressedEventArgs e)
    {
        if (!e.GetCurrentPoint(sender as Visual).Properties.IsLeftButtonPressed)
        {
            e.Handled = true;
        }
    }

    private void OnKeyDown(object? sender, KeyEventArgs e)
    {
        if (e.Key is Key.Enter or Key.Space && e.Source is Control source && source.FindAncestorOfType<ListBoxItem>(true) is { } item
            && _list.IndexFromContainer(item) is var index and >= 0 && index < _regionIds.Count)
        {
            _list.SelectedIndex = index;
            e.Handled = true;
            Choose(_regionIds[index]);
        }
    }

    private void Choose(int regionId)
    {
        var index = _regionIds.IndexOf(regionId);
        if (index >= 0)
        {
            _list.SelectedIndex = index;
        }
        RegionChosen?.Invoke(regionId);
    }
}
