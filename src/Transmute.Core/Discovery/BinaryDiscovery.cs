using Transmute.Core.Config;

namespace Transmute.Core.Discovery;

public class BinaryDiscovery
{
    private readonly BinariesConfig _config;

    public BinaryDiscovery(BinariesConfig config)
    {
        _config = config;
    }

    public string? Resolve(string name)
    {
        // Check config override first
        var configPath = name switch
        {
            "cwebp" => _config.Cwebp,
            "dwebp" => _config.Dwebp,
            "cjxl" => _config.Cjxl,
            "djxl" => _config.Djxl,
            "ffmpeg" => _config.Ffmpeg,
            "magick" => _config.Magick,
            _ => null
        };

        if (configPath is not null)
            return File.Exists(configPath) ? configPath : null;

        return FindOnPath(name);
    }

    private static string? FindOnPath(string name)
    {
        // Try both bare name and .exe for Windows
        var candidates = OperatingSystem.IsWindows()
            ? new[] { name + ".exe", name }
            : new[] { name };

        var pathDirs = (Environment.GetEnvironmentVariable("PATH") ?? "")
            .Split(Path.PathSeparator, StringSplitOptions.RemoveEmptyEntries);

        foreach (var dir in pathDirs)
        {
            foreach (var candidate in candidates)
            {
                var full = Path.Combine(dir, candidate);
                if (File.Exists(full))
                    return full;
            }
        }

        return null;
    }

    public Dictionary<string, string?> ResolveAll() => new()
    {
        ["cwebp"] = Resolve("cwebp"),
        ["dwebp"] = Resolve("dwebp"),
        ["cjxl"] = Resolve("cjxl"),
        ["djxl"] = Resolve("djxl"),
        ["ffmpeg"] = Resolve("ffmpeg"),
        ["magick"] = Resolve("magick"),
    };
}
