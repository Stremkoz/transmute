using System.CommandLine;
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

        var jobsOpt = new Option<int>("--jobs", () => 0, "Number of parallel jobs (0 = CPU count)");
        jobsOpt.AddAlias("-j");

        var overwriteOpt = new Option<bool>("--overwrite", "Overwrite existing output files");

        var preserveMetaOpt = new Option<bool>("--preserve-metadata", () => true, "Keep EXIF and other metadata");

        var backendOpt = new Option<string?>("--backend", "Force a specific backend (webp, jxl, ffmpeg, vips, magick)");

        var recursiveOpt = new Option<bool>("--recursive", "Include files from subdirectories");
        recursiveOpt.AddAlias("-r");

        var cmd = new Command("convert", "Convert image(s) to a target format")
        {
            inputsArg, formatOpt, outputOpt, outputDirOpt, qualityOpt,
            jobsOpt, overwriteOpt, preserveMetaOpt, backendOpt, recursiveOpt,
        };

        cmd.SetHandler(async (ctx) =>
        {
            var inputs = ctx.ParseResult.GetValueForArgument(inputsArg);
            var format = ctx.ParseResult.GetValueForOption(formatOpt)!;
            var output = ctx.ParseResult.GetValueForOption(outputOpt);
            var outputDir = ctx.ParseResult.GetValueForOption(outputDirOpt);
            var quality = ctx.ParseResult.GetValueForOption(qualityOpt);
            var jobs = ctx.ParseResult.GetValueForOption(jobsOpt);
            var overwrite = ctx.ParseResult.GetValueForOption(overwriteOpt);
            var preserveMeta = ctx.ParseResult.GetValueForOption(preserveMetaOpt);
            var backend = ctx.ParseResult.GetValueForOption(backendOpt);
            var recursive = ctx.ParseResult.GetValueForOption(recursiveOpt);
            var ct = ctx.GetCancellationToken();

            var config = configManager.Config;
            if (jobs == 0) jobs = config.Processing.MaxParallelJobs;

            var options = new ConversionOptions
            {
                Quality = quality ?? GetDefaultQuality(format, config.Defaults),
                PreserveMetadata = preserveMeta,
                Overwrite = overwrite || config.Defaults.OverwriteExisting,
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
                var allInputs = ExpandInputs(inputs, recursive);
                var conversionJobs = allInputs.Select(f =>
                    new ConversionJob
                    {
                        InputPath = f,
                        OutputPath = engine.ResolveOutputPath(f, format, options),
                        OutputFormat = format,
                        Options = options,
                    }).ToList();

                if (conversionJobs.Count == 0)
                {
                    Console.WriteLine("No input files found.");
                    return;
                }

                Console.WriteLine($"Converting {conversionJobs.Count} file(s) to {format.ToUpperInvariant()}...");

                var progress = new Progress<ConversionProgress>(p =>
                {
                    if (p.LastResult is not null)
                    {
                        var r = p.LastResult;
                        if (r.Success)
                            Console.WriteLine($"  [{p.Completed}/{p.Total}] {Path.GetFileName(r.InputPath)} → {Path.GetFileName(r.OutputPath)} ({r.BackendUsed}, {r.Elapsed.TotalSeconds:F2}s)");
                        else
                            Console.Error.WriteLine($"  [{p.Completed}/{p.Total}] FAILED {Path.GetFileName(r.InputPath)}: {r.Error}");
                    }
                });

                var results = await engine.ConvertAllAsync(conversionJobs, progress, ct);

                var succeeded = results.Count(r => r.Success);
                var failedCount = results.Count(r => !r.Success);
                Console.WriteLine($"\nDone: {succeeded} succeeded, {failedCount} failed.");

                if (failedCount > 0)
                    ctx.ExitCode = 1;
            }
        });

        return cmd;
    }

    private static int? GetDefaultQuality(string format, DefaultsConfig defaults) =>
        format.ToLowerInvariant() switch
        {
            "webp" => defaults.WebpQuality,
            "jpg" or "jpeg" => defaults.JpegQuality,
            "jxl" => defaults.JxlQuality,
            "avif" => defaults.AvifQuality,
            _ => null
        };

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
