using System.Diagnostics;
using Transmute.Core.Models;

namespace Transmute.Core.Backends;

public class MagickBackend : BackendBase
{
    private readonly string? _magickPath;

    // ImageMagick is a broad fallback — declare wide support
    public static readonly HashSet<string> BroadFormats = new(StringComparer.OrdinalIgnoreCase)
    {
        "jpg", "jpeg", "png", "tiff", "tif", "bmp", "gif", "webp", "avif", "heif", "heic",
        "ico", "svg", "eps", "pdf", "psd", "xcf", "dds", "tga", "pcx", "xbm", "xpm",
        "wbmp", "jng", "mng", "sgi", "sun", "viff", "xwd", "yuv", "dcm", "miff",
        "hdr", "exr", "fits", "pnm", "ppm", "pgm", "pbm", "jfif", "jp2", "j2k",
        "cin", "dpx", "iff", "palm", "pict", "ras", "pct", "fpx", "cur"
    };

    public override string Name => "ImageMagick";
    public override bool IsAvailable => _magickPath is not null;
    public override IReadOnlySet<string> SupportedInputFormats => BroadFormats;
    public override IReadOnlySet<string> SupportedOutputFormats => BroadFormats;

    public MagickBackend(string? magickPath)
    {
        _magickPath = magickPath;
    }

    public override async Task<ConversionResult> ConvertAsync(ConversionJob job, CancellationToken ct = default)
    {
        var sw = Stopwatch.StartNew();
        var outputExt = Normalize(Path.GetExtension(job.OutputPath));

        var args = new List<string> { "convert", job.InputPath };

        if (job.Options.Quality.HasValue)
            args.AddRange(["-quality", job.Options.Quality.Value.ToString()]);

        if (!job.Options.PreserveMetadata)
            args.AddRange(["-strip"]);

        args.Add(job.OutputPath);

        var (code, _, stderr) = await RunProcessAsync(_magickPath!, args, ct);
        return BuildResult(job, code, stderr, sw);
    }
}
