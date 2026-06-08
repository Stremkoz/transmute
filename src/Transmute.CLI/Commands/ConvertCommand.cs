using System.CommandLine;
using System.Diagnostics;
using Transmute.Core;
using Transmute.Core.Config;
using Transmute.Core.Models;
using Transmute.Core.Processing;

namespace Transmute.CLI.Commands;

public static class ConvertCommand
{
    public static Command Build(ConfigManager configManager, ProfileManager profileManager)
    {
        var inputsArg = new Argument<string[]>("inputs",
            "Input file(s) or folder(s) to convert. Pass '-' to read paths from stdin (one per line).")
        {
            Arity = ArgumentArity.OneOrMore,
        };

        var formatOpt = new Option<string>("--format", "Target output format (e.g. webp, avif, jxl, png)") { IsRequired = true };
        formatOpt.AddAlias("-f");

        var outputOpt = new Option<string?>("--output", "Output file path (single input only)");
        outputOpt.AddAlias("-o");

        var outputDirOpt = new Option<DirectoryInfo?>("--output-dir", "Output directory (overrides config/profile default)");

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

        var profileOpt = new Option<string?>("--profile", "Use a named profile for defaults (overridden by other flags)");
        profileOpt.AddAlias("-p");

        var skipOpt = new Option<string[]>("--skip",
            "Skip input files with these extensions, e.g. --skip jpg,png or --skip jpg --skip png. Replaces any profile skip/only filter.")
        {
            Arity = ArgumentArity.ZeroOrMore,
            AllowMultipleArgumentsPerToken = true,
        };

        var onlyOpt = new Option<string[]>("--only",
            "Process ONLY these extensions, e.g. --only jpg,png. Replaces any profile skip/only filter.")
        {
            Arity = ArgumentArity.ZeroOrMore,
            AllowMultipleArgumentsPerToken = true,
        };

        var namePatternOpt = new Option<string?>("--name-pattern",
            "Output filename pattern, e.g. '{name}-converted.{ext}'. Overrides config/profile setting.");
        namePatternOpt.AddAlias("-n");

        var logOpt   = new Option<bool>("--log",    "Write a log file after conversion, even when disabled in config");
        var noLogOpt = new Option<bool>("--no-log", "Skip the log file, even when enabled in config");

        var logFormatOpt = new Option<string?>("--log-format", "Log format: 'text' (default) or 'json'");

        var dryRunOpt = new Option<bool>("--dry-run", "Preview which files would be converted and their output paths, without converting");

        var cmd = new Command("convert", "Convert image(s) to a target format")
        {
            inputsArg, formatOpt, outputOpt, outputDirOpt, qualityOpt, losslessOpt,
            methodOpt, effortOpt, jobsOpt, overwriteOpt, preserveMetaOpt, backendOpt,
            recursiveOpt, profileOpt, skipOpt, onlyOpt, namePatternOpt,
            logOpt, noLogOpt, logFormatOpt, dryRunOpt,
        };

        cmd.SetHandler(async (ctx) =>
        {
            var inputs       = ctx.ParseResult.GetValueForArgument(inputsArg);
            var format       = ctx.ParseResult.GetValueForOption(formatOpt)!;
            var output       = ctx.ParseResult.GetValueForOption(outputOpt);
            var outputDir    = ctx.ParseResult.GetValueForOption(outputDirOpt);
            var quality      = ctx.ParseResult.GetValueForOption(qualityOpt);
            var lossless     = ctx.ParseResult.GetValueForOption(losslessOpt);
            var method       = ctx.ParseResult.GetValueForOption(methodOpt);
            var effort       = ctx.ParseResult.GetValueForOption(effortOpt);
            var jobs         = ctx.ParseResult.GetValueForOption(jobsOpt);
            var overwrite    = ctx.ParseResult.GetValueForOption(overwriteOpt);
            var preserveMeta = ctx.ParseResult.GetValueForOption(preserveMetaOpt);
            var backend      = ctx.ParseResult.GetValueForOption(backendOpt);
            var recursive    = ctx.ParseResult.GetValueForOption(recursiveOpt);
            var profileName  = ctx.ParseResult.GetValueForOption(profileOpt);
            var skipRaw      = ctx.ParseResult.GetValueForOption(skipOpt);
            var onlyRaw      = ctx.ParseResult.GetValueForOption(onlyOpt);
            var namePattern  = ctx.ParseResult.GetValueForOption(namePatternOpt);
            var logFlag      = ctx.ParseResult.GetValueForOption(logOpt);
            var noLogFlag    = ctx.ParseResult.GetValueForOption(noLogOpt);
            var logFormat    = ctx.ParseResult.GetValueForOption(logFormatOpt);
            var dryRun       = ctx.ParseResult.GetValueForOption(dryRunOpt);
            var ct           = ctx.GetCancellationToken();

            // Normalize skip/only token lists — each token may itself be comma-separated
            var skipTokens = ParseFormatTokens(skipRaw);
            var onlyTokens = ParseFormatTokens(onlyRaw);

            // Load profile (null = Default = use global config as-is)
            var profile = string.IsNullOrEmpty(profileName)
                ? null
                : profileManager.Load(profileName);

            if (!string.IsNullOrEmpty(profileName) &&
                !string.Equals(profileName, ProfileManager.DefaultProfileName, StringComparison.OrdinalIgnoreCase) &&
                profile is null)
            {
                Console.Error.WriteLine($"Error: Profile '{profileName}' not found. Run 'transmute profile list' to see available profiles.");
                ctx.ExitCode = 1;
                return;
            }

            // Effective defaults = global config, with profile overrides layered on top
            var config   = configManager.Config;
            var defaults = profile?.ApplyOver(config.Defaults) ?? config.Defaults;

            if (jobs == 0) jobs = config.Processing.MaxParallelJobs;

            // Determine effective format filter.
            // CLI --skip/--only fully replace profile filters (no merging).
            HashSet<string>? onlyFormats = null;
            HashSet<string> skipFormats  = [];

            if (onlyTokens.Count > 0)
            {
                onlyFormats = onlyTokens;
                Console.Error.WriteLine($"Format filter: only processing {string.Join(", ", onlyFormats)}.");
            }
            else if (skipTokens.Count > 0)
            {
                skipFormats = skipTokens;
            }
            else if (profile is not null)
            {
                if (profile.HasOnlyFilter)
                {
                    onlyFormats = profile.OnlyFormats
                        .Select(s => s.TrimStart('.').ToLowerInvariant())
                        .ToHashSet(StringComparer.OrdinalIgnoreCase);
                    Console.Error.WriteLine($"Profile filter active: only processing {string.Join(", ", onlyFormats)}.");
                }
                else if (profile.HasSkipFilter)
                {
                    skipFormats = profile.SkipFormats
                        .Select(s => s.TrimStart('.').ToLowerInvariant())
                        .ToHashSet(StringComparer.OrdinalIgnoreCase);
                }
            }

            var fmt = format.ToLowerInvariant();
            bool effectiveLossless = lossless
                ? true
                : (fmt is "jxl" or "webp") && defaults.LosslessDefault;

            var options = new ConversionOptions
            {
                Quality             = quality ?? GetDefaultQuality(format, defaults),
                Lossless            = effectiveLossless,
                WebpMethod          = method ?? defaults.WebpMethod,
                JxlEffort           = effort ?? defaults.JxlEffort,
                PreserveMetadata    = preserveMeta,
                Overwrite           = overwrite || defaults.OverwriteExisting,
                OutputDirectory     = outputDir?.FullName ?? defaults.DefaultOutputDirectory,
                OutputFile          = output,
                ForcedBackend       = backend,
                MaxParallelJobs     = jobs,
                OutputNamingPattern = namePattern ?? defaults.OutputNamingPattern,
            };

            if (output is not null && inputs.Length > 1)
            {
                Console.Error.WriteLine("Error: --output can only be used with a single input file.");
                ctx.ExitCode = 1;
                return;
            }

            if (!string.IsNullOrEmpty(profileName) && profile is not null)
                Console.WriteLine($"Using profile: {profileName}");

            var (engine, _, temp, _) = TransmuteFactory.Create(config);
            using (temp)
            {
                var allInputs = ExpandInputs(inputs, recursive, skipFormats, onlyFormats).ToList();

                if (allInputs.Count == 0)
                {
                    Console.WriteLine("No input files found.");
                    return;
                }

                // Warn about same-format inputs
                var sameFormat = allInputs
                    .Where(p => Path.GetExtension(p).TrimStart('.').Equals(fmt, StringComparison.OrdinalIgnoreCase))
                    .ToList();

                if (sameFormat.Count > 0 && !options.Overwrite)
                {
                    Console.Error.WriteLine(
                        $"Note: {sameFormat.Count} file(s) are already {fmt.ToUpperInvariant()} and will be skipped " +
                        "(pass --overwrite to re-encode them).");
                }

                var conversionJobs = allInputs
                    .Select((f, i) => new ConversionJob
                    {
                        InputPath    = f,
                        OutputPath   = engine.ResolveOutputPath(f, format, options, i + 1, allInputs.Count),
                        OutputFormat = format,
                        Options      = options,
                    }).ToList();

                // ── Dry run — show plan and exit ──────────────────────────────────────
                if (dryRun)
                {
                    var modeLabel = effectiveLossless ? "lossless" : $"q{options.Quality}";
                    Console.WriteLine($"Dry run — {conversionJobs.Count} file(s) would be converted to {fmt.ToUpperInvariant()} ({modeLabel}):");
                    Console.WriteLine();

                    var inputColWidth = conversionJobs.Max(j => Path.GetFileName(j.InputPath).Length);
                    foreach (var job in conversionJobs)
                    {
                        var inName  = Path.GetFileName(job.InputPath).PadRight(inputColWidth);
                        var outName = Path.GetFileName(job.OutputPath);
                        Console.WriteLine($"  {inName}  →  {outName}");
                        if (job.InputPath != Path.GetDirectoryName(job.InputPath) + Path.DirectorySeparatorChar + Path.GetFileName(job.InputPath))
                            Console.WriteLine($"    {job.InputPath}");
                    }

                    Console.WriteLine();
                    if (namePattern is not null)
                        Console.WriteLine($"  Name pattern : {namePattern}");
                    if (options.OutputDirectory is not null)
                        Console.WriteLine($"  Output dir   : {options.OutputDirectory}");
                    Console.WriteLine($"  Overwrite    : {options.Overwrite}");
                    return;
                }

                // ── Actual conversion ─────────────────────────────────────────────────
                var modeStr = effectiveLossless ? "lossless" : $"q{options.Quality}";
                Console.WriteLine($"Converting {conversionJobs.Count} file(s) to {format.ToUpperInvariant()} ({modeStr})...");

                var totalSw = Stopwatch.StartNew();
                var progress = new Progress<ConversionProgress>(p =>
                {
                    if (p.LastResult is not { } r) return;

                    if (r.Success)
                    {
                        var sizePart = FormatSizeDelta(r.InputBytes, r.OutputBytes);
                        Console.WriteLine($"  [{p.Completed}/{p.Total}] {Path.GetFileName(r.InputPath)} → {Path.GetFileName(r.OutputPath)} [{r.BackendUsed}]{sizePart} {r.Elapsed.TotalSeconds:F2}s");
                    }
                    else if (r.Skipped)
                    {
                        Console.WriteLine($"  [{p.Completed}/{p.Total}] SKIPPED {Path.GetFileName(r.InputPath)} (output already exists)");
                    }
                    else
                    {
                        Console.Error.WriteLine($"  [{p.Completed}/{p.Total}] FAILED {Path.GetFileName(r.InputPath)}: {r.Error}");
                    }
                });

                var results = await engine.ConvertAllAsync(conversionJobs, progress, ct);
                totalSw.Stop();

                var succeeded    = results.Where(r => r.Success).ToList();
                var skippedCount = results.Count(r => r.Skipped);
                var failedCount  = results.Count(r => !r.Success && !r.Skipped);

                var totalIn   = succeeded.Sum(r => r.InputBytes ?? 0);
                var totalOut  = succeeded.Sum(r => r.OutputBytes ?? 0);
                var totalSize = totalIn > 0 ? $"  {FormatSizeDelta(totalIn, totalOut).Trim()}" : string.Empty;

                var skippedPart = skippedCount > 0 ? $", {skippedCount} skipped" : string.Empty;
                Console.WriteLine($"\nDone: {succeeded.Count} succeeded{skippedPart}, {failedCount} failed — {totalSw.Elapsed.TotalSeconds:F1}s{totalSize}");

                // Write log file — --log enables, --no-log suppresses, otherwise use config
                bool writeLog = noLogFlag ? false : (logFlag ? true : config.Log.Enabled);
                if (writeLog && results.Count > 0)
                {
                    try
                    {
                        var effectiveFormat = logFormat ?? config.Log.Format;
                        var logDir = options.OutputDirectory
                            ?? Path.GetDirectoryName(results[0].OutputPath)
                            ?? ".";
                        var logPath = LogWriter.Write(results, logDir, effectiveFormat, totalSw.Elapsed);
                        Console.WriteLine($"Log written: {logPath}");
                    }
                    catch (Exception ex)
                    {
                        Console.Error.WriteLine($"Warning: could not write log file: {ex.Message}");
                    }
                }

                if (failedCount > 0)
                    ctx.ExitCode = 1;
            }
        });

        return cmd;
    }

