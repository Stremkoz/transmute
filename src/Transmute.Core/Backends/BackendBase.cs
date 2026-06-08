using System.Diagnostics;
using System.Text;
using Transmute.Core.Models;

namespace Transmute.Core.Backends;

public abstract class BackendBase : IBackend
{
    public abstract string Name { get; }
    public abstract bool IsAvailable { get; }
    public abstract IReadOnlySet<string> SupportedInputFormats { get; }
    public abstract IReadOnlySet<string> SupportedOutputFormats { get; }

    public virtual bool CanConvert(string inputExt, string outputExt) =>
        SupportedInputFormats.Contains(Normalize(inputExt)) &&
        SupportedOutputFormats.Contains(Normalize(outputExt));

    public abstract Task<ConversionResult> ConvertAsync(ConversionJob job, CancellationToken ct = default);

    protected static string Normalize(string ext) =>
        ext.TrimStart('.').ToLowerInvariant();

    protected async Task<(int ExitCode, string Stdout, string Stderr)> RunProcessAsync(
        string executable,
        IEnumerable<string> args,
        CancellationToken ct = default)
    {
        var psi = new ProcessStartInfo
        {
            FileName = executable,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            UseShellExecute = false,
            CreateNoWindow = true,
        };

        foreach (var arg in args)
            psi.ArgumentList.Add(arg);

        using var process = new Process { StartInfo = psi };

        var stdout = new StringBuilder();
        var stderr = new StringBuilder();

        process.OutputDataReceived += (_, e) => { if (e.Data != null) stdout.AppendLine(e.Data); };
        process.ErrorDataReceived += (_, e) => { if (e.Data != null) stderr.AppendLine(e.Data); };

        process.Start();
        process.BeginOutputReadLine();
        process.BeginErrorReadLine();

        await process.WaitForExitAsync(ct);

        return (process.ExitCode, stdout.ToString(), stderr.ToString());
    }

    protected ConversionResult BuildResult(ConversionJob job, int exitCode, string stderr, Stopwatch sw)
    {
        if (exitCode != 0)
            return ConversionResult.Fail(job.InputPath, job.OutputPath,
                stderr.Trim().Length > 0 ? stderr.Trim() : $"{Name} exited with code {exitCode}", Name);

        var inputBytes = TryGetFileSize(job.InputPath);
        var outputBytes = TryGetFileSize(job.OutputPath);
        return ConversionResult.Ok(job.InputPath, job.OutputPath, Name, sw.Elapsed) with
        {
            InputBytes = inputBytes,
            OutputBytes = outputBytes,
        };
    }

    private static long? TryGetFileSize(string path)
    {
        try { return new FileInfo(path).Length; }
        catch { return null; }
    }
}
