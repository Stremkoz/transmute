using System.Diagnostics;
using Transmute.Core.Models;

namespace Transmute.Core.Backends;

public class FfmpegBackend : BackendBase
{
    private readonly string? _ffmpegPath;

    // Formats ffmpeg handles as primary backend (video containers + animated)
    public static readonly HashSet<string> VideoContainers = new(StringComparer.OrdinalIgnoreCase)
    {
        "mp4", "mkv", "avi", "mov", "wmv", "flv", "webm", "m4v", "ts", "mpg", "mpeg",
        "3gp", "ogv", "rm", "rmvb", "vob", "mxf", "asf"
    };

    public static readonly HashSet<string> AnimatedFormats = new(StringComparer.OrdinalIgnoreCase)
    {
        "gif", "apng", "webp"
    };

    public static readonly HashSet<string> ReadFormats;
    public static readonly HashSet<string> WriteFormats;

    static FfmpegBackend()
    {
        ReadFormats = [.. VideoContainers, .. AnimatedFormats, "png", "jpg", "jpeg", "bmp", "tiff", "tif"];
        WriteFormats = [.. VideoContainers, .. AnimatedFormats, "png", "jpg", "jpeg", "bmp", "tiff", "tif"];
    }

    public override string Name => "ffmpeg";
    public override bool IsAvailable => _ffmpegPath is not null;
    public override IReadOnlySet<string> SupportedInputFormats => ReadFormats;
    public override IReadOnlySet<string> SupportedOutputFormats => WriteFormats;

    public FfmpegBackend(string? ffmpegPath)
    {
        _ffmpegPath = ffmpegPath;
    }

    public override async Task<ConversionResult> ConvertAsync(ConversionJob job, CancellationToken ct = default)
    {
        var sw = Stopwatch.StartNew();
        var outputExt = Normalize(Path.GetExtension(job.OutputPath));

        var args = new List<string> { "-i", job.InputPath, "-y" };

        if (outputExt is "gif")
        {
            // High-quality GIF: generate palette first, then apply
            args.AddRange(["-vf", "split[s0][s1];[s0]palettegen[p];[s1][p]paletteuse"]);
        }
        else if (outputExt is "jpg" or "jpeg")
        {
            var q = job.Options.Quality ?? 90;
            // Map 0-100 quality to ffmpeg's 2-31 qscale (inverted, 2=best)
            var qscale = Math.Max(2, 33 - (int)(q * 31.0 / 100));
            args.AddRange(["-q:v", qscale.ToString()]);
        }
        else if (outputExt is "png")
        {
            args.AddRange(["-compression_level", "6"]);
        }

        // ffmpeg: StripAll supported; ColorProfile/Copyright degrade to PreserveAll
        // (no selective ICC-only flag exists for still image outputs)
        if (job.Options.Metadata == MetadataMode.StripAll)
            args.AddRange(["-map_metadata", "-1"]);

        args.Add(job.OutputPath);

        var (code, _, stderr) = await RunProcessAsync(_ffmpegPath!, args, ct);
        return BuildResult(job, code, stderr, sw);
    }
}