    // Parses a token array where each token may be comma-separated (e.g. "jpg,png" or "jpg" "png")
    private static HashSet<string> ParseFormatTokens(string[]? tokens) =>
        tokens?
            .SelectMany(t => t.Split(',', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries))
            .Select(s => s.TrimStart('.').ToLowerInvariant())
            .ToHashSet(StringComparer.OrdinalIgnoreCase)
        ?? [];

    private static int? GetDefaultQuality(string format, DefaultsConfig defaults) =>
        format.ToLowerInvariant() switch
        {
            "webp"          => defaults.WebpQuality,
            "jpg" or "jpeg" => defaults.JpegQuality,
            "jxl"           => defaults.JxlQuality,
            "avif"          => defaults.AvifQuality,
            _               => null
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

    private static IEnumerable<string> ExpandInputs(
        string[] inputs, bool recursive,
        HashSet<string> skipFormats,
        HashSet<string>? onlyFormats)
    {
        foreach (var input in inputs)
        {
            // "-" means read paths from stdin, one per line
            if (input == "-")
            {
                string? line;
                while ((line = Console.In.ReadLine()) is not null)
                {
                    line = line.Trim();
                    if (string.IsNullOrEmpty(line)) continue;

                    if (File.Exists(line))
                    {
                        var ext = Path.GetExtension(line).TrimStart('.').ToLowerInvariant();
                        if (IsAllowed(ext, skipFormats, onlyFormats))
                            yield return line;
                    }
                    else if (Directory.Exists(line))
                    {
                        foreach (var f in EnumerateFolder(line, recursive, skipFormats, onlyFormats))
                            yield return f;
                    }
                    else
                    {
                        Console.Error.WriteLine($"Warning: '{line}' not found, skipping.");
                    }
                }
                continue;
            }

            if (File.Exists(input))
            {
                var ext = Path.GetExtension(input).TrimStart('.').ToLowerInvariant();
                if (IsAllowed(ext, skipFormats, onlyFormats))
                    yield return input;
            }
            else if (Directory.Exists(input))
            {
                foreach (var f in EnumerateFolder(input, recursive, skipFormats, onlyFormats))
                    yield return f;
            }
            else
            {
                Console.Error.WriteLine($"Warning: '{input}' not found, skipping.");
            }
        }
    }

    private static IEnumerable<string> EnumerateFolder(
        string folder, bool recursive,
        HashSet<string> skipFormats, HashSet<string>? onlyFormats)
    {
        var option = recursive ? SearchOption.AllDirectories : SearchOption.TopDirectoryOnly;
        foreach (var file in Directory.EnumerateFiles(folder, "*", option))
        {
            var ext = Path.GetExtension(file).TrimStart('.').ToLowerInvariant();
            if (ImageExtensions.Contains($".{ext}") && IsAllowed(ext, skipFormats, onlyFormats))
                yield return file;
        }
    }

    private static bool IsAllowed(string ext, HashSet<string> skipFormats, HashSet<string>? onlyFormats)
    {
        if (onlyFormats is not null)
            return onlyFormats.Contains(ext);
        return !skipFormats.Contains(ext);
    }
}
