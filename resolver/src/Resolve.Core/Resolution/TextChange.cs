namespace Resolve.Core.Resolution;

/// <summary>Replace <paramref name="RemovedLength"/> chars at <paramref name="Offset"/> with <paramref name="InsertedText"/>.</summary>
public readonly record struct TextChange(int Offset, int RemovedLength, string InsertedText);
