using System.CommandLine;
using System.Diagnostics;
using Transmute.Core;
using Transmute.Core.Config;
using Transmute.Core.Models;

namespace Transmute.CLI.Commands;

public static class ConvertCommand
{
    public static Command Build(ConfigManager configManager)
    {
        var inputsArg = new Argument<string[]>("inputs", "Input file(s) or folder(s) to convert")
        {
            Arity = ArgumentArity.OneOrMore,
        };

        var formatOpt = new Option<string>("--format", "Target output format (e.g. webp, avif, jxl, png)") { IsRequired = true };
        formatOpt.AddAlias("-f");

        var outputOpt = new Option<string?>("--output", "Output file path (single input only)");
        outputOpt.AddAlias("-o");

        var outputDirOpt = new Option<DirectoryInfo?>("--output-dir", "Output directory for converted files");

        var qualityOpt = new Option<int?>("--quality", "Quality 0-100 for lossy formats");
        qualityOpt.AddAlias("-q");

        var losslessOpt = new Option<bool>("--lossless", "Lossless encoding (JXL and WebP only)");
        losslessOpt.AddAlias("-l");

        var methodOpt = new Option<int?>("--method", "WebP compression method 0-6 (default from config, usually 6)");

        var effortOpt = new Option<int?>("--effort", "JXL effort level 1-9 (default from config, usually 7)");
        effortOpt.AddAlias("-e");

        var jobsOpt = new Option<int>("--jobs", () => 0, "Number of parallel jobs (0 = CPU count)");
        jobsOpt.AddAlias("-j");

        var overwriteOpt = new Option<bool>("--overwrite", "Overwrite existing output files");

        var preserveMetaOpt = new Option<bool>("--preserve-metadata", () => true, "Keep EXIF and other metadata");

        var backendOpt = new Option<string?>("--backend", "Force a specific backend (webp, jxl, ffmpeg, vips, magick)");

        var recursiveOpt = new Option<bool>("--recursive", "Include files from subdirectories");
        recursiveOpt.AddAlias("-r");

        var cmd = new Command("convert", "Convert image(s) to a target format")
        {
            inputsArg, formatOpt, outputOpt, outputDirOpt, qualityOpt, losslessOpt,
            methodOpt, effortOpt, jobsOpt, overwriteOpt, preserveMetaOpt, backendOpt, recursiveOpt,
        };

        cmd.SetHandler(async (ctx) =>
        {
            var inputs   = ctx.ParseResult.GetValueForArgument(inputsArg);
            var format   = ctx.ParseResult.GetValueForOption(formatOpt)!;
            var output   = ctx.ParseResult.GetValueForOption(outputOpt);
            var outputDir = ctx.ParseResult.GetValueForOption(outputDirOpt);
            var quality  = ctx.ParseResult.GetValueForOption(qualityOpt);
            var lossless = ctx.ParseResult.GetValueForOption(losslessOpt);
            var method   = ctx.ParseResult.GetValueForOption(methodOpt);
            var effort   = ctx.ParseResult.GetValueForOption(effortOpt);
            var jobs     = ctx.ParseResult.GetValueForOption(jobsOpt);
            var overwrite = ctx.ParseResult.GetValueForOption(overwriteOpt);
            var preserveMeta = ctx.ParseResult.GetValueForOption(preserveMetaOpt);
            var backend  = ctx.ParseResult.GetValueForOption(backendOpt);
            var recursive = ctx.ParseResult.GetValueForOption(recursiveOpt);
            var ct = ctx.GetCancellationToken();

            var config = configManager.Config;
            if (jobs == 0) jobs = config.Processing.MaxParallelJobs;

            // --lossless flag overrides the config default; without it, use the config default
            // for formats that support lossless (jxl, webp), or false for everything else.
            var fmt = format.ToLowerInvariant();
            bool effectiveLossless = lossless
                ? true
                : (fmt is "jxl" or "webp") && config.Defaults.LosslessDefault;

            var options = new ConversionOptions
            {
                Quality    = quality ?? GetDefaultQuality(format, config.Defaults),
                Lossless   = effectiveLossless,
                WebpMethod = method ?? config.Defaults.WebpMethod,
                JxlEffort  = effort ?? config.Defaults.JxlEffort,
                PreserveMetadata = preserveMeta,
                Overwrite  = overwrite || config.Defaults.OverwriteExisting,
                OutputDirectory = outputDir?.FullName,
                OutputFile = output,
                ForcedBackend = backend,
                MaxParallelJobs = jobs,
                OutputNamingPattern = config.Defaults.OutputNamingPattern,
            };

            if (output is not null && inputs.Length > 1)
            {
                Console.Error.WriteLine("Error: --output can only be used with a single input file.");
                ctx.ExitCode = 1;
                return;
            }

            var (engine, _, temp, _) = TransmuteFactory.Create(config);
            using (temp)
            {
                var allInputs = ExpandInputs(inputs, recursive).ToList();
                var conversionJobs = allInputs
                    .Select((f, i) => new ConversionJob
                    {
                        InputPath    = f,
                        OutputPath   = engine.ResolveOutputPath(f, format, options, i + 1, allInputs.Count),
                        OutputFormat = format,
                        Options      = options,
                    }).ToList();

                if (conversionJobs.Count == 0)
                {
                    Console.WriteLine("No input files found.");
                    return;
                }

                var modeLabel = effectiveLossless ? "lossless" : $"q{options.Quality}";
                Console.WriteLine($"Converting {conversionJobs.Count} file(s) to {format.ToUpperInvariant()} ({modeLabel})...");

                var totalSw = Stopwatch.StartNew();

                var progress = new Progress<ConversionProgress>(p =>
                {
                    if (p.LastResult is not { } r) return;
                    if (r.Success)
                    {
                        var sizePart = FormatSizeDelta(r.InputBytes, r.OutputBytes);
                        Console.WriteLine($"  [{p.Completed}/{p.Total}] {Path.GetFileName(r.InputPath)} → {Path.GetFileName(r.OutputPath)} [{r.BackendUsed}]{sizePart} {r.Elapsed.TotalSeconds:F2}s");
                    }
                    else
                    {
                        Console.Error.WriteLine($"  [{p.Completed}/{p.Total}] FAILED {Path.GetFileName(r.InputPath)}: {r.Error}");
                    }
                });

                var results = await engine.ConvertAllAsync(conversionJobs, progress, ct);
                totalSw.Stop();

                var succeeded  = results.Where(r => r.Success).ToList();
                var failedCount = results.Count(r => !r.Success);

                var totalIn  = succeeded.Sum(r => r.InputBytes ?? 0);
                var totalOut = succeeded.Sum(r => r.OutputBytes ?? 0);
                var totalSize = totalIn > 0 ? $"  {FormatSizeDelta(totalIn, totalOut).Trim()}" : string.Empty;

                Console.WriteLine($"\nDone: {succeeded.Count} succeeded, {failedCount} failed — {totalSw.Elapsed.TotalSeconds:F1}s{totalSize}");

                if (failedCount > 0)
                    ctx.ExitCode = 1;
            }
        });

        return cmd;
    }

