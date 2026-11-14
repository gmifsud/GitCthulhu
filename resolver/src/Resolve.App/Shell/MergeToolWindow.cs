using System.Globalization;
using Avalonia;
using Avalonia.Automation;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.Layout;
using Avalonia.Media;
using Resolve.App.Panes;
using Resolve.Core.Cli;
using Resolve.Core.Merge;
using Resolve.Core.Session;

namespace Resolve.App.Shell;

/// <summary>
/// The merge-tool window (spec §4.2): header, three panes, the accessible navigator, the action bar and the footer.
/// This file is the layout; <c>MergeToolWindow.Commands.cs</c> holds what the controls do. Nothing here writes a file
/// except through <see cref="MergeToolSession"/>, and title-bar close never saves.
/// </summary>
public sealed partial class MergeToolWindow : Window
{
    private const double Gap = 8;
    private const double NavigatorWidth = 300;
    private const string NoCommonAncestor = "No common ancestor — every difference is a conflict";

    private readonly MergeToolSession _session;
    private readonly Func<string, Task<bool>> _confirm;
    private readonly TextWriter _warnings;
    private readonly TextBlock _counter = new();
    private WrapPanel? _actionBar;
    private StackPanel? _footer;
    private bool _busy;
    private bool _closing;

    /// <param name="warnings">Where a successful Save &amp; exit lists conflict-marker-like lines; standard error when null.</param>
    public MergeToolWindow(MergeToolSession session, Func<string, Task<bool>> confirm, TextWriter? warnings = null)
    {
        ArgumentNullException.ThrowIfNull(session);
        ArgumentNullException.ThrowIfNull(confirm);
        _session = session;
        _confirm = confirm;
        _warnings = warnings ?? Console.Error;

        var labels = session.Inputs.Labels;
        HeaderText = HeaderFor(session);
        Title = $"Cthulhu Resolve — {FileName(session)}";

        View = new ThreePaneView(session);
        Navigator = new ConflictNavigator(session);
        AcceptLeftButton = Action($"Accept LEFT · {labels.LeftRole}", WindowCommand.AcceptLeft);
        ApplyAllButton = Action("Apply all non-conflicting", WindowCommand.ApplyAllNonConflicting);
        SaveButton = Footer("Save", accessKey: null, WindowCommand.Save);
        // Ruling R29: the three exits have access keys (Alt+S, Alt+U, Alt+C), handled through the key table.
        SaveAndExitButton = Footer(FooterLabel("Save & exit", ExitCode.Resolved), 'S', WindowCommand.SaveAndExit);
        ExitUnresolvedButton = Footer(FooterLabel("Exit unresolved", ExitCode.Unresolved), 'U', WindowCommand.ExitUnresolved);
        CancelButton = Footer(FooterLabel("Cancel — leave file untouched", ExitCode.Unresolved), 'C', WindowCommand.Cancel);

        var root = new Grid { RowDefinitions = new RowDefinitions("Auto,*,Auto,Auto"), Margin = new Thickness(Gap) };
        Place(root, BuildHeader(session), 0);
        Place(root, BuildBody(), 1);
        Place(root, BuildActionBar(labels), 2);
        Place(root, BuildFooter(), 3);
        Content = root;

        AddHandler(KeyDownEvent, OnKeyDown, RoutingStrategies.Tunnel);
        Closing += OnClosing;
        Navigator.RegionChosen += id => Safely(() => GoTo(id, announce: false));
        View.ResultEdited += () => Safely(() => Update(refreshNavigator: true));
        View.Problem += reason => Announce(EditFailureMessage(reason));
        Update(refreshNavigator: false);
    }

    /// <summary>The exit code once a footer button has finished the session; null while it is open. Title-bar close sets it only after cancelling.</summary>
    public ExitCode? Result { get; private set; }

    /// <summary>The one-line header: file, then each side's role and detail.</summary>
    public string HeaderText { get; }

    /// <summary>"1 conflict · 2 changes", plus " · 1 manual" when hand-edited conflicts are not yet marked resolved.</summary>
    public string CounterText
    {
        get
        {
            var counts = _session.Document.Counts;
            var text = $"{Count(counts.UnresolvedConflicts, "conflict")} · {Count(counts.Changes, "change")}";
            return counts.ManualUnacknowledged > 0
                ? $"{text} · {string.Create(CultureInfo.InvariantCulture, $"{counts.ManualUnacknowledged}")} manual"
                : text;
        }
    }

    internal MergeToolSession Session => _session;

    /// <summary>The counter line; its accessible name is its text, so a screen reader reads the counts.</summary>
    internal TextBlock Counter => _counter;

    internal ThreePaneView View { get; }

    internal ConflictNavigator Navigator { get; }

    internal Button AcceptLeftButton { get; }

    internal Button ApplyAllButton { get; }

    internal Button SaveButton { get; }

    internal Button SaveAndExitButton { get; }

    internal Button ExitUnresolvedButton { get; }

    internal Button CancelButton { get; }

    /// <summary>The RESULT caret's zero-based line.</summary>
    internal int ResultCaretLine() => View.Result.CaretLine;

    /// <summary>"Save &amp; exit (0)": the label names the exit code git will see (spec §4.2).</summary>
    private static string FooterLabel(string text, ExitCode code) =>
        string.Create(CultureInfo.InvariantCulture, $"{text} ({(int)code})");

    private static string Count(int number, string noun) =>
        string.Create(CultureInfo.InvariantCulture, $"{number} {noun}{(number == 1 ? string.Empty : "s")}");

