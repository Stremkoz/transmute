using System.Collections.Concurrent;
using System.CommandLine;
using System.Threading.Channels;
using Transmute.Core;
using Transmute.Core.Config;
using Transmute.Core.Models;
using Transmute.Core.Processing;

namespace Transmute.CLI.Commands;

public static class WatchCommand
{
    private static readonly HashSet<string> ImageExtensions = new(StringComparer.OrdinalIgnoreCase)
    {
        ".jpg", ".jpeg", ".png", ".webp", ".avif", ".jxl", ".tiff", ".tif",
        ".gif", ".bmp", ".heic", ".heif", ".hdr", ".jp2"
    };

    private static readonly HashSet<string> TempExtensions = new(StringComparer.OrdinalIgnoreCase)
    {
        ".tmp", ".part", ".crdownload", ".download", ".partial", ".!ut", ".!bt"
    };

    public static Command Build(ConfigManager configManager, ProfileManager profileManager)
    {
        var folderArg = new Argument<DirectoryInfo>("folder", "Folder to watch for new images");

        var formatOpt = new Option<string>("--format", "Target output format (webp, avif, jxl, png, ...)") { IsRequired = true };
        formatOpt.AddAlias("-f");

        var outputDirOpt = new Option<DirectoryInfo?>("--output-dir", "Output directory (default: same as input)");
        outputDirOpt.AddAlias("-o");

        var recursiveOpt = new Option<bool>("--recursive", "Watch subdirectories");
        recursiveOpt.AddAlias("-r");

        var profileOpt = new Option<string?>("--profile", "Use a named profile for defaults");
        profileOpt.AddAlias("-p");

        var stableTimeOpt = new Option<int>("--stable-time", () => 500,
            "Milliseconds a file must be unchanged before processing (default: 500)");

        var qualityOpt = new Option<int?>("--quality", "Quality 0-100 for lossy formats");
        qualityOpt.AddAlias("-q");

        var distanceOpt = new Option<double?>("--distance",
            "JXL distance 0-2 (0 = lossless, 0.1-1.0 = visually lossless, 1.1-2 = lossy). Overrides --quality for JXL output.");
        distanceOpt.AddAlias("-d");

        var losslessOpt  = new Option<bool>("--lossless", "Lossless encoding (JXL and WebP only)");
        var overwriteOpt = new Option<bool>("--overwrite", "Overwrite existing output files");

        var metadataOpt = new Option<MetadataMode?>("--metadata",
            "Metadata handling: preserve, strip, color, copyright") { ArgumentHelpName = "mode" };
        metadataOpt.AddAlias("--meta");

        var jobsOpt = new Option<int>("--jobs", () => 0, "Max parallel conversions (0 = CPU count)");
        jobsOpt.AddAlias("-j");

        var verboseOpt = new Option<bool>("--verbose", "Show detailed routing decisions per file");
        verboseOpt.AddAlias("-v");

        var cmd = new Command("watch",
            "Watch a folder and automatically convert new images as they appear")
        {
            folderArg, formatOpt, outputDirOpt, recursiveOpt, profileOpt,
            stableTimeOpt, qualityOpt, distanceOpt, losslessOpt, overwriteOpt, metadataOpt,
            jobsOpt, verboseOpt,
        };

        cmd.SetHandler(async ctx =>
        {
            var watchDir    = ctx.ParseResult.GetValueForArgument(folderArg);
            var format      = ctx.ParseResult.GetValueForOption(formatOpt)!;
            var outputDir   = ctx.ParseResult.GetValueForOption(outputDirOpt);
            var recursive   = ctx.ParseResult.GetValueForOption(recursiveOpt);
            var profileName = ctx.ParseResult.GetValueForOption(profileOpt);
            var stableMs    = Math.Max(50, ctx.ParseResult.GetValueForOption(stableTimeOpt));
            var quality     = ctx.ParseResult.GetValueForOption(qualityOpt);
            var distance    = ctx.ParseResult.GetValueForOption(distanceOpt);
            var lossless    = ctx.ParseResult.GetValueForOption(losslessOpt);
            var overwrite   = ctx.ParseResult.GetValueForOption(overwriteOpt);
            var metaMode    = ctx.ParseResult.GetValueForOption(metadataOpt);
            var jobs        = ctx.ParseResult.GetValueForOption(jobsOpt);
            var verbose     = ctx.ParseResult.GetValueForOption(verboseOpt);
            var ct          = ctx.GetCancellationToken();

            var profile = string.IsNullOrEmpty(profileName) ? null : profileManager.Load(profileName);
            if (!string.IsNullOrEmpty(profileName) &&
                !string.Equals(profileName, ProfileManager.DefaultProfileName, StringComparison.OrdinalIgnoreCase) &&
                profile is null)
            {
                Console.Error.WriteLine($"Error: Profile '{profileName}' not found. Run 'transmute profile list' to see available profiles.");
                ctx.ExitCode = 1;
                return;
            }

            var config   = configManager.Config;
            var defaults = profile?.ApplyOver(config.Defaults) ?? config.Defaults;
            if (jobs == 0) jobs = config.Processing.MaxParallelJobs;

            var fmt = format.ToLowerInvariant();
            bool jxlDistanceOverride = distance.HasValue && fmt == "jxl";
            bool effectiveLossless = lossless ||
                (fmt is "jxl" or "webp" && defaults.LosslessDefault && quality is null && !jxlDistanceOverride);

            double? jxlDistance = null;
            if (fmt == "jxl")
            {
                if (distance.HasValue)
                    jxlDistance = distance.Value;
                else if (!effectiveLossless && quality is null)
                    jxlDistance = defaults.JxlDistance;

                if (jxlDistance == 0)
                    effectiveLossless = true;
            }

            var options = new ConversionOptions
            {
                Quality             = quality ?? GetDefaultQuality(format, defaults),
                Lossless            = effectiveLossless,
                WebpMethod          = defaults.WebpMethod,
                JxlEffort           = defaults.JxlEffort,
                JxlDistance         = jxlDistance,
                Metadata            = metaMode ?? defaults.MetadataMode,
                Overwrite           = overwrite || defaults.OverwriteExisting,
                OutputDirectory     = outputDir?.FullName ?? defaults.DefaultOutputDirectory,
                OutputNamingPattern = defaults.OutputNamingPattern,
            };

            if (!string.IsNullOrEmpty(profileName) && profile is not null)
                Console.WriteLine($"Using profile: {profileName}");

            var (engine, _, temp, _) = TransmuteFactory.Create(config);
            using (temp)
            {
                await RunWatchLoopAsync(watchDir.FullName, fmt, options, engine,
                    recursive, stableMs, verbose, jobs, ct);
            }
        });

        return cmd;
    }

