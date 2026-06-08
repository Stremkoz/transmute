using Transmute.Core.Backends;
using Transmute.Core.Models;
using Transmute.Core.Processing;

namespace Transmute.Core.Routing;

public class ConversionPlan
{
    public required IBackend PrimaryBackend { get; init; }
    public required ConversionJob PrimaryJob { get; init; }
    public IBackend? SecondaryBackend { get; init; }
    public ConversionJob? SecondaryJob { get; init; }
    public bool IsTwoStep => SecondaryBackend is not null;

    // Routing diagnostics propagated to ConversionResult
    public string RoutingReason { get; init; } = string.Empty;
    public string? FallbackNote { get; init; }
}

public class FormatRouter
{
    private readonly JxlBackend _jxl;
    private readonly WebPBackend _webp;
    private readonly FfmpegBackend _ffmpeg;
    private readonly VipsBackend _vips;
    private readonly MagickBackend _magick;
    private readonly TempFileManager _temp;

    public FormatRouter(
        JxlBackend jxl,
        WebPBackend webp,
        FfmpegBackend ffmpeg,
        VipsBackend vips,
        MagickBackend magick,
        TempFileManager temp)
    {
        _jxl = jxl;
        _webp = webp;
        _ffmpeg = ffmpeg;
        _vips = vips;
        _magick = magick;
        _temp = temp;
    }

    public ConversionPlan Route(ConversionJob job)
    {
        if (job.Options.ForcedBackend is not null)
            return BuildForcedPlan(job);

        var inputExt = Normalize(Path.GetExtension(job.InputPath));
        var outputExt = Normalize(Path.GetExtension(job.OutputPath));

        // JXL has highest priority when either side is JXL
        if (inputExt == "jxl" || outputExt == "jxl")
        {
            if (_jxl.CanConvert(inputExt, outputExt) && _jxl.IsAvailable)
            {
                var reason = outputExt == "jxl"
                    ? $"cjxl — preferred JPEG XL encoder (.{inputExt} accepted directly)"
                    : $"djxl — preferred JPEG XL decoder (.{outputExt} output supported directly)";
                return SingleStep(job, _jxl, reason);
            }

            if (outputExt == "jxl" && _jxl.IsAvailable)
            {
                // Need intermediate: input → PNG first, then cjxl
                var (step1, _, step1Reason) = PickBackendWithNote(inputExt, "png");
                return TwoStep(job, inputExt, "png", step1, _jxl,
                    $"two-step: {step1Reason} → cjxl encode (via .png intermediate)");
            }

            if (inputExt == "jxl" && _jxl.IsAvailable)
            {
                // djxl → PNG temp, then re-encode
                var (step2, step2Note, step2Reason) = PickBackendWithNote("png", outputExt);
                return TwoStep(job, "jxl", "png", _jxl, step2,
                    $"two-step: djxl decode → {step2Reason} (via .png intermediate)",
                    step2Note);
            }
            // jxl backend unavailable — falls through to general routing
        }

        // WebP priority
        if (inputExt == "webp" || outputExt == "webp")
        {
            if (outputExt == "webp" && _webp.IsAvailable)
            {
                if (_webp.CanEncodeDirectly(inputExt))
                    return SingleStep(job, _webp,
                        $"cwebp — preferred WebP encoder (.{inputExt} accepted directly)");

                // Need intermediate: decode input to PNG, then cwebp
                var (step1, step1Note, step1Reason) = PickBackendWithNote(inputExt, "png");
                return TwoStep(job, inputExt, "png", step1, _webp,
                    $"two-step: {step1Reason} → cwebp encode (via .png intermediate)",
                    step1Note);
            }

            if (inputExt == "webp" && _webp.IsAvailable)
            {
                if (outputExt == "png")
                    return SingleStep(job, _webp,
                        "dwebp — preferred WebP decoder (native .png output)");

                // dwebp → PNG temp, then re-encode
                var (step2, step2Note, step2Reason) = PickBackendWithNote("png", outputExt);
                return TwoStep(job, "webp", "png", _webp, step2,
                    $"two-step: dwebp decode → {step2Reason} (via .png intermediate)",
                    step2Note);
            }
            // webp backend unavailable — falls through
        }

        // Ffmpeg priority for animated/video
        if (FormatRegistry.IsVideoContainer(inputExt) || FormatRegistry.IsVideoContainer(outputExt) ||
            FormatRegistry.IsAnimated(inputExt) || FormatRegistry.IsAnimated(outputExt))
        {
            if (_ffmpeg.IsAvailable)
                return SingleStep(job, _ffmpeg,
                    $"ffmpeg — preferred for animated/video formats (.{inputExt} → .{outputExt})");
            // ffmpeg unavailable — falls through
        }

        // General routing via affinity
        var (bestBackend, bestNote, bestReason) = PickBackendWithNote(inputExt, outputExt);

        if (bestBackend.CanConvert(inputExt, outputExt) && bestBackend.IsAvailable)
            return SingleStep(job, bestBackend, bestReason, bestNote);

        // Two-step through PNG for anything that didn't match
        var (fb1, fn1, fr1) = PickBackendWithNote(inputExt, "png");
        var (fb2, fn2, fr2) = PickBackendWithNote("png", outputExt);
        return TwoStep(job, inputExt, "png", fb1, fb2,
            $"two-step: {fr1} → {fr2} (via .png intermediate)",
            fn1 ?? fn2);
    }

