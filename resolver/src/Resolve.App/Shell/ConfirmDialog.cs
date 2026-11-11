using Avalonia;
using Avalonia.Automation;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Layout;
using Avalonia.Media;

namespace Resolve.App.Shell;

/// <summary>A small modal question with Yes (the default) and No; Escape and the title-bar close both answer No.</summary>
public sealed class ConfirmDialog : Window
{
    private const double Gap = 12;

    public ConfirmDialog(string message)
    {
        ArgumentNullException.ThrowIfNull(message);
        MessageText = message;
        Title = "Cthulhu Resolve";
        Width = 460;
        SizeToContent = SizeToContent.Height;
        CanResize = false;
        WindowStartupLocation = WindowStartupLocation.CenterOwner;
        ShowInTaskbar = false;

        var text = new TextBlock { Text = message, TextWrapping = TextWrapping.Wrap };
        AutomationProperties.SetName(this, message);

        YesButton = new Button { Content = "Yes", IsDefault = true, MinWidth = 80 };
        NoButton = new Button { Content = "No", IsCancel = true, MinWidth = 80 };
        YesButton.Click += (_, _) => Close(true);
        NoButton.Click += (_, _) => Close(false);

        var buttons = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = Gap,
            HorizontalAlignment = HorizontalAlignment.Right,
            Children = { YesButton, NoButton },
        };
        Content = new StackPanel { Margin = new Thickness(Gap * 2), Spacing = Gap * 2, Children = { text, buttons } };
        AddHandler(KeyDownEvent, OnKeyDown, Avalonia.Interactivity.RoutingStrategies.Tunnel);
    }

    internal string MessageText { get; }

    internal Button YesButton { get; }

    internal Button NoButton { get; }

    /// <summary>Shows the question over <paramref name="owner"/> and returns true only when the user answered Yes.</summary>
    public static Task<bool> ShowAsync(Window owner, string message)
    {
        ArgumentNullException.ThrowIfNull(owner);
        return new ConfirmDialog(message).ShowDialog<bool>(owner);
    }

    private void OnKeyDown(object? sender, KeyEventArgs e)
    {
        if (e.Key == Key.Escape)
        {
            e.Handled = true;
            Close(false);
        }
    }
}
