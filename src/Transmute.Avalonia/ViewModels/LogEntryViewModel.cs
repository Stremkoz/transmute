namespace Transmute.Avalonia.ViewModels;

public enum LogEntryKind
{
    Success,
    Skipped,
    Error,
    Info,
}

/// <summary>A single line in the conversion log, with a kind for colour coding and an optional
/// associated file path for the "Open containing folder" / "Copy file path" context menu actions.</summary>
public sealed class LogEntryViewModel
{
    public string Text { get; }
    public LogEntryKind Kind { get; }
    public string? FilePath { get; }

    public bool IsSuccess => Kind == LogEntryKind.Success;
    public bool IsSkipped => Kind == LogEntryKind.Skipped;
    public bool IsError => Kind == LogEntryKind.Error;
    public bool IsInfo => Kind == LogEntryKind.Info;

    public LogEntryViewModel(string text, LogEntryKind kind, string? filePath = null)
    {
        Text = text;
        Kind = kind;
        FilePath = filePath;
    }
}
