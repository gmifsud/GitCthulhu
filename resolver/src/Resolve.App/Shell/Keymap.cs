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
}

/// <summary>The key table as pure data: modifiers must match exactly, so Ctrl+Shift+Z is Redo and never Undo.</summary>
public static class Keymap
{
    private const KeyModifiers CtrlAlt = KeyModifiers.Control | KeyModifiers.Alt;

    public static WindowCommand? For(Key key, KeyModifiers modifiers) => (key, modifiers) switch
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
        _ => null,
    };
}
