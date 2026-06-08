using Transmute.Core.Backends;

namespace Transmute.Core.Routing;

public enum BackendAffinity
{
    Jxl,
    WebP,
    Ffmpeg,
    Vips,
    Magick,
}

public static class FormatRegistry
{
    // Maps extension → preferred backend affinity
    private static readonly Dictionary<string, BackendAffinity> AffinityMap = new(StringComparer.OrdinalIgnoreCase)
    {
        ["jxl"] = BackendAffinity.Jxl,
        ["webp"] = BackendAffinity.WebP,
        ["gif"] = BackendAffinity.Ffmpeg,
        ["apng"] = BackendAffinity.Ffmpeg,
        ["mp4"] = BackendAffinity.Ffmpeg,
        ["mkv"] = BackendAffinity.Ffmpeg,
        ["avi"] = BackendAffinity.Ffmpeg,
        ["mov"] = BackendAffinity.Ffmpeg,
        ["wmv"] = BackendAffinity.Ffmpeg,
        ["flv"] = BackendAffinity.Ffmpeg,
        ["webm"] = BackendAffinity.Ffmpeg,
        ["m4v"] = BackendAffinity.Ffmpeg,
        ["ts"] = BackendAffinity.Ffmpeg,
        ["mpg"] = BackendAffinity.Ffmpeg,
        ["mpeg"] = BackendAffinity.Ffmpeg,
        ["jpg"] = BackendAffinity.Vips,
        ["jpeg"] = BackendAffinity.Vips,
        ["png"] = BackendAffinity.Vips,
        ["tiff"] = BackendAffinity.Vips,
        ["tif"] = BackendAffinity.Vips,
        ["avif"] = BackendAffinity.Vips,
        ["heif"] = BackendAffinity.Vips,
        ["heic"] = BackendAffinity.Vips,
        ["bmp"] = BackendAffinity.Vips,
        ["svg"] = BackendAffinity.Vips,
        ["jp2"] = BackendAffinity.Vips,
        ["j2k"] = BackendAffinity.Vips,
        ["hdr"] = BackendAffinity.Vips,
    };

    public static BackendAffinity GetAffinity(string ext)
    {
        var normalized = ext.TrimStart('.').ToLowerInvariant();
        return AffinityMap.TryGetValue(normalized, out var affinity) ? affinity : BackendAffinity.Magick;
    }

    public static bool IsVideoContainer(string ext) =>
        FfmpegBackend.VideoContainers.Contains(ext.TrimStart('.').ToLowerInvariant());

    public static bool IsAnimated(string ext)
    {
        var normalized = ext.TrimStart('.').ToLowerInvariant();
        return normalized is "gif" or "apng";
    }
}
