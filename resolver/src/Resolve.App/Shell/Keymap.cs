using Avalonia.Input;

namespace Resolve.App.Shell;

/// <summary>What a key press asks the merge-tool window to do (spec §4.6, Phase 1 subset).</summary>
public enum WindowCommand
{
    NextConflict,
    PreviousConflict,
    NextChange,
    PreviousChange,
    AcceptLeft,
    AcceptRight,
    BothLeftFirst,
    BothRightFirst,
    KeepBase,
    ResetRegion,
    AcknowledgeRegion,
    Undo,
    Redo,
    Save,
    ApplyAllNonConflicting,
    SaveAndExit,
    ExitUnresolved,
    Cancel,
}

/// <summary>Which keyboard conventions the window follows.</summary>
public enum KeyStyle
{
    /// <summary>Windows and Linux: Ctrl for commands, Alt for the footer's access keys.</summary>
    Standard,
    /// <summary>
    /// macOS: Cmd where the others use Ctrl, and Cmd+Option for the exits. Option+letter types a character there
    /// (ß, ¨, ç), so Option+S/U/C must stay typing and never end the session.
    /// </summary>
    Mac,
}

/// <summary>The key table as pure data: modifiers must match exactly, so Ctrl+Shift+Z is Redo and never Undo.</summary>
public static class Keymap
{
    private const KeyModifiers CtrlAlt = KeyModifiers.Control | KeyModifiers.Alt;

    /// <summary>The conventions of the platform the window runs on.</summary>
    public static KeyStyle Current => OperatingSystem.IsMacOS() ? KeyStyle.Mac : KeyStyle.Standard;

    /// <summary>The footer exit on <paramref name="letter"/>, named as the platform writes it.</summary>
    public static string ExitKeyName(char letter, KeyStyle style) =>
        style == KeyStyle.Mac ? $"Cmd+Option+{char.ToUpperInvariant(letter)}" : $"Alt+{char.ToUpperInvariant(letter)}";

    /// <summary>The command for a key in the window's own conventions (<see cref="Current"/>).</summary>
    public static WindowCommand? For(Key key, KeyModifiers modifiers) => For(key, modifiers, Current);

    /// <summary>
    /// The command for a key under <paramref name="style"/>. A macOS chord is read as its standard counterpart:
    /// Cmd as Ctrl, Cmd+Option as Alt. Ctrl+Option (the region commands) is free on macOS and means the same there.
    /// On macOS a chord with Ctrl alone or Option alone is never a command: Option+letter is typing.
    /// </summary>
    public static WindowCommand? For(Key key, KeyModifiers modifiers, KeyStyle style)
    {
        if (style == KeyStyle.Mac)
        {
            var standard = AsStandard(modifiers);
            return standard is { } translated ? Standard(key, translated) : null;
        }
        return Standard(key, modifiers);
    }

    private static KeyModifiers? AsStandard(KeyModifiers mac)
    {
        const KeyModifiers command = KeyModifiers.Meta;
        if (mac == CtrlAlt)
        {
            return CtrlAlt;
        }
        if (mac == (command | KeyModifiers.Alt))
        {
            return KeyModifiers.Alt;
        }
        var other = mac & ~(command | KeyModifiers.Shift);
        if (mac.HasFlag(command) && other == KeyModifiers.None)
        {
            return (mac & ~command) | KeyModifiers.Control;
        }
        return other == KeyModifiers.None ? mac : null;
    }

    private static WindowCommand? Standard(Key key, KeyModifiers modifiers) => (key, modifiers) switch
    {
        (Key.F8, KeyModifiers.None) => WindowCommand.NextConflict,
        (Key.F8, KeyModifiers.Shift) => WindowCommand.PreviousConflict,
        (Key.F7, KeyModifiers.None) => WindowCommand.NextChange,
        (Key.F7, KeyModifiers.Shift) => WindowCommand.PreviousChange,
        (Key.Left, CtrlAlt) => WindowCommand.AcceptLeft,
        (Key.Right, CtrlAlt) => WindowCommand.AcceptRight,
        (Key.Down, CtrlAlt) => WindowCommand.BothLeftFirst,
        (Key.Up, CtrlAlt) => WindowCommand.BothRightFirst,
        (Key.Back, CtrlAlt) => WindowCommand.KeepBase,
        (Key.R, CtrlAlt) => WindowCommand.ResetRegion,
        (Key.Enter, KeyModifiers.Control) => WindowCommand.AcknowledgeRegion,
        (Key.Z, KeyModifiers.Control) => WindowCommand.Undo,
        (Key.Y, KeyModifiers.Control) => WindowCommand.Redo,
        (Key.Z, KeyModifiers.Control | KeyModifiers.Shift) => WindowCommand.Redo,
        (Key.S, KeyModifiers.Control) => WindowCommand.Save,
        (Key.S, KeyModifiers.Alt) => WindowCommand.SaveAndExit,      // ruling R29: the footer's access keys
        (Key.U, KeyModifiers.Alt) => WindowCommand.ExitUnresolved,
        (Key.C, KeyModifiers.Alt) => WindowCommand.Cancel,
        _ => null,
    };
}
