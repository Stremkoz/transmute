using System.Text.Json.Serialization;

namespace Transmute.Core.Config;

public class AppConfig
{
    public BinariesConfig Binaries { get; set; } = new();
    public ProcessingConfig Processing { get; set; } = new();
    public DefaultsConfig Defaults { get; set; } = new();
}

public class BinariesConfig
{
    [JsonPropertyName("cwebp")]
    public string? Cwebp { get; set; }

    [JsonPropertyName("dwebp")]
    public string? Dwebp { get; set; }

    [JsonPropertyName("cjxl")]
    public string? Cjxl { get; set; }

    [JsonPropertyName("djxl")]
    public string? Djxl { get; set; }

    [JsonPropertyName("ffmpeg")]
    public string? Ffmpeg { get; set; }

    [JsonPropertyName("magick")]
    public string? Magick { get; set; }
}

public class ProcessingConfig
{
    public int MaxParallelJobs { get; set; } = 0; // 0 = logical CPU count
    public string? TempDirectory { get; set; }
}

public class DefaultsConfig
{
    public int WebpQuality { get; set; } = 85;
    public int JpegQuality { get; set; } = 90;
    public int JxlQuality { get; set; } = 90;
    public int AvifQuality { get; set; } = 80;
    public bool PreserveMetadata { get; set; } = true;
    public bool OverwriteExisting { get; set; } = false;
    public string OutputNamingPattern { get; set; } = "{name}.{ext}";
}
