using Avalonia.Input;
using Resolve.App.Shell;

namespace Resolve.App.Tests.Support;

/// <summary>The window's chords as the platform the tests run on presses them (<see cref="Keymap.Current"/>).</summary>
internal static class Keys
{
    /// <summary>Ctrl, or Cmd on macOS.</summary>
    public static RawInputModifiers Command => Keymap.Current == KeyStyle.Mac ? RawInputModifiers.Meta : RawInputModifiers.Control;

    /// <summary>The footer exits' modifier: Alt, or Cmd+Option on macOS.</summary>
    public static RawInputModifiers Exit =>
        Keymap.Current == KeyStyle.Mac ? RawInputModifiers.Meta | RawInputModifiers.Alt : RawInputModifiers.Alt;
}