    // ── Core watch loop ───────────────────────────────────────────────────────

    private static async Task RunWatchLoopAsync(
        string watchPath, string format, ConversionOptions options,
        ConversionEngine engine, bool recursive, int stableMs,
        bool verbose, int maxJobs, CancellationToken ct)
    {
        int workerCount = maxJobs <= 0 ? Environment.ProcessorCount : maxJobs;

        var processedFiles = new ConcurrentDictionary<string, byte>(StringComparer.OrdinalIgnoreCase);
        var debounceMap    = new ConcurrentDictionary<string, CancellationTokenSource>(StringComparer.OrdinalIgnoreCase);
        var queue          = Channel.CreateUnbounded<string>(new UnboundedChannelOptions { SingleReader = false });

        // Loop prevention: skip files already in target format when output == watch folder
        var resolvedOutput = options.OutputDirectory;
        bool sameFolder = resolvedOutput is null ||
            string.Equals(
                Path.GetFullPath(resolvedOutput).TrimEnd(Path.DirectorySeparatorChar),
                Path.GetFullPath(watchPath).TrimEnd(Path.DirectorySeparatorChar),
                StringComparison.OrdinalIgnoreCase);

        Console.WriteLine($"Watching {watchPath} for new images... (Ctrl+C to stop)");
        if (resolvedOutput is not null)
            Console.WriteLine($"Output: {resolvedOutput}");

        // Worker pool — uses CancellationToken.None so Ctrl+C drains the queue gracefully
        var workers = Enumerable.Range(0, workerCount)
            .Select(_ => Task.Run(async () =>
            {
                await foreach (var path in queue.Reader.ReadAllAsync(CancellationToken.None))
                {
                    await ProcessFileAsync(path, format, options, engine,
                        processedFiles, sameFolder, verbose, CancellationToken.None);
                }
            }))
            .ToArray();

        // ── File event handler ────────────────────────────────────────────────

        void OnFileEvent(string fullPath)
        {
            if (ct.IsCancellationRequested) return;
            if (!ShouldConsider(fullPath, format, sameFolder)) return;
            if (processedFiles.ContainsKey(fullPath)) return;

            // Reset debounce: cancel any previous timer for this path
            if (debounceMap.TryRemove(fullPath, out var oldCts))
            {
                oldCts.Cancel();
                oldCts.Dispose();
            }

            var delayCts = CancellationTokenSource.CreateLinkedTokenSource(ct);
            debounceMap[fullPath] = delayCts;

            _ = Task.Run(async () =>
            {
                try
                {
                    await Task.Delay(stableMs, delayCts.Token);
                    debounceMap.TryRemove(fullPath, out _);

                    if (ct.IsCancellationRequested || processedFiles.ContainsKey(fullPath)) return;

                    await EnqueueWhenStableAsync(fullPath, stableMs, queue.Writer, processedFiles, ct);
                }
                catch (OperationCanceledException) { }
                finally
                {
                    debounceMap.TryRemove(fullPath, out _);
                }
            });
        }

        // ── Watcher lifecycle ─────────────────────────────────────────────────

        FileSystemWatcher? watcher = null;
        bool unavailableLogged    = false;

        void StartWatcher()
        {
            watcher?.Dispose();
            watcher = null;
            try
            {
                watcher = new FileSystemWatcher(watchPath)
                {
                    IncludeSubdirectories = recursive,
                    NotifyFilter          = NotifyFilters.FileName | NotifyFilters.LastWrite,
                    InternalBufferSize    = 65536,
                    EnableRaisingEvents   = true,
                };
                watcher.Created += (_, e) => OnFileEvent(e.FullPath);
                watcher.Changed += (_, e) => OnFileEvent(e.FullPath);
                watcher.Error   += (_, _) => { watcher!.EnableRaisingEvents = false; };
                unavailableLogged = false;
            }
            catch
            {
                watcher?.Dispose();
                watcher = null;
            }
        }

        if (Directory.Exists(watchPath))
        {
            StartWatcher();
        }
        else
        {
            Console.WriteLine("⚠ Watch folder not found, waiting for it to appear...");
            unavailableLogged = true;
        }

        // ── Folder availability monitor ───────────────────────────────────────

        var monitorTask = Task.Run(async () =>
        {
            while (!ct.IsCancellationRequested)
            {
                try { await Task.Delay(2000, ct); }
                catch (OperationCanceledException) { break; }

                bool exists   = Directory.Exists(watchPath);
                bool watching = watcher?.EnableRaisingEvents == true;

                if (!exists && !unavailableLogged)
                {
                    Console.WriteLine("⚠ Watch folder temporarily unavailable, waiting...");
                    unavailableLogged = true;
                    watcher?.Dispose();
                    watcher = null;
                }
                else if (exists && !watching)
                {
                    if (unavailableLogged)
                        Console.WriteLine("✓ Watch folder restored, resuming.");
                    StartWatcher();
                }
            }
        });

        // ── Wait for Ctrl+C ───────────────────────────────────────────────────

        try { await Task.Delay(Timeout.Infinite, ct); }
        catch (OperationCanceledException) { }

        Console.WriteLine();

        // ── Graceful shutdown ─────────────────────────────────────────────────

        watcher?.Dispose();

        foreach (var cts in debounceMap.Values) { cts.Cancel(); cts.Dispose(); }
        debounceMap.Clear();

        // Complete the channel so workers exit their ReadAllAsync loop after draining
        queue.Writer.TryComplete();
        await Task.WhenAll(workers);
        await monitorTask;

        Console.WriteLine("Watch mode stopped.");
    }

