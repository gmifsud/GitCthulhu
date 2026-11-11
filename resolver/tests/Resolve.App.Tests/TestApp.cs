using Avalonia;
using Avalonia.Headless;
using Resolve.App.Tests;

[assembly: AvaloniaTestApplication(typeof(TestApp))]

namespace Resolve.App.Tests;

public class TestApp : App
{
    public static AppBuilder BuildAvaloniaApp() =>
        AppBuilder.Configure<TestApp>().UseHeadless(new AvaloniaHeadlessPlatformOptions());
}
