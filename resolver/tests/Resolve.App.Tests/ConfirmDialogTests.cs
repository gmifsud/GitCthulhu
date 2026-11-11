using Avalonia.Controls;
using Avalonia.Headless;
using Avalonia.Headless.XUnit;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.Threading;
using Resolve.App.Shell;

namespace Resolve.App.Tests;

public class ConfirmDialogTests
{
    private static async Task<bool> Answer(Action<ConfirmDialog> respond)
    {
        var owner = new Window { Width = 400, Height = 300 };
        owner.Show();
        var dialog = new ConfirmDialog("Really?");
        var answer = dialog.ShowDialog<bool>(owner);
        respond(dialog);
        return await answer;
    }

    [AvaloniaFact]
    public async Task Yes_returns_true() =>
        Assert.True(await Answer(d => d.YesButton.RaiseEvent(new RoutedEventArgs(Button.ClickEvent))));

    [AvaloniaFact]
    public async Task No_returns_false() =>
        Assert.False(await Answer(d => d.NoButton.RaiseEvent(new RoutedEventArgs(Button.ClickEvent))));

    [AvaloniaFact]
    public async Task Escape_returns_false() =>
        Assert.False(await Answer(d => d.KeyPressQwerty(PhysicalKey.Escape, RawInputModifiers.None)));

    [AvaloniaFact]
    public void Yes_is_the_default_button_and_the_message_is_shown()
    {
        var dialog = new ConfirmDialog("Really?");
        Assert.True(dialog.YesButton.IsDefault);
        Assert.Equal("Really?", dialog.MessageText);
    }

    [AvaloniaFact]
    public async Task ShowAsync_is_the_public_entry_point()
    {
        var owner = new Window { Width = 400, Height = 300 };
        owner.Show();
        var task = ConfirmDialog.ShowAsync(owner, "Really?");
        Assert.False(task.IsCompleted);
        Dispatcher.UIThread.RunJobs();
        owner.OwnedWindows.OfType<ConfirmDialog>().Single().NoButton.RaiseEvent(new RoutedEventArgs(Button.ClickEvent));
        Assert.False(await task);
    }
}