    // ── Stability check ───────────────────────────────────────────────────────

    private static async Task EnqueueWhenStableAsync(
        string filePath, int stableMs, ChannelWriter<string> queue,
        ConcurrentDictionary<string, byte> processedFiles, CancellationToken ct)
    {
        const int maxWaitMs  = 30_000;
        const int noticeableMs = 1_000;
        var sw = System.Diagnostics.Stopwatch.StartNew();
        bool waitingLogged = false;

        while (sw.ElapsedMilliseconds < maxWaitMs)
        {
            if (ct.IsCancellationRequested || !File.Exists(filePath)) return;

            DateTime mtime;
            try { mtime = File.GetLastWriteTimeUtc(filePath); }
            catch { return; }

            try { await Task.Delay(stableMs, ct); }
            catch (OperationCanceledException) { return; }

            // Log "waiting" once the stability check has been running for a noticeable time
            if (!waitingLogged && sw.ElapsedMilliseconds >= noticeableMs)
            {
                Console.WriteLine($"→ Detected: {Path.GetFileName(filePath)} (waiting for file to stabilise...)");
                waitingLogged = true;
            }

            DateTime newMtime;
            try { newMtime = File.GetLastWriteTimeUtc(filePath); }
            catch { return; }

            if (newMtime != mtime)
                continue; // Still being written

            // mtime stable — confirm no writer still holds the file open
            try
            {
                using var _ = new FileStream(filePath, FileMode.Open, FileAccess.Read, FileShare.None);
            }
            catch (IOException)
            {
                // Still locked — retry next cycle
                try { await Task.Delay(stableMs, ct); }
                catch (OperationCanceledException) { return; }
                continue;
            }
            catch { return; } // Permission error etc. — skip

            // Atomically claim this path so concurrent events don't double-queue it
            if (!processedFiles.TryAdd(filePath, 0)) return;

            if (!waitingLogged)
                Console.WriteLine($"→ Detected: {Path.GetFileName(filePath)}");

            // Channel is unbounded so this never blocks; CancellationToken.None avoids
            // a race where ct fires in the tiny window after TryAdd
            await queue.WriteAsync(filePath, CancellationToken.None);
            return;
        }

        Console.WriteLine($"⚠ {Path.GetFileName(filePath)} — did not stabilise within {maxWaitMs / 1000}s, skipping");
    }

