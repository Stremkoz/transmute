using System.Text.Json;
using System.Text.Json.Serialization;
using Transmute.Core.Models;

namespace Transmute.Core.Processing;

public static class LogWriter
{
    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        WriteIndented = true,
        DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull,
    };

    /// <summary>
    /// Writes a conversion log file alongside the output images.
    /// The file is placed in <paramref name="outputDirectory"/> (or beside the first output file
    /// if no explicit output dir was set).
    /// </summary>
    public static string Write(
        IReadOnlyList<ConversionResult> results,
        string outputDirectory,
        string format,
        TimeSpan elapsed)
    {
        Directory.CreateDirectory(outputDirectory);

        var timestamp = DateTime.Now.ToString("yyyyMMdd-HHmmss");
        var ext = string.Equals(format, "json", StringComparison.OrdinalIgnoreCase) ? "json" : "log";
        var path = Path.Combine(outputDirectory, $"transmute-{timestamp}.{ext}");

        if (ext == "json")
            WriteJson(results, path, elapsed);
        else
            WriteText(results, path, elapsed);

        return path;
    }

    private static void WriteText(IReadOnlyList<ConversionResult> results, string path, TimeSpan elapsed)
    {
        var succeeded = results.Count(r => r.Success);
        var skipped   = results.Count(r => r.Skipped);
        var failed    = results.Count(r => !r.Success && !r.Skipped);

        using var w = new StreamWriter(path, append: false, System.Text.Encoding.UTF8);
        w.WriteLine("Transmute Conversion Log");
        w.WriteLine($"Generated : {DateTime.Now:yyyy-MM-dd HH:mm:ss}");
        w.WriteLine($"Duration  : {elapsed.TotalSeconds:F1}s");
        w.WriteLine($"Results   : {results.Count} file(s)");
        w.WriteLine();

        foreach (var r in results)
        {
            if (r.Success)
            {
                var sizePart = FormatSizeDelta(r.InputBytes, r.OutputBytes);
                w.WriteLine($"  ✓ {r.InputPath}");
                w.WriteLine($"    → {r.OutputPath}  [{r.BackendUsed}]{sizePart}  {r.Elapsed.TotalSeconds:F2}s");
            }
            else if (r.Skipped)
            {
                w.WriteLine($"  ⊘ {r.InputPath}");
                w.WriteLine($"    → skipped (output already exists)");
            }
            else
            {
                w.WriteLine($"  ✗ {r.InputPath}");
                w.WriteLine($"    → failed: {r.Error}");
            }
        }

        w.WriteLine();
        w.WriteLine($"Summary: {succeeded} succeeded, {skipped} skipped, {failed} failed");

        var fallbackGroups = results
            .Where(r => r.FallbackNote is not null)
            .GroupBy(r => r.FallbackNote!)
            .ToList();

        if (fallbackGroups.Count > 0)
        {
            w.WriteLine();
            foreach (var group in fallbackGroups)
                w.WriteLine($"⚠ {group.Count()} file(s): {group.Key}");
        }
    }

    private static void WriteJson(IReadOnlyList<ConversionResult> results, string path, TimeSpan elapsed)
    {
        var succeeded = results.Count(r => r.Success);
        var skipped   = results.Count(r => r.Skipped);
        var failed    = results.Count(r => !r.Success && !r.Skipped);

        var doc = new
        {
            generated       = DateTime.Now.ToString("yyyy-MM-ddTHH:mm:ss"),
            durationSeconds = Math.Round(elapsed.TotalSeconds, 2),
            results         = results.Select(r => new
            {
                input          = r.InputPath,
                output         = r.OutputPath,
                status         = r.Success ? "success" : r.Skipped ? "skipped" : "failed",
                backend        = r.BackendUsed,
                routingReason  = r.RoutingReason,
                fallbackNote   = r.FallbackNote,
                inputBytes     = r.InputBytes,
                outputBytes    = r.OutputBytes,
                elapsedSeconds = r.Success ? (double?)Math.Round(r.Elapsed.TotalSeconds, 3) : null,
                error          = r.Error,
            }).ToArray(),
            summary = new { succeeded, skipped, failed },
        };

        var json = JsonSerializer.Serialize(doc, JsonOptions);
        File.WriteAllText(path, json, System.Text.Encoding.UTF8);
    }

    private static string FormatSizeDelta(long? inputBytes, long? outputBytes)
    {
        if (inputBytes is null || outputBytes is null || inputBytes == 0) return string.Empty;
        var inMb  = inputBytes.Value  / (1024.0 * 1024);
        var outMb = outputBytes.Value / (1024.0 * 1024);
        var pct   = (outputBytes.Value - inputBytes.Value) * 100.0 / inputBytes.Value;
        var sign  = pct < 0 ? "" : "+";
        return $"  {inMb:F1}MB→{outMb:F1}MB ({sign}{pct:F0}%)";
    }
}
