using System.Diagnostics;
using NetVips;
using Transmute.Core.Models;

namespace Transmute.Core.Backends;

public class VipsBackend : BackendBase
{
    public static readonly HashSet<string> ReadFormats = new(StringComparer.OrdinalIgnoreCase)
    {
        "jpg", "jpeg", "png", "tiff", "tif", "webp", "avif", "heif", "heic",
        "bmp", "gif", "svg", "pdf", "ppm", "pgm", "pbm", "hdr", "fits",
        "vips", "openslide", "jp2", "j2k"
    };

    public static readonly HashSet<string> WriteFormats = new(StringComparer.OrdinalIgnoreCase)
    {
        "jpg", "jpeg", "png", "tiff", "tif", "webp", "avif", "heif", "heic",
        "gif", "ppm", "pbm", "hdr", "vips", "jp2", "dz", "fits", "matrix",
        "csv", "magick", "openslide", "raw"
    };

    private bool? _available;

    public override string Name => "libvips (NetVips)";
    public override bool IsAvailable
    {
        get
        {
            if (_available.HasValue) return _available.Value;
            try
            {
                _available = ModuleInitializer.VipsInitialized;
            }
            catch
            {
                _available = false;
            }
            return _available.Value;
        }
    }

    public override IReadOnlySet<string> SupportedInputFormats => ReadFormats;
    public override IReadOnlySet<string> SupportedOutputFormats => WriteFormats;

    public override async Task<ConversionResult> ConvertAsync(ConversionJob job, CancellationToken ct = default)
    {
        var sw = Stopwatch.StartNew();
        try
        {
            await Task.Run(() => DoConvert(job), ct);
            sw.Stop();
            return ConversionResult.Ok(job.InputPath, job.OutputPath, Name, sw.Elapsed);
        }
        catch (Exception ex)
        {
            return ConversionResult.Fail(job.InputPath, job.OutputPath, ex.Message, Name);
        }
    }

    private static void DoConvert(ConversionJob job)
    {
        using var image = Image.NewFromFile(job.InputPath, access: Enums.Access.Sequential);
        var outputExt = Normalize(Path.GetExtension(job.OutputPath));
        var outputDir = Path.GetDirectoryName(job.OutputPath);
        if (outputDir is not null)
            Directory.CreateDirectory(outputDir);

        var options = BuildSaveOptions(job, outputExt);
        image.WriteToFile(job.OutputPath, options);
    }

    private static VOption? BuildSaveOptions(ConversionJob job, string ext)
    {
        var q = job.Options.Quality;
        return ext switch
        {
            "jpg" or "jpeg" => new VOption { { "Q", q ?? 90 }, { "keep", job.Options.PreserveMetadata ? Enums.ForeignKeep.All : Enums.ForeignKeep.None } },
            "webp" => new VOption { { "Q", q ?? 85 }, { "keep", job.Options.PreserveMetadata ? Enums.ForeignKeep.All : Enums.ForeignKeep.None } },
            "avif" => new VOption { { "Q", q ?? 80 }, { "keep", job.Options.PreserveMetadata ? Enums.ForeignKeep.All : Enums.ForeignKeep.None } },
            "heif" or "heic" => new VOption { { "Q", q ?? 80 }, { "keep", job.Options.PreserveMetadata ? Enums.ForeignKeep.All : Enums.ForeignKeep.None } },
            "png" => new VOption { { "keep", job.Options.PreserveMetadata ? Enums.ForeignKeep.All : Enums.ForeignKeep.None } },
            "tiff" or "tif" => new VOption { { "keep", job.Options.PreserveMetadata ? Enums.ForeignKeep.All : Enums.ForeignKeep.None } },
            _ => null
        };
    }
}