    // ── Single-file conversion ────────────────────────────────────────────────

    private static async Task ProcessFileAsync(
        string filePath, string format, ConversionOptions options,
        ConversionEngine engine, ConcurrentDictionary<string, byte> processedFiles,
        bool sameFolder, bool verbose, CancellationToken ct)
    {
        if (!File.Exists(filePath))
        {
            Console.WriteLine($"⚠ {Path.GetFileName(filePath)} — disappeared before conversion, skipping");
            return;
        }

        var job = new ConversionJob
        {
            InputPath    = filePath,
            OutputPath   = engine.ResolveOutputPath(filePath, format, options),
            OutputFormat = format,
            Options      = options,
        };

        var progress = new Progress<ConversionProgress>(p =>
        {
            if (p.LastResult is not { } r) return;

            if (verbose)
            {
                Console.WriteLine(FormatVerboseResult(r, options));
                return;
            }

            if (r.Success)
            {
                var size = FormatSizeDelta(r.InputBytes, r.OutputBytes);
                Console.WriteLine($"✓ {Path.GetFileName(r.InputPath)} → {Path.GetFileName(r.OutputPath)} [{r.BackendUsed}]{size} {r.Elapsed.TotalSeconds:F2}s");
            }
            else if (r.Skipped)
            {
                Console.WriteLine($"⊘ {Path.GetFileName(r.InputPath)}: skipped (output already exists)");
            }
            else
            {
                Console.WriteLine($"⚠ {Path.GetFileName(r.InputPath)} — conversion failed, skipping");
            }
        });

        try
        {
            await engine.ConvertAllAsync([job], progress, ct);
        }
        catch (OperationCanceledException) { }
        catch (Exception ex)
        {
            Console.WriteLine($"⚠ {Path.GetFileName(filePath)} — unexpected error: {ex.Message}");
        }
    }

