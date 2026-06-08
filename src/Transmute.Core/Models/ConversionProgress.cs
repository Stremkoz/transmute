namespace Transmute.Core.Models;

public class ConversionProgress
{
    public int Total { get; init; }
    public int Completed { get; init; }
    public int Failed { get; init; }
    public string? CurrentFile { get; init; }
    public ConversionResult? LastResult { get; init; }
}
