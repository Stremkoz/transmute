namespace Transmute.Core.Models;

public record ConversionResult
{
    public required string InputPath { get; init; }
    public required string OutputPath { get; init; }
    public bool Success { get; init; }
    public string? Error { get; init; }
    public string? BackendUsed { get; init; }
    public TimeSpan Elapsed { get; init; }
    public long? InputBytes { get; init; }
    public long? OutputBytes { get; init; }

    public static ConversionResult Ok(string input, string output, string backend, TimeSpan elapsed) =>
        new() { InputPath = input, OutputPath = output, Success = true, BackendUsed = backend, Elapsed = elapsed };

    public static ConversionResult Fail(string input, string output, string error, string? backend = null) =>
        new() { InputPath = input, OutputPath = output, Success = false, Error = error, BackendUsed = backend };
}
