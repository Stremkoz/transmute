using System.Text.Json;
using System.Text.Json.Serialization;

namespace Transmute.Core.Config;

public class ProfileManager
{
    public const string DefaultProfileName = "Default";

    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        WriteIndented = true,
        DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull,
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
    };

    private readonly string _folderPath;

    public string FolderPath => _folderPath;

    public ProfileManager(string? folderPath = null)
    {
        _folderPath = folderPath ?? ResolveFolderPath();
    }

    /// <summary>
    /// Portable if a Profiles/ folder exists beside the exe, or the portable marker exists.
    /// Otherwise AppData\Roaming\Transmute\Profiles\.
    /// </summary>
    public static string ResolveFolderPath()
    {
        var exeDir = AppContext.BaseDirectory;
        var portableProfiles = Path.Combine(exeDir, "Profiles");
        var portableMarker = Path.Combine(exeDir, "portable");

        if (Directory.Exists(portableProfiles) || File.Exists(portableMarker))
            return portableProfiles;

        return Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
            "Transmute", "Profiles");
    }

    public void EnsureFolder() => Directory.CreateDirectory(_folderPath);

    /// <summary>Returns all profile names, not including "Default".</summary>
    public IReadOnlyList<string> List()
    {
        if (!Directory.Exists(_folderPath))
            return [];

        return Directory.EnumerateFiles(_folderPath, "*.json")
            .Select(f => Path.GetFileNameWithoutExtension(f))
            .Where(n => !string.Equals(n, DefaultProfileName, StringComparison.OrdinalIgnoreCase))
            .OrderBy(n => n, StringComparer.OrdinalIgnoreCase)
            .ToList();
    }

    public bool Exists(string name) =>
        !string.Equals(name, DefaultProfileName, StringComparison.OrdinalIgnoreCase) &&
        File.Exists(FilePath(name));

    /// <summary>
    /// Loads a named profile. Returns null for "Default" (caller uses global config instead).
    /// </summary>
    public ProfileConfig? Load(string name)
    {
        if (string.Equals(name, DefaultProfileName, StringComparison.OrdinalIgnoreCase))
            return null;

        var path = FilePath(name);
        if (!File.Exists(path))
            return null;

        try
        {
            var json = File.ReadAllText(path);
            var profile = JsonSerializer.Deserialize<ProfileConfig>(json, JsonOptions) ?? new ProfileConfig();
            profile.Name = name;
            return profile;
        }
        catch
        {
            return null;
        }
    }

    public void Save(ProfileConfig profile)
    {
        EnsureFolder();
        var json = JsonSerializer.Serialize(profile, JsonOptions);
        File.WriteAllText(FilePath(profile.Name), json);
    }

    public ProfileConfig Create(string name)
    {
        ValidateName(name);
        var profile = new ProfileConfig { Name = name };
        Save(profile);
        return profile;
    }

    public void Duplicate(string sourceName, string destName)
    {
        ValidateName(destName);
        ProfileConfig source;
        if (string.Equals(sourceName, DefaultProfileName, StringComparison.OrdinalIgnoreCase))
        {
            source = new ProfileConfig { Name = destName };
        }
        else
        {
            source = Load(sourceName) ?? throw new InvalidOperationException($"Profile '{sourceName}' not found.");
            source.Name = destName;
        }
        Save(source);
    }

    public void Delete(string name)
    {
        if (string.Equals(name, DefaultProfileName, StringComparison.OrdinalIgnoreCase))
            throw new InvalidOperationException("Cannot delete the Default profile.");

        var path = FilePath(name);
        if (!File.Exists(path))
            throw new InvalidOperationException($"Profile '{name}' not found.");

        File.Delete(path);
    }

    public void Rename(string oldName, string newName)
    {
        if (string.Equals(oldName, DefaultProfileName, StringComparison.OrdinalIgnoreCase))
            throw new InvalidOperationException("Cannot rename the Default profile.");

        ValidateName(newName);

        var oldPath = FilePath(oldName);
        if (!File.Exists(oldPath))
            throw new InvalidOperationException($"Profile '{oldName}' not found.");

        var newPath = FilePath(newName);
        if (File.Exists(newPath))
            throw new InvalidOperationException($"A profile named '{newName}' already exists.");

        File.Move(oldPath, newPath);

        // Update the Name field inside the file
        var profile = Load(newName);
        if (profile is not null)
        {
            profile.Name = newName;
            Save(profile);
        }
    }

    /// <summary>
    /// Gets a single field from a named profile. Key forms:
    ///   defaults.webpQuality   — nullable override (shows value or "(inherited)")
    ///   filter.skip            — SkipFormats as comma-separated string
    ///   filter.only            — OnlyFormats as comma-separated string
    /// Pass null for key to get the full profile JSON.
    /// </summary>
    public string? GetField(string name, string? key, DefaultsConfig globalDefaults)
    {
        var profile = Load(name);
        if (profile is null)
            throw new ArgumentException($"Profile '{name}' not found.");

        if (key is null)
        {
            var opts = new System.Text.Json.JsonSerializerOptions
            {
                WriteIndented = true,
                DefaultIgnoreCondition = System.Text.Json.Serialization.JsonIgnoreCondition.WhenWritingNull,
                PropertyNamingPolicy = System.Text.Json.JsonNamingPolicy.CamelCase,
            };
            return System.Text.Json.JsonSerializer.Serialize(profile, opts);
        }

        var parts = key.Split('.', 2);
        var (section, prop) = parts.Length == 2
            ? (parts[0].ToLowerInvariant(), parts[1].ToLowerInvariant())
            : (key.ToLowerInvariant(), string.Empty);

        return (section, prop) switch
        {
            ("defaults", "webpquality")           => Inherited(profile.WebpQuality,           globalDefaults.WebpQuality),
            ("defaults", "jpegquality")           => Inherited(profile.JpegQuality,           globalDefaults.JpegQuality),
            ("defaults", "jxlquality")            => Inherited(profile.JxlQuality,            globalDefaults.JxlQuality),
            ("defaults", "avifquality")           => Inherited(profile.AvifQuality,           globalDefaults.AvifQuality),
            ("defaults", "preservemetadata")      => Inherited(profile.PreserveMetadata,      globalDefaults.PreserveMetadata),
            ("defaults", "overwriteexisting")     => Inherited(profile.OverwriteExisting,     globalDefaults.OverwriteExisting),
            ("defaults", "losslessdefault")       => Inherited(profile.LosslessDefault,       globalDefaults.LosslessDefault),
            ("defaults", "webpmethod")            => Inherited(profile.WebpMethod,            globalDefaults.WebpMethod),
            ("defaults", "jxleffort")             => Inherited(profile.JxlEffort,             globalDefaults.JxlEffort),
            ("defaults", "outputnamingpattern")   => Inherited(profile.OutputNamingPattern,   globalDefaults.OutputNamingPattern),
            ("defaults", "defaultoutputdirectory")=> Inherited(profile.DefaultOutputDirectory, globalDefaults.DefaultOutputDirectory),
            ("filter", "skip")                    => profile.SkipFormats.Count > 0 ? string.Join(", ", profile.SkipFormats) : "(none)",
            ("filter", "only")                    => profile.OnlyFormats.Count > 0 ? string.Join(", ", profile.OnlyFormats) : "(none)",
            _ => throw new ArgumentException($"Unknown profile key: {key}")
        };
    }

    /// <summary>
    /// Sets a single field on a named profile and saves it.
    /// Key forms match GetField. Use "null" to clear (revert to inherited).
    /// For filter.skip / filter.only pass comma-separated extensions, or "null" to clear.
    /// </summary>
    public void SetField(string name, string key, string value)
    {
        var profile = Load(name) ?? throw new ArgumentException($"Profile '{name}' not found.");

        var parts = key.Split('.', 2);
        var (section, prop) = parts.Length == 2
            ? (parts[0].ToLowerInvariant(), parts[1].ToLowerInvariant())
            : (key.ToLowerInvariant(), string.Empty);

        switch (section, prop)
        {
            case ("defaults", "webpquality"):            profile.WebpQuality           = NullableInt(value);   break;
            case ("defaults", "jpegquality"):            profile.JpegQuality           = NullableInt(value);   break;
            case ("defaults", "jxlquality"):             profile.JxlQuality            = NullableInt(value);   break;
            case ("defaults", "avifquality"):            profile.AvifQuality           = NullableInt(value);   break;
            case ("defaults", "preservemetadata"):       profile.PreserveMetadata      = NullableBool(value);  break;
            case ("defaults", "overwriteexisting"):      profile.OverwriteExisting     = NullableBool(value);  break;
            case ("defaults", "losslessdefault"):        profile.LosslessDefault       = NullableBool(value);  break;
            case ("defaults", "webpmethod"):             profile.WebpMethod            = NullableInt(value);   break;
            case ("defaults", "jxleffort"):              profile.JxlEffort             = NullableInt(value);   break;
            case ("defaults", "outputnamingpattern"):    profile.OutputNamingPattern   = value == "null" ? null : value; break;
            case ("defaults", "defaultoutputdirectory"): profile.DefaultOutputDirectory = value == "null" ? null : value; break;
            case ("filter", "skip"):
                if (value == "null") { profile.SkipFormats = []; profile.OnlyFormats = []; }
                else { profile.SkipFormats = ParseFormats(value); profile.OnlyFormats = []; }
                break;
            case ("filter", "only"):
                if (value == "null") { profile.OnlyFormats = []; profile.SkipFormats = []; }
                else { profile.OnlyFormats = ParseFormats(value); profile.SkipFormats = []; }
                break;
            default:
                throw new ArgumentException($"Unknown profile key: {key}");
        }

        Save(profile);
    }

    private static string Inherited<T>(T? value, T globalValue) where T : struct =>
        value.HasValue ? value.Value.ToString()! : $"(inherited: {globalValue})";

    private static string Inherited(string? value, string? globalValue) =>
        value is not null ? value : $"(inherited: {globalValue ?? "(not set)"})";

    private static int? NullableInt(string value) =>
        value == "null" ? null : int.Parse(value);

    private static bool? NullableBool(string value) =>
        value == "null" ? null : bool.Parse(value);

    private static List<string> ParseFormats(string value) =>
        value.Split(',', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries)
             .Select(s => s.TrimStart('.').ToLowerInvariant())
             .Where(s => !string.IsNullOrEmpty(s))
             .Distinct()
             .ToList();

    private string FilePath(string name) =>
        Path.Combine(_folderPath, SanitizeFileName(name) + ".json");

    private static string SanitizeFileName(string name)
    {
        var invalid = Path.GetInvalidFileNameChars();
        return string.Concat(name.Select(c => invalid.Contains(c) ? '_' : c));
    }

    private void ValidateName(string name)
    {
        if (string.IsNullOrWhiteSpace(name))
            throw new ArgumentException("Profile name cannot be empty.");

        if (string.Equals(name, DefaultProfileName, StringComparison.OrdinalIgnoreCase))
            throw new ArgumentException("'Default' is a reserved profile name.");

        if (Exists(name))
            throw new InvalidOperationException($"A profile named '{name}' already exists.");
    }
}
