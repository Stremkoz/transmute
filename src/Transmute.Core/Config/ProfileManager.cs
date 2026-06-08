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
