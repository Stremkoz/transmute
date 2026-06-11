namespace Transmute.Core.Models;

public record ConversionOptions
{
    public int? Quality { get; set; }
    public bool Lossless { get; set; } = false;
    public int? WebpMethod { get; set; }   // null = use config default
    public int? JxlEffort { get; set; }    // null = use config default
    public double? JxlDistance { get; set; } // null = derive from Quality via formula
    public MetadataMode Metadata { get; set; } = MetadataMode.PreserveAll;
    public bool Overwrite { get; set; } = false;
    public string? OutputDirectory { get; set; }
    public string? OutputFile { get; set; }
    public string OutputNamingPattern { get; set; } = "{name}.{ext}";
    public string? ForcedBackend { get; set; }
    public int MaxParallelJobs { get; set; } = 0; // 0 = logical CPU count
}
