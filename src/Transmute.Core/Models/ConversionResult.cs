namespace Transmute.Core.Models;

public record ConversionResult
{
    public required string InputPath { get; init; }
    public required string OutputPath { get; init; }
    public bool Success { get; init; }
    public bool Skipped { get; init; }
    public string? Error { get; init; }
    public string? BackendUsed { get; init; }
    public TimeSpan Elapsed { get; init; }
    public long? InputBytes { get; init; }
    public long? OutputBytes { get; init; }

    // Routing diagnostics — null for normal/forced/skipped results
    public string? RoutingReason { get; init; }
    public string? FallbackNote { get; init; }

    public static ConversionResult Ok(string input, string output, string backend, TimeSpan elapsed) =>
        new() { InputPath = input, OutputPath = output, Success = true, BackendUsed = backend, Elapsed = elapsed };

    public static ConversionResult Fail(string input, string output, string error, string? backend = null) =>
        new() { InputPath = input, OutputPath = output, Success = false, Error = error, BackendUsed = backend };

    public static ConversionResult Skip(string input, string output) =>
        new() { InputPath = input, OutputPath = output, Success = false, Skipped = true };
}
