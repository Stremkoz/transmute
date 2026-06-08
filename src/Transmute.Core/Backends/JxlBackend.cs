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
        // cjxl: -d 0 = lossless, -d <float> = lossy (0.5–1.0 visually lossless, higher = worse)
        // --quality maps 0–100 like libjpeg and is mutually exclusive with -d
        var args = new List<string> { job.InputPath, job.OutputPath };

        if (job.Options.Lossless)
        {
            args.AddRange(["-d", "0"]);
        }
        else
        {
            // -d is the primary, universally supported distance flag.
            // Formula from libjxl source for quality 30-100: distance = 0.1 + (100 - q) * 0.09
            //   q=100 → 0.1 (near-lossless), q=90 → 1.0 (visually lossless), q=75 → 2.35, q=50 → 4.6
            var quality = job.Options.Quality ?? 90;
            var distance = Math.Round(0.1 + (100 - quality) * 0.09, 2);
            args.AddRange(["-d", distance.ToString("F2", System.Globalization.CultureInfo.InvariantCulture)]);

            // cjxl defaults to --lossless_jpeg=1 for JPEG inputs (just wraps the bitstream).
            // A non-zero distance conflicts with that default, so force re-encoding for lossy.
            var inputExt = Normalize(Path.GetExtension(job.InputPath));
            if (inputExt is "jpg" or "jpeg")
                args.Add("--lossless_jpeg=0");
        }

        var effort = job.Options.JxlEffort ?? 7;
        args.AddRange(["-e", effort.ToString()]);

        // underscore, not hyphen; only matters for lossy (lossless always keeps invisible pixels)
        if (!job.Options.Lossless)
            args.Add(job.Options.PreserveMetadata ? "--keep_invisible=1" : "--keep_invisible=0");

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
