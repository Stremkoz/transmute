using System.Text.Json.Serialization;
using Transmute.Core.Models;

namespace Transmute.Core.Config;

public class AppConfig
{
    public BinariesConfig Binaries { get; set; } = new();
    public ProcessingConfig Processing { get; set; } = new();
    public DefaultsConfig Defaults { get; set; } = new();
    public LogConfig Log { get; set; } = new();
    public UIConfig UI { get; set; } = new();
}

public class UIConfig
{
    public bool PlaySoundOnCompletion { get; set; } = false;
    public Transmute.Core.Models.AppTheme Theme { get; set; } = Transmute.Core.Models.AppTheme.System;
}

public class LogConfig
{
    public bool Enabled { get; set; } = false;
    public string Format { get; set; } = "text";  // "text" or "json"
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
    public int VipsConcurrency { get; set; } = 0; // 0 = let libvips decide (uses all cores by default)
}

public class DefaultsConfig
{
    public int WebpQuality { get; set; } = 85;
    public int JpegQuality { get; set; } = 90;
    public int JxlQuality { get; set; } = 90;
    public int AvifQuality { get; set; } = 80;
    public MetadataMode MetadataMode { get; set; } = MetadataMode.PreserveAll;
    public bool OverwriteExisting { get; set; } = false;
    public string OutputNamingPattern { get; set; } = "{name}.{ext}";
    public bool LosslessDefault { get; set; } = true;
    public int WebpMethod { get; set; } = 6;   // cwebp -m 0-6, 6 = slowest/best
    public int JxlEffort { get; set; } = 7;    // cjxl -e 1-9, 7 = default
    public string? DefaultOutputDirectory { get; set; }
}
