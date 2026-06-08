namespace Transmute.Core.Models;

public class ConversionJob
{
    public required string InputPath { get; init; }
    public required string OutputPath { get; init; }
    public required string OutputFormat { get; init; }
    public ConversionOptions Options { get; init; } = new();
    public string? IntermediatePath { get; set; } // set by router for two-step conversions
}
