using Transmute.Core.Models;

namespace Transmute.Core.Backends;

public interface IBackend
{
    string Name { get; }
    bool IsAvailable { get; }
    IReadOnlySet<string> SupportedInputFormats { get; }
    IReadOnlySet<string> SupportedOutputFormats { get; }
    bool CanConvert(string inputExt, string outputExt);
    Task<ConversionResult> ConvertAsync(ConversionJob job, CancellationToken ct = default);
}
