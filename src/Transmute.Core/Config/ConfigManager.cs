using System.Text.Json;
using System.Text.Json.Serialization;

namespace Transmute.Core.Config;

public class ConfigManager
{
    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        WriteIndented = true,
        DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull,
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
    };

    private readonly string _configPath;
    private AppConfig _config;

    public AppConfig Config => _config;

    /// <summary>The actual path in use (portable or AppData depending on resolution).</summary>
    public string ConfigPath => _configPath;

    /// <summary>Always the AppData path, regardless of mode. Used as the fallback.</summary>
    public static string AppDataConfigPath => Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
        "Transmute", "config.json");

    /// <summary>
    /// Portable mode: config.json exists beside the exe, or a file named "portable" does.
    /// Installed mode: AppData\Roaming\Transmute\config.json.
    /// </summary>
    public static string ResolveConfigPath()
    {
        var exeDir = AppContext.BaseDirectory;
        var portableConfig = Path.Combine(exeDir, "config.json");
        var portableMarker = Path.Combine(exeDir, "portable");

        if (File.Exists(portableConfig) || File.Exists(portableMarker))
            return portableConfig;

        return AppDataConfigPath;
    }

    public bool IsPortable => !_configPath.Equals(AppDataConfigPath, StringComparison.OrdinalIgnoreCase);

    public ConfigManager(string? configPath = null)
    {
        _configPath = configPath ?? ResolveConfigPath();
        _config = Load();
    }

    private AppConfig Load()
    {
        if (!File.Exists(_configPath))
            return new AppConfig();

        try
        {
            var json = File.ReadAllText(_configPath);
            return JsonSerializer.Deserialize<AppConfig>(json, JsonOptions) ?? new AppConfig();
        }
        catch
        {
            return new AppConfig();
        }
    }

    public void Save()
    {
        Directory.CreateDirectory(Path.GetDirectoryName(_configPath)!);
        var json = JsonSerializer.Serialize(_config, JsonOptions);
        File.WriteAllText(_configPath, json);
    }

    public void Set(string key, string value)
    {
        var parts = key.Split('.', 2);
        if (parts.Length != 2)
            throw new ArgumentException($"Key must be in section.property format, got: {key}");

        var (section, prop) = (parts[0].ToLowerInvariant(), parts[1].ToLowerInvariant());

        switch (section)
        {
            case "binaries":
                SetBinary(prop, value == "null" ? null : value);
                break;
            case "processing":
                SetProcessing(prop, value);
                break;
            case "defaults":
                SetDefault(prop, value);
                break;
            default:
                throw new ArgumentException($"Unknown config section: {section}");
        }

        Save();
    }

    public string? Get(string? key = null)
    {
        if (key is null)
            return JsonSerializer.Serialize(_config, JsonOptions);

        var parts = key.Split('.', 2);
        if (parts.Length != 2)
            throw new ArgumentException($"Key must be in section.property format, got: {key}");

        var (section, prop) = (parts[0].ToLowerInvariant(), parts[1].ToLowerInvariant());

        return section switch
        {
            "binaries" => GetBinary(prop),
            "processing" => GetProcessing(prop),
            "defaults" => GetDefault(prop),
            _ => throw new ArgumentException($"Unknown config section: {section}")
        };
    }

    public void Reset()
    {
        _config = new AppConfig();
        Save();
    }

    private void SetBinary(string prop, string? value)
    {
        switch (prop)
        {
            case "cwebp": _config.Binaries.Cwebp = value; break;
            case "dwebp": _config.Binaries.Dwebp = value; break;
            case "cjxl": _config.Binaries.Cjxl = value; break;
            case "djxl": _config.Binaries.Djxl = value; break;
            case "ffmpeg": _config.Binaries.Ffmpeg = value; break;
            case "magick": _config.Binaries.Magick = value; break;
            default: throw new ArgumentException($"Unknown binary: {prop}");
        }
    }

    private string? GetBinary(string prop) => prop switch
    {
        "cwebp" => _config.Binaries.Cwebp,
        "dwebp" => _config.Binaries.Dwebp,
        "cjxl" => _config.Binaries.Cjxl,
        "djxl" => _config.Binaries.Djxl,
        "ffmpeg" => _config.Binaries.Ffmpeg,
        "magick" => _config.Binaries.Magick,
        _ => throw new ArgumentException($"Unknown binary: {prop}")
    };

    private void SetProcessing(string prop, string value)
    {
        switch (prop)
        {
            case "maxparalleljobs": _config.Processing.MaxParallelJobs = int.Parse(value); break;
            case "tempdirectory": _config.Processing.TempDirectory = value == "null" ? null : value; break;
            default: throw new ArgumentException($"Unknown processing setting: {prop}");
        }
    }

    private string? GetProcessing(string prop) => prop switch
    {
        "maxparalleljobs" => _config.Processing.MaxParallelJobs.ToString(),
        "tempdirectory" => _config.Processing.TempDirectory,
        _ => throw new ArgumentException($"Unknown processing setting: {prop}")
    };

    private void SetDefault(string prop, string value)
    {
        switch (prop)
        {
            case "webpquality": _config.Defaults.WebpQuality = int.Parse(value); break;
            case "jpegquality": _config.Defaults.JpegQuality = int.Parse(value); break;
            case "jxlquality": _config.Defaults.JxlQuality = int.Parse(value); break;
            case "avifquality": _config.Defaults.AvifQuality = int.Parse(value); break;
            case "preservemetadata": _config.Defaults.PreserveMetadata = bool.Parse(value); break;
            case "overwriteexisting": _config.Defaults.OverwriteExisting = bool.Parse(value); break;
            case "outputnamingpattern": _config.Defaults.OutputNamingPattern = value; break;
            default: throw new ArgumentException($"Unknown default: {prop}");
        }
    }

    private string? GetDefault(string prop) => prop switch
    {
        "webpquality" => _config.Defaults.WebpQuality.ToString(),
        "jpegquality" => _config.Defaults.JpegQuality.ToString(),
        "jxlquality" => _config.Defaults.JxlQuality.ToString(),
        "avifquality" => _config.Defaults.AvifQuality.ToString(),
        "preservemetadata" => _config.Defaults.PreserveMetadata.ToString(),
        "overwriteexisting" => _config.Defaults.OverwriteExisting.ToString(),
        "outputnamingpattern" => _config.Defaults.OutputNamingPattern,
        _ => throw new ArgumentException($"Unknown default: {prop}")
    };
}
