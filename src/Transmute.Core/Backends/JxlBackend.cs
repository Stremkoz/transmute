using System.Diagnostics;
using Transmute.Core.Models;

namespace Transmute.Core.Backends;

public class JxlBackend : BackendBase
{
    private readonly string? _cjxlPath;
    private readonly string? _djxlPath;

    public static readonly HashSet<string> ReadFormats = new(StringComparer.OrdinalIgnoreCase)
    {
        "jxl", "png", "jpg", "jpeg", "apng", "gif", "exr", "ppm", "pfm", "pgm", "pgx", "npy"
    };

    public static readonly HashSet<string> WriteFormats = new(StringComparer.OrdinalIgnoreCase)
    {
        "jxl", "png", "jpg", "jpeg", "ppm", "pfm"
    };

    public override string Name => "JPEG XL (cjxl/djxl)";
    public override bool IsAvailable => _cjxlPath is not null && _djxlPath is not null;
    public override IReadOnlySet<string> SupportedInputFormats => ReadFormats;
    public override IReadOnlySet<string> SupportedOutputFormats => WriteFormats;

    public JxlBackend(string? cjxlPath, string? djxlPath)
    {
        _cjxlPath = cjxlPath;
        _djxlPath = djxlPath;
    }

    public override bool CanConvert(string inputExt, string outputExt)
    {
        var input = Normalize(inputExt);
        var output = Normalize(outputExt);
        // Primary purpose: encode to JXL or decode from JXL
        return (output == "jxl" && SupportedInputFormats.Contains(input)) ||
               (input == "jxl" && SupportedOutputFormats.Contains(output));
    }

    public override async Task<ConversionResult> ConvertAsync(ConversionJob job, CancellationToken ct = default)
    {
        var outputExt = Normalize(Path.GetExtension(job.OutputPath));
        var sw = Stopwatch.StartNew();

        if (outputExt == "jxl")
            return await EncodeAsync(job, sw, ct);
        else
            return await DecodeAsync(job, sw, ct);
    }

    private async Task<ConversionResult> EncodeAsync(ConversionJob job, Stopwatch sw, CancellationToken ct)
    {
        // cjxl: -d 0 = lossless, -d <float> = lossy (0.1-1.0 visually lossless, 1.1-2 lossy, higher = worse)
        // --quality maps 0–100 like libjpeg and is mutually exclusive with -d
        var args = new List<string> { job.InputPath, job.OutputPath };

        double distance;
        if (job.Options.Lossless)
        {
            distance = 0;
        }
        else if (job.Options.JxlDistance.HasValue)
        {
            // Explicit distance takes priority over the quality-derived formula.
            distance = job.Options.JxlDistance.Value;
        }
        else
        {
            // Formula from libjxl source for quality 30-100: distance = 0.1 + (100 - q) * 0.09
            //   q=100 → 0.1 (near-lossless), q=90 → 1.0 (visually lossless), q=75 → 2.35, q=50 → 4.6
            var quality = job.Options.Quality ?? 90;
            distance = Math.Round(0.1 + (100 - quality) * 0.09, 2);
        }

        args.AddRange(["-d", distance.ToString("F2", System.Globalization.CultureInfo.InvariantCulture)]);

        var isLossless = distance <= 0;

        if (!isLossless)
        {
            // cjxl defaults to --lossless_jpeg=1 for JPEG inputs (just wraps the bitstream).
            // A non-zero distance conflicts with that default, so force re-encoding for lossy.
            var inputExt = Normalize(Path.GetExtension(job.InputPath));
            if (inputExt is "jpg" or "jpeg")
                args.Add("--lossless_jpeg=0");
        }

        var effort = job.Options.JxlEffort ?? 7;
        args.AddRange(["-e", effort.ToString()]);

        // cjxl preserves all metadata by default; there is no CLI flag to strip it.
        // All MetadataMode values degrade to PreserveAll here (safe — no data loss).
        // Keep invisible pixels (alpha=0) in lossy mode; lossless always keeps them.
        if (!isLossless)
            args.Add("--keep_invisible=1");

        var (code, _, stderr) = await RunProcessAsync(_cjxlPath!, args, ct);
        return BuildResult(job, code, stderr, sw);
    }

    private async Task<ConversionResult> DecodeAsync(ConversionJob job, Stopwatch sw, CancellationToken ct)
    {
        var args = new List<string> { job.InputPath, job.OutputPath };
        var (code, _, stderr) = await RunProcessAsync(_djxlPath!, args, ct);
        return BuildResult(job, code, stderr, sw);
    }
}