    // Returns (backend, fallbackNote, routingReason).
    // fallbackNote is non-null when the preferred backend was unavailable.
    private (IBackend backend, string? fallbackNote, string reason) PickBackendWithNote(
        string inputExt, string outputExt)
    {
        var outputAffinity = FormatRegistry.GetAffinity(outputExt);
        var inputAffinity  = FormatRegistry.GetAffinity(inputExt);
        var affinity = outputAffinity != BackendAffinity.Magick ? outputAffinity : inputAffinity;

        IBackend? preferred = affinity switch
        {
            BackendAffinity.Jxl    when _jxl.IsAvailable    => _jxl,
            BackendAffinity.WebP   when _webp.IsAvailable   => _webp,
            BackendAffinity.Ffmpeg when _ffmpeg.IsAvailable => _ffmpeg,
            BackendAffinity.Vips   when _vips.IsAvailable   => _vips,
            BackendAffinity.Magick when _magick.IsAvailable => _magick,
            _ => null,
        };

        if (preferred is not null)
            return (preferred, null, $"{preferred.Name} — preferred for .{inputExt} → .{outputExt}");

        // Preferred backend unavailable — fall back
        IBackend fallback = _magick.IsAvailable ? _magick : _vips;
        var preferredName = affinity switch
        {
            BackendAffinity.Jxl    => "cjxl",
            BackendAffinity.WebP   => "cwebp",
            BackendAffinity.Ffmpeg => "ffmpeg",
            BackendAffinity.Vips   => "libvips",
            _                      => "ImageMagick",
        };
        var note   = $"{fallback.Name} used ({preferredName} unavailable for .{inputExt} → .{outputExt})";
        var reason = $"{preferredName} unavailable for .{inputExt} → .{outputExt} — {fallback.Name} used as fallback";
        return (fallback, note, reason);
    }

    // Legacy wrapper used by PickBackend calls that don't need the note
    private IBackend PickBackend(string inputExt, string outputExt) =>
        PickBackendWithNote(inputExt, outputExt).backend;

    private ConversionPlan SingleStep(ConversionJob job, IBackend backend,
        string reason, string? fallbackNote = null) =>
        new()
        {
            PrimaryBackend = backend,
            PrimaryJob     = job,
            RoutingReason  = reason,
            FallbackNote   = fallbackNote,
        };

    private ConversionPlan TwoStep(
        ConversionJob originalJob,
        string fromExt, string intermediateExt,
        IBackend step1Backend, IBackend step2Backend,
        string reason, string? fallbackNote = null)
    {
        var tempPath = _temp.GetTempPath(intermediateExt);

        var step1Job = new ConversionJob
        {
            InputPath    = originalJob.InputPath,
            OutputPath   = tempPath,
            OutputFormat = intermediateExt,
            Options      = originalJob.Options with { Overwrite = true },
        };

        var step2Job = new ConversionJob
        {
            InputPath    = tempPath,
            OutputPath   = originalJob.OutputPath,
            OutputFormat = originalJob.OutputFormat,
            Options      = originalJob.Options,
        };

        return new ConversionPlan
        {
            PrimaryBackend   = step1Backend,
            PrimaryJob       = step1Job,
            SecondaryBackend = step2Backend,
            SecondaryJob     = step2Job,
            RoutingReason    = reason,
            FallbackNote     = fallbackNote,
        };
    }

    private ConversionPlan BuildForcedPlan(ConversionJob job)
    {
        IBackend backend = job.Options.ForcedBackend!.ToLowerInvariant() switch
        {
            "webp"                  => _webp,
            "jxl"                   => _jxl,
            "ffmpeg"                => _ffmpeg,
            "vips" or "libvips"     => _vips,
            "magick" or "imagemagick" => _magick,
            _ => throw new ArgumentException($"Unknown backend: {job.Options.ForcedBackend}")
        };

        return SingleStep(job, backend,
            $"{backend.Name} — forced via --backend {job.Options.ForcedBackend}");
    }

    private static string Normalize(string ext) => ext.TrimStart('.').ToLowerInvariant();

    public IEnumerable<IBackend> AllBackends() =>
        [_jxl, _webp, _ffmpeg, _vips, _magick];
}
