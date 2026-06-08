using System.Diagnostics;
using Transmute.Core.Models;

namespace Transmute.Core.Backends;

public class WebPBackend : BackendBase
{
    private readonly string? _cwebpPath;
    private readonly string? _dwebpPath;

    // cwebp accepts these directly without intermediate conversion
    private static readonly HashSet<string> CwebpNativeInputs = ["png", "jpg", "jpeg", "tiff", "tif", "bmp", "ppm", "pgm", "pfm", "pam"];

    public static readonly HashSet<string> ReadFormats = new(StringComparer.OrdinalIgnoreCase) { "webp" };
    public static readonly HashSet<string> WriteFormats = new(StringComparer.OrdinalIgnoreCase) { "webp" };

    public override string Name => "WebP (cwebp/dwebp)";
    public override bool IsAvailable => _cwebpPath is not null && _dwebpPath is not null;
    public override IReadOnlySet<string> SupportedInputFormats => ReadFormats;
    public override IReadOnlySet<string> SupportedOutputFormats => WriteFormats;

    public WebPBackend(string? cwebpPath, string? dwebpPath)
    {
        _cwebpPath = cwebpPath;
        _dwebpPath = dwebpPath;
    }

    public bool CanEncodeDirectly(string inputExt) =>
        CwebpNativeInputs.Contains(Normalize(inputExt));

    public override async Task<ConversionResult> ConvertAsync(ConversionJob job, CancellationToken ct = default)
    {
        var inputExt = Normalize(Path.GetExtension(job.InputPath));
        var outputExt = Normalize(Path.GetExtension(job.OutputPath));
        var sw = Stopwatch.StartNew();

        if (outputExt == "webp")
            return await EncodeAsync(job, sw, ct);
        else if (inputExt == "webp")
            return await DecodeAsync(job, sw, ct);

        return ConversionResult.Fail(job.InputPath, job.OutputPath,
            $"WebPBackend cannot convert {inputExt} to {outputExt}", Name);
    }

    private async Task<ConversionResult> EncodeAsync(ConversionJob job, Stopwatch sw, CancellationToken ct)
    {
        var args = new List<string>();

        if (job.Options.Lossless)
        {
            args.Add("-lossless");
        }
        else
        {
            var quality = job.Options.Quality ?? 85;
            args.AddRange(["-q", quality.ToString()]);
        }

        var method = job.Options.WebpMethod ?? 6;
        args.AddRange(["-m", method.ToString(), job.InputPath, "-o", job.OutputPath]);

        // cwebp: -metadata all|none|icc|exif,xmp
        var metaFlag = job.Options.Metadata switch
        {
            MetadataMode.StripAll     => "none",
            MetadataMode.ColorProfile => "icc",
            MetadataMode.Copyright    => "exif,xmp",
            _                         => "all",
        };
        args.AddRange(["-metadata", metaFlag]);

        var (code, _, stderr) = await RunProcessAsync(_cwebpPath!, args, ct);
        return BuildResult(job, code, stderr, sw);
    }

    private async Task<ConversionResult> DecodeAsync(ConversionJob job, Stopwatch sw, CancellationToken ct)
    {
        var outputExt = Normalize(Path.GetExtension(job.OutputPath));
        // dwebp can output png, pam, ppm, pgm, yuv
        // For other targets use the intermediate path (set by router)
        var actualOutput = outputExt == "png" ? job.OutputPath : (job.IntermediatePath ?? job.OutputPath);

        // dwebp is decode-only; it has no metadata control flags — all metadata passes through as-is
        var args = new List<string> { job.InputPath, "-o", actualOutput };

        var (code, _, stderr) = await RunProcessAsync(_dwebpPath!, args, ct);
        return BuildResult(job, code, stderr, sw);
    }
}
