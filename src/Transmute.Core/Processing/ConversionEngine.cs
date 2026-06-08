using System.Threading.Channels;
using Transmute.Core.Models;
using Transmute.Core.Routing;

namespace Transmute.Core.Processing;

public class ConversionEngine
{
    private readonly FormatRouter _router;
    private readonly int _maxParallelJobs;

    public ConversionEngine(FormatRouter router, int maxParallelJobs = 0)
    {
        _router = router;
        _maxParallelJobs = maxParallelJobs <= 0 ? Environment.ProcessorCount : maxParallelJobs;
    }

    public async Task<IReadOnlyList<ConversionResult>> ConvertAllAsync(
        IEnumerable<ConversionJob> jobs,
        IProgress<ConversionProgress>? progress = null,
        CancellationToken ct = default)
    {
        var jobList = jobs.ToList();
        var results = new ConversionResult[jobList.Count];
        var completed = 0;
        var failed = 0;
        var skipped = 0;

        var channel = Channel.CreateUnbounded<(int Index, ConversionJob Job)>();

        // Produce
        foreach (var (i, job) in jobList.Select((j, i) => (i, j)))
            await channel.Writer.WriteAsync((i, job), ct);
        channel.Writer.Complete();

        var sem = new SemaphoreSlim(_maxParallelJobs);
        var workers = new List<Task>();

        await foreach (var (index, job) in channel.Reader.ReadAllAsync(ct))
        {
            await sem.WaitAsync(ct);

            workers.Add(Task.Run(async () =>
            {
                try
                {
                    progress?.Report(new ConversionProgress
                    {
                        Total = jobList.Count,
                        Completed = completed,
                        Failed = failed,
                        Skipped = skipped,
                        CurrentFile = Path.GetFileName(job.InputPath),
                    });

                    var result = await ExecuteJobAsync(job, ct);
                    results[index] = result;

                    Interlocked.Increment(ref completed);
                    if (result.Skipped) Interlocked.Increment(ref skipped);
                    else if (!result.Success) Interlocked.Increment(ref failed);

                    progress?.Report(new ConversionProgress
                    {
                        Total = jobList.Count,
                        Completed = completed,
                        Failed = failed,
                        Skipped = skipped,
                        CurrentFile = Path.GetFileName(job.InputPath),
                        LastResult = result,
                    });
                }
                finally
                {
                    sem.Release();
                }
            }, ct));
        }

        await Task.WhenAll(workers);
        return results;
    }

    private async Task<ConversionResult> ExecuteJobAsync(ConversionJob job, CancellationToken ct)
    {
        try
        {
            if (!File.Exists(job.InputPath))
                return ConversionResult.Fail(job.InputPath, job.OutputPath, "Input file not found");

            if (!job.Options.Overwrite && File.Exists(job.OutputPath))
                return ConversionResult.Skip(job.InputPath, job.OutputPath);

            var outputDir = Path.GetDirectoryName(job.OutputPath);
            if (outputDir is not null)
                Directory.CreateDirectory(outputDir);

            var plan = _router.Route(job);

            if (!plan.PrimaryBackend.IsAvailable)
                return ConversionResult.Fail(job.InputPath, job.OutputPath,
                    $"Backend '{plan.PrimaryBackend.Name}' is not available", plan.PrimaryBackend.Name);

            var step1 = await plan.PrimaryBackend.ConvertAsync(plan.PrimaryJob, ct);
            if (!step1.Success || !plan.IsTwoStep)
                return step1 with
                {
                    InputPath     = job.InputPath,
                    OutputPath    = job.OutputPath,
                    RoutingReason = plan.RoutingReason,
                    FallbackNote  = plan.FallbackNote,
                };

            if (!plan.SecondaryBackend!.IsAvailable)
                return ConversionResult.Fail(job.InputPath, job.OutputPath,
                    $"Backend '{plan.SecondaryBackend.Name}' is not available for step 2", plan.SecondaryBackend.Name);

            var step2 = await plan.SecondaryBackend.ConvertAsync(plan.SecondaryJob!, ct);

            // Annotate final result with both backends used and routing diagnostics
            var backendLabel = $"{plan.PrimaryBackend.Name} → {plan.SecondaryBackend.Name}";
            return step2 with
            {
                InputPath     = job.InputPath,
                OutputPath    = job.OutputPath,
                BackendUsed   = backendLabel,
                Elapsed       = step1.Elapsed + step2.Elapsed,
                RoutingReason = plan.RoutingReason,
                FallbackNote  = plan.FallbackNote,
            };
        }
        catch (OperationCanceledException)
        {
            return ConversionResult.Fail(job.InputPath, job.OutputPath, "Cancelled");
        }
        catch (Exception ex)
        {
            return ConversionResult.Fail(job.InputPath, job.OutputPath, ex.Message);
        }
    }

    public string ResolveOutputPath(string inputPath, string outputFormat, ConversionOptions options,
        int counter = 1, int total = 1)
    {
        var nameWithoutExt = Path.GetFileNameWithoutExtension(inputPath);
        var ext = outputFormat.TrimStart('.');

        // Auto-pad counter to match the width of the total, e.g. total=120 → "001".."120"
        var digits = Math.Max(1, total.ToString().Length);
        var filename = options.OutputNamingPattern
            .Replace("{name}", nameWithoutExt)
            .Replace("{ext}", ext)
            .Replace("{original_ext}", Path.GetExtension(inputPath).TrimStart('.'))
            .Replace("{date}", DateTime.Now.ToString("yyyyMMdd"))
            .Replace("{counter}", counter.ToString($"D{digits}"));

        if (options.OutputFile is not null)
            return options.OutputFile;

        var dir = options.OutputDirectory ?? Path.GetDirectoryName(inputPath) ?? ".";
        return Path.Combine(dir, filename);
    }
}
