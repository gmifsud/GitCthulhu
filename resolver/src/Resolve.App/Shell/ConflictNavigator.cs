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

    /// <summary>Rebuilds every item name from the current document, keeping the current region.</summary>
    public void Refresh()
    {
        var current = CurrentRegionId;
        var labels = _session.Inputs.Labels;
        var regions = _session.Document.Regions.Where(region => region.Kind != RegionKind.Equal).ToList();
        var conflicts = regions.Where(region => region.Kind == RegionKind.Conflict).ToList();
        var changes = regions.Where(region => region.Kind != RegionKind.Conflict).ToList();

        var items = new List<ListBoxItem>(regions.Count);
        _regionIds.Clear();
        _names.Clear();
        _summaries.Clear();
        AddItems(conflicts, "Conflict", labels, items);
        AddItems(changes, "Change", labels, items);

        _list.ItemsSource = items;
        _list.SelectedIndex = current is { } id ? _regionIds.IndexOf(id) : -1;
    }

    internal IReadOnlyList<Control> Items() => _list.ItemsSource is IEnumerable<ListBoxItem> items ? [.. items] : [];

    private void AddItems(List<RegionView> regions, string noun, SideLabels labels, List<ListBoxItem> items)
    {
        for (var position = 0; position < regions.Count; position++)
        {
            var region = regions[position];
            var heading = string.Create(CultureInfo.InvariantCulture, $"{noun} {position + 1} of {regions.Count}");
            var state = State(region, labels);
            var name = $"{heading}, {Lines(region)}, {state}";
            var item = new ListBoxItem { Content = new TextBlock { Text = name }, Focusable = true };
            AutomationProperties.SetName(item, name);
            var regionId = region.RegionId;
            _names[regionId] = name;
            _summaries[regionId] = $"{heading}: {state}";
            // ListBoxItem marks the press handled when it selects itself, so listen to handled events too.
            item.AddHandler(PointerPressedEvent, (_, e) =>
            {
                if (e.GetCurrentPoint(item).Properties.IsLeftButtonPressed)
                {
                    Choose(regionId);
                }
            }, RoutingStrategies.Bubble, handledEventsToo: true);
            items.Add(item);
            _regionIds.Add(regionId);
        }
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
