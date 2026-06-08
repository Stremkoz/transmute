using System.Text.Json.Serialization;

namespace Transmute.Core.Config;

public class ProfileConfig
{
    public string Name { get; set; } = string.Empty;

    // All nullable — null means "inherit from global defaults"
    public int? WebpQuality { get; set; }
    public int? JpegQuality { get; set; }
    public int? JxlQuality { get; set; }
    public int? AvifQuality { get; set; }
    public bool? PreserveMetadata { get; set; }
    public bool? OverwriteExisting { get; set; }
    public bool? LosslessDefault { get; set; }
    public int? WebpMethod { get; set; }
    public int? JxlEffort { get; set; }
    public string? DefaultOutputDirectory { get; set; }
    public string? OutputNamingPattern { get; set; }

    // Always serialized as [] so power users editing the JSON directly can see and fill them in.
    // Empty = no filter active.
    public List<string> SkipFormats { get; set; } = [];

    // When non-empty, ONLY these formats are processed. Overrides SkipFormats for the profile.
    // GUI warns when this is set; CLI announces it at conversion time.
    public List<string> OnlyFormats { get; set; } = [];

    [JsonIgnore]
    public bool HasOnlyFilter => OnlyFormats.Count > 0;

    [JsonIgnore]
    public bool HasSkipFilter => SkipFormats.Count > 0;

    /// <summary>
    /// Returns an effective DefaultsConfig by layering this profile's non-null fields over
    /// the provided global defaults.
    /// </summary>
    public DefaultsConfig ApplyOver(DefaultsConfig global) => new()
    {
        WebpQuality            = WebpQuality            ?? global.WebpQuality,
        JpegQuality            = JpegQuality            ?? global.JpegQuality,
        JxlQuality             = JxlQuality             ?? global.JxlQuality,
        AvifQuality            = AvifQuality            ?? global.AvifQuality,
        PreserveMetadata       = PreserveMetadata       ?? global.PreserveMetadata,
        OverwriteExisting      = OverwriteExisting      ?? global.OverwriteExisting,
        LosslessDefault        = LosslessDefault        ?? global.LosslessDefault,
        WebpMethod             = WebpMethod             ?? global.WebpMethod,
        JxlEffort              = JxlEffort              ?? global.JxlEffort,
        DefaultOutputDirectory = DefaultOutputDirectory ?? global.DefaultOutputDirectory,
        OutputNamingPattern    = OutputNamingPattern    ?? global.OutputNamingPattern,
    };
}
