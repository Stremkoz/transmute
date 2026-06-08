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

        // Determine primary affinities
        var inputAffinity = FormatRegistry.GetAffinity(inputExt);
        var outputAffinity = FormatRegistry.GetAffinity(outputExt);

        // JXL has highest priority when either side is JXL
        if (inputExt == "jxl" || outputExt == "jxl")
        {
            if (_jxl.CanConvert(inputExt, outputExt) && _jxl.IsAvailable)
                return SingleStep(job, _jxl);

            // JXL as intermediate step
            if (outputExt == "jxl" && _jxl.IsAvailable)
            {
                // Encode to JXL: convert input to PNG first, then cjxl
                return TwoStep(job, inputExt, "png", _vips, _jxl);
            }
            if (inputExt == "jxl" && _jxl.IsAvailable)
            {
                // Decode from JXL to PNG, then convert PNG to output
                return TwoStep(job, "jxl", "png", _jxl, PickBackend("png", outputExt));
            }
        }

        // WebP priority
        if (inputExt == "webp" || outputExt == "webp")
        {
            if (outputExt == "webp" && _webp.IsAvailable)
            {
                if (_webp.CanEncodeDirectly(inputExt))
                    return SingleStep(job, _webp);

                // Need intermediate: decode input to PNG, then cwebp
                return TwoStep(job, inputExt, "png", PickBackend(inputExt, "png"), _webp);
            }

            if (inputExt == "webp" && _webp.IsAvailable)
            {
                // dwebp decodes to PNG natively; if output is PNG we're done
                if (outputExt == "png")
                    return SingleStep(job, _webp);

                // dwebp → PNG temp, then convert PNG → output
                return TwoStep(job, "webp", "png", _webp, PickBackend("png", outputExt));
            }
        }

        // Ffmpeg priority for animated/video
        if (FormatRegistry.IsVideoContainer(inputExt) || FormatRegistry.IsVideoContainer(outputExt) ||
            FormatRegistry.IsAnimated(inputExt) || FormatRegistry.IsAnimated(outputExt))
        {
            if (_ffmpeg.IsAvailable)
                return SingleStep(job, _ffmpeg);
        }

        // General vips routing
        var bestBackend = PickBackend(inputExt, outputExt);

        if (bestBackend.CanConvert(inputExt, outputExt) && bestBackend.IsAvailable)
            return SingleStep(job, bestBackend);

        // Two-step through PNG for anything that didn't match
        var firstBackend = PickBackend(inputExt, "png");
        var secondBackend = PickBackend("png", outputExt);
        return TwoStep(job, inputExt, "png", firstBackend, secondBackend);
    }

    private IBackend PickBackend(string inputExt, string outputExt)
    {
        var inputAffinity = FormatRegistry.GetAffinity(inputExt);
        var outputAffinity = FormatRegistry.GetAffinity(outputExt);

        // Prefer output affinity for encoding decisions
        var affinity = outputAffinity != BackendAffinity.Magick ? outputAffinity : inputAffinity;

        return affinity switch
        {
            BackendAffinity.Jxl when _jxl.IsAvailable => _jxl,
            BackendAffinity.WebP when _webp.IsAvailable => _webp,
            BackendAffinity.Ffmpeg when _ffmpeg.IsAvailable => _ffmpeg,
            BackendAffinity.Vips when _vips.IsAvailable => _vips,
            _ => _magick.IsAvailable ? _magick : _vips // last resort
        };
    }

    private ConversionPlan SingleStep(ConversionJob job, IBackend backend) =>
        new() { PrimaryBackend = backend, PrimaryJob = job };

    private ConversionPlan TwoStep(ConversionJob originalJob, string fromExt, string intermediateExt,
        IBackend step1Backend, IBackend step2Backend)
    {
        var tempPath = _temp.GetTempPath(intermediateExt);

        var step1Job = new ConversionJob
        {
            InputPath = originalJob.InputPath,
            OutputPath = tempPath,
            OutputFormat = intermediateExt,
            Options = originalJob.Options with { Overwrite = true },
        };

        var step2Job = new ConversionJob
        {
            InputPath = tempPath,
            OutputPath = originalJob.OutputPath,
            OutputFormat = originalJob.OutputFormat,
            Options = originalJob.Options,
        };

        return new ConversionPlan
        {
            PrimaryBackend = step1Backend,
            PrimaryJob = step1Job,
            SecondaryBackend = step2Backend,
            SecondaryJob = step2Job,
        };
    }

    private ConversionPlan BuildForcedPlan(ConversionJob job)
    {
        IBackend backend = job.Options.ForcedBackend!.ToLowerInvariant() switch
        {
            "webp" => _webp,
            "jxl" => _jxl,
            "ffmpeg" => _ffmpeg,
            "vips" or "libvips" => _vips,
            "magick" or "imagemagick" => _magick,
            _ => throw new ArgumentException($"Unknown backend: {job.Options.ForcedBackend}")
        };

        return SingleStep(job, backend);
    }

    private static string Normalize(string ext) => ext.TrimStart('.').ToLowerInvariant();

    public IEnumerable<IBackend> AllBackends() =>
        [_jxl, _webp, _ffmpeg, _vips, _magick];
}
