using Avalonia;
using Avalonia.Markup.Xaml.Styling;
using Avalonia.Styling;
using Avalonia.Themes.Fluent;

namespace Resolve.App;

/// <summary>Application shell: themes only. The window and lifetime arrive with the window task.</summary>
public class App : Application
{
    public override void Initialize()
    {
        Styles.Add(new FluentTheme());
        // AvaloniaEdit's TextEditor has no control template until its own theme is included.
        Styles.Add(new StyleInclude(new Uri("avares://cthulhu-resolve/"))
        {
            Source = new Uri("avares://AvaloniaEdit/Themes/Fluent/AvaloniaEdit.xaml"),
        });
        RequestedThemeVariant = ThemeVariant.Dark;
    }
}