    private static string FileName(MergeToolSession session) => session.Inputs.RelativePath?.Value ?? Path.GetFileName(session.Inputs.MergedPath);

    private static string HeaderFor(MergeToolSession session)
    {
        var labels = session.Inputs.Labels;
        static string Side(string name, string role, string? detail) => string.IsNullOrEmpty(detail) ? $"{name} {role}" : $"{name} {role} · {detail}";
        return $"Merging {FileName(session)} · {Side("LEFT", labels.LeftRole, labels.LeftDetail)} · {Side("RIGHT", labels.RightRole, labels.RightDetail)}";
    }

    private static void Place(Grid grid, Control control, int row)
    {
        Grid.SetRow(control, row);
        grid.Children.Add(control);
    }

    private StackPanel BuildHeader(MergeToolSession session)
    {
        var header = new StackPanel { Spacing = 4, Margin = new Thickness(0, 0, 0, Gap) };
        var title = new TextBlock { Text = HeaderText, FontWeight = FontWeight.SemiBold, TextWrapping = TextWrapping.Wrap };
        header.Children.Add(title);
        if (session.Inputs.Labels.Banner is { } banner)
        {
            header.Children.Add(new TextBlock { Text = banner, TextWrapping = TextWrapping.Wrap });
        }
        // An empty stage-1 file is a real (empty) common ancestor; only a missing one means "no common ancestor".
        if (session.Inputs.BaseKind is BaseKind.Absent or BaseKind.AbsentAssumed)
        {
            header.Children.Add(new TextBlock { Text = NoCommonAncestor, TextWrapping = TextWrapping.Wrap });
        }
        if (session.EncodingsDiffer)
        {
            header.Children.Add(BuildEncodingChoice(session));
        }
        header.Children.Add(_counter);
        return header;
    }

    private static StackPanel BuildEncodingChoice(MergeToolSession session)
    {
        var left = new RadioButton { Content = "Save as LEFT's encoding", GroupName = "encoding", IsChecked = session.ResultEncoding == session.Inputs.Left.Encoding };
        var right = new RadioButton { Content = "Save as RIGHT's encoding", GroupName = "encoding", IsChecked = session.ResultEncoding != session.Inputs.Left.Encoding };
        left.IsCheckedChanged += (_, _) =>
        {
            if (left.IsChecked == true)
            {
                session.ResultEncoding = session.Inputs.Left.Encoding;
            }
        };
        right.IsCheckedChanged += (_, _) =>
        {
            if (right.IsChecked == true)
            {
                session.ResultEncoding = session.Inputs.Right.Encoding;
            }
        };
        return new StackPanel { Orientation = Orientation.Horizontal, Spacing = Gap, Children = { left, right } };
    }

    /// <summary>LEFT, RESULT, RIGHT, then the navigator: child order is tab order.</summary>
    private Grid BuildBody()
    {
        var body = new Grid { ColumnDefinitions = new ColumnDefinitions("*,Auto") };
        Navigator.Width = NavigatorWidth;
        Navigator.Margin = new Thickness(Gap, 0, 0, 0);
        Grid.SetColumn(Navigator, 1);
        body.Children.Add(View);
        body.Children.Add(Navigator);
        return body;
    }

    private WrapPanel BuildActionBar(SideLabels labels)
    {
        var bar = new WrapPanel { Margin = new Thickness(0, Gap, 0, 0) };
        _actionBar = bar;
        bar.Children.Add(AcceptLeftButton);
        bar.Children.Add(Action($"Accept RIGHT · {labels.RightRole}", WindowCommand.AcceptRight));
        bar.Children.Add(Action("Both, LEFT first", WindowCommand.BothLeftFirst));
        bar.Children.Add(Action("Both, RIGHT first", WindowCommand.BothRightFirst));
        bar.Children.Add(Action("Keep base", WindowCommand.KeepBase));
        bar.Children.Add(Action("Mark resolved", WindowCommand.AcknowledgeRegion));
        bar.Children.Add(ApplyAllButton);
        return bar;
    }

    private StackPanel BuildFooter() => _footer = new()
    {
        Orientation = Orientation.Horizontal,
        Spacing = Gap,
        Margin = new Thickness(0, Gap, 0, 0),
        HorizontalAlignment = HorizontalAlignment.Right,
        Children = { SaveButton, SaveAndExitButton, ExitUnresolvedButton, CancelButton },
    };

    private Button Action(string text, WindowCommand command)
    {
        var button = new Button { Content = text, Margin = new Thickness(0, 0, Gap, Gap) };
        button.Click += (_, _) => Run(command);
        return button;
    }

    /// <summary>
    /// A footer button. With an access key, the content carries Avalonia's underscore marker so the key is shown
    /// underlined, while the accessible name stays the plain label.
    /// </summary>
    private Button Footer(string text, char? accessKey, WindowCommand command)
    {
        // macOS has no Alt access keys, and Avalonia's would fire on Option+letter, which is typing there:
        // the exits are Cmd+Option chords from the key table instead, and the label carries no marker.
        var style = Keymap.Current;
        var marked = style == KeyStyle.Standard && accessKey is { } key && text.IndexOf(key, StringComparison.OrdinalIgnoreCase) is var at and >= 0;
        var content = marked ? text.Insert(text.IndexOf(accessKey!.Value.ToString(), StringComparison.OrdinalIgnoreCase), "_") : text;
        var button = new Button { Content = content };
        AutomationProperties.SetName(button, text);
        if (accessKey is { } shortcut)
        {
            AutomationProperties.SetAccessKey(button, Keymap.ExitKeyName(shortcut, style));
        }
        button.Click += (_, _) => Run(command);
        return button;
    }
}