    // ── Filtering ─────────────────────────────────────────────────────────────

    private static bool ShouldConsider(string filePath, string targetFormat, bool sameFolder)
    {
        var name = Path.GetFileName(filePath);
        if (string.IsNullOrEmpty(name)) return false;
        if (name.StartsWith('.')) return false; // hidden / dot-files

        var ext = Path.GetExtension(filePath).ToLowerInvariant();
        if (TempExtensions.Contains(ext)) return false;
        if (!ImageExtensions.Contains(ext)) return false;

        // Prevent output-overwrites-input loops when output dir == watch dir
        if (sameFolder && ext.TrimStart('.').Equals(targetFormat, StringComparison.OrdinalIgnoreCase))
            return false;

        return true;
    }

    // ── Formatting helpers ────────────────────────────────────────────────────

    private static string FormatSizeDelta(long? inputBytes, long? outputBytes)
    {
        if (inputBytes is null || outputBytes is null || inputBytes == 0) return string.Empty;
        var pct  = (outputBytes.Value - inputBytes.Value) * 100.0 / inputBytes.Value;
        var sign = pct < 0 ? "" : "+";
        return $"  {FormatBytes(inputBytes.Value)}→{FormatBytes(outputBytes.Value)} ({sign}{pct:F0}%)";
    }

    private static string FormatBytes(long bytes)
    {
        if (bytes >= 1_048_576) return $"{bytes / 1_048_576.0:F1}MB";
        if (bytes >= 1_024)     return $"{bytes / 1_024.0:F0}KB";
        return $"{bytes}B";
    }

    private static string FormatVerboseResult(ConversionResult r, ConversionOptions options)
    {
        var sb = new System.Text.StringBuilder();
        sb.AppendLine($"── {Path.GetFileName(r.InputPath)} ──");
        sb.AppendLine($"  {"Input:",-10}{r.InputPath}");
        sb.AppendLine($"  {"Output:",-10}{r.OutputPath}");

        if (r.Skipped)
        {
            sb.Append($"  {"Status:",-10}skipped (output already exists)");
            return sb.ToString();
        }
        if (!r.Success)
        {
            sb.Append($"  {"Status:",-10}FAILED — {r.Error}");
            return sb.ToString();
        }

        var backend = r.BackendUsed ?? "unknown";
        if (r.FallbackNote is not null) backend += "  ⚠ fallback";
        sb.AppendLine($"  {"Backend:",-10}{backend}");
        if (r.RoutingReason is not null) sb.AppendLine($"  {"Reason:",-10}{r.RoutingReason}");
        var size = r.InputBytes.HasValue && r.OutputBytes.HasValue
            ? FormatSizeDelta(r.InputBytes, r.OutputBytes)
            : string.Empty;
        sb.Append($"  {"Result:",-10}{r.Elapsed.TotalSeconds:F2}s{size}");

        return sb.ToString();
    }

    private static int? GetDefaultQuality(string format, DefaultsConfig defaults) =>
        format.ToLowerInvariant() switch
        {
            "webp"          => defaults.WebpQuality,
            "jpg" or "jpeg" => defaults.JpegQuality,
            "jxl"           => defaults.JxlQuality,
            "avif"          => defaults.AvifQuality,
            _               => null
        };
}