    private static int? GetDefaultQuality(string format, DefaultsConfig defaults) =>
        format.ToLowerInvariant() switch
        {
            "webp"       => defaults.WebpQuality,
            "jpg" or "jpeg" => defaults.JpegQuality,
            "jxl"        => defaults.JxlQuality,
            "avif"       => defaults.AvifQuality,
            _            => null
        };

    private static string FormatSizeDelta(long? inputBytes, long? outputBytes)
    {
        if (inputBytes is null || outputBytes is null || inputBytes == 0) return string.Empty;
        var inMb  = inputBytes.Value  / (1024.0 * 1024);
        var outMb = outputBytes.Value / (1024.0 * 1024);
        var pct   = (outputBytes.Value - inputBytes.Value) * 100.0 / inputBytes.Value;
        var sign  = pct < 0 ? "" : "+";
        return $"  {inMb:F1}MB→{outMb:F1}MB ({sign}{pct:F0}%)";
    }

    private static readonly HashSet<string> ImageExtensions = new(StringComparer.OrdinalIgnoreCase)
    {
        ".jpg", ".jpeg", ".png", ".webp", ".avif", ".jxl", ".tiff", ".tif",
        ".gif", ".bmp", ".heic", ".heif", ".svg", ".hdr", ".jp2", ".j2k"
    };

    private static IEnumerable<string> ExpandInputs(string[] inputs, bool recursive)
    {
        foreach (var input in inputs)
        {
            if (File.Exists(input))
            {
                yield return input;
            }
            else if (Directory.Exists(input))
            {
                var option = recursive
                    ? SearchOption.AllDirectories
                    : SearchOption.TopDirectoryOnly;
                foreach (var file in Directory.EnumerateFiles(input, "*", option))
                {
                    if (ImageExtensions.Contains(Path.GetExtension(file)))
                        yield return file;
                }
            }
            else
            {
                Console.Error.WriteLine($"Warning: '{input}' not found, skipping.");
            }
        }
    }
}
