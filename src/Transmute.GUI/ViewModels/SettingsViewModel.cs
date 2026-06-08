using System.Collections.ObjectModel;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Windows;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Transmute.Core.Config;
using Transmute.Core.Discovery;
using Transmute.Core.Models;
using Transmute.GUI;

namespace Transmute.GUI.ViewModels;

public partial class SettingsViewModel : ObservableObject
{
    private readonly ConfigManager _configManager;
    private readonly ProfileManager _profileManager;

    // Binary paths — always global, never per-profile
    [ObservableProperty] private string _cwebpPath = string.Empty;
    [ObservableProperty] private string _dwebpPath = string.Empty;
    [ObservableProperty] private string _cjxlPath = string.Empty;
    [ObservableProperty] private string _djxlPath = string.Empty;
    [ObservableProperty] private string _ffmpegPath = string.Empty;
    [ObservableProperty] private string _magickPath = string.Empty;

    // Processing — always global, never per-profile
    [ObservableProperty] private int _maxParallelJobs;
    [ObservableProperty] private int _vipsConcurrency;
    [ObservableProperty] private string _tempDirectory = string.Empty;

    // Defaults — per-profile when a named profile is selected, otherwise global
    [ObservableProperty] private int _webpQuality;
    [ObservableProperty] private int _jpegQuality;
    [ObservableProperty] private int _jxlQuality;
    [ObservableProperty] private int _avifQuality;
    [ObservableProperty] private MetadataMode _metadataMode;
    [ObservableProperty] private bool _overwriteExisting;
    [ObservableProperty] private bool _losslessDefault;
    [ObservableProperty] private int _webpMethod;
    [ObservableProperty] private int _jxlEffort;
    [ObservableProperty] private string _outputNamingPattern = string.Empty;
    [ObservableProperty] private string? _defaultOutputDirectory;

    // Log file — global settings
    [ObservableProperty] private bool _logEnabled = false;
    [ObservableProperty] private bool _logFormatIsJson = false;  // false=text, true=json

    // UI behaviour — global settings
    [ObservableProperty] private bool _playSoundOnCompletion = false;
    [ObservableProperty] private AppTheme _theme = AppTheme.System;

    private bool _loading = false;

    partial void OnThemeChanged(AppTheme value)
    {
        if (!_loading) ThemeManager.Apply(value, Application.Current.Resources);
    }

    // Format filter — per named profile only; Default profile has no filter
    [ObservableProperty] private string _profileFormatFilter = string.Empty;  // comma-separated
    [ObservableProperty] private bool _profileFilterIsOnly = false;            // false=skip, true=only

    // Profile selection — synced with main window on open
    [ObservableProperty] private string _selectedProfile = ProfileManager.DefaultProfileName;
    public ObservableCollection<string> Profiles { get; } = new();

    public bool IsDefaultProfile =>
        string.Equals(SelectedProfile, ProfileManager.DefaultProfileName, StringComparison.OrdinalIgnoreCase);

    public event EventHandler? Saved;

    public string ConfigFilePath => _configManager.ConfigPath;
    public string ConfigMode => _configManager.IsPortable ? "Portable (beside exe)" : "Installed (AppData\\Roaming)";
    public string ProfilesFolder => _profileManager.FolderPath;

    public SettingsViewModel(ConfigManager configManager, ProfileManager profileManager, string activeProfile)
    {
        _configManager = configManager;
        _profileManager = profileManager;

        RefreshProfiles();
        _selectedProfile = Profiles.Contains(activeProfile) ? activeProfile : ProfileManager.DefaultProfileName;

        LoadDefaults();
        LoadGlobalOnly();
    }

    private void RefreshProfiles()
    {
        Profiles.Clear();
        Profiles.Add(ProfileManager.DefaultProfileName);
        foreach (var name in _profileManager.List())
            Profiles.Add(name);
    }

    partial void OnSelectedProfileChanged(string value)
    {
        OnPropertyChanged(nameof(IsDefaultProfile));
        LoadDefaults();
    }

    private void LoadDefaults()
    {
        DefaultsConfig d;
        ProfileConfig? rawProfile = null;

        if (IsDefaultProfile)
        {
            d = _configManager.Config.Defaults;
        }
        else
        {
            rawProfile = _profileManager.Load(SelectedProfile);
            if (rawProfile is null) { LoadDefaults(); return; }
            // Show effective values (profile overrides on top of global) so the user sees what they actually get
            d = rawProfile.ApplyOver(_configManager.Config.Defaults);
        }

        WebpQuality        = d.WebpQuality;
        JpegQuality        = d.JpegQuality;
        JxlQuality         = d.JxlQuality;
        AvifQuality        = d.AvifQuality;
        MetadataMode       = d.MetadataMode;
        OverwriteExisting  = d.OverwriteExisting;
        LosslessDefault    = d.LosslessDefault;
        WebpMethod         = d.WebpMethod;
        JxlEffort          = d.JxlEffort;
        OutputNamingPattern = d.OutputNamingPattern;
        DefaultOutputDirectory = d.DefaultOutputDirectory ?? string.Empty;

        // Format filter is profile-specific only
        if (rawProfile?.HasOnlyFilter == true)
        {
            ProfileFilterIsOnly = true;
            ProfileFormatFilter = string.Join(", ", rawProfile.OnlyFormats);
        }
        else if (rawProfile?.HasSkipFilter == true)
        {
            ProfileFilterIsOnly = false;
            ProfileFormatFilter = string.Join(", ", rawProfile.SkipFormats);
        }
        else
        {
            ProfileFilterIsOnly = false;
            ProfileFormatFilter = string.Empty;
        }
    }

    private void LoadGlobalOnly()
    {
        var c = _configManager.Config;
        CwebpPath = c.Binaries.Cwebp ?? string.Empty;
        DwebpPath = c.Binaries.Dwebp ?? string.Empty;
        CjxlPath  = c.Binaries.Cjxl  ?? string.Empty;
        DjxlPath  = c.Binaries.Djxl  ?? string.Empty;
        FfmpegPath = c.Binaries.Ffmpeg ?? string.Empty;
        MagickPath = c.Binaries.Magick ?? string.Empty;

        MaxParallelJobs = c.Processing.MaxParallelJobs;
        VipsConcurrency = c.Processing.VipsConcurrency;
        TempDirectory   = c.Processing.TempDirectory ?? string.Empty;

        LogEnabled           = c.Log.Enabled;
        LogFormatIsJson      = string.Equals(c.Log.Format, "json", StringComparison.OrdinalIgnoreCase);
        PlaySoundOnCompletion = c.UI.PlaySoundOnCompletion;
        _loading = true;
        Theme = c.UI.Theme;
        _loading = false;
    }

    [RelayCommand]
    private void Save()
    {
        // Binaries + Processing always go to global config
        var c = _configManager.Config;
        c.Binaries.Cwebp   = NullIfEmpty(CwebpPath);
        c.Binaries.Dwebp   = NullIfEmpty(DwebpPath);
        c.Binaries.Cjxl    = NullIfEmpty(CjxlPath);
        c.Binaries.Djxl    = NullIfEmpty(DjxlPath);
        c.Binaries.Ffmpeg  = NullIfEmpty(FfmpegPath);
        c.Binaries.Magick  = NullIfEmpty(MagickPath);

        c.Processing.MaxParallelJobs = MaxParallelJobs;
        c.Processing.VipsConcurrency = VipsConcurrency;
        c.Processing.TempDirectory   = NullIfEmpty(TempDirectory);

        c.Log.Enabled = LogEnabled;
        c.Log.Format  = LogFormatIsJson ? "json" : "text";
        c.UI.PlaySoundOnCompletion = PlaySoundOnCompletion;
        c.UI.Theme = Theme;

        if (IsDefaultProfile)
        {
            // Defaults tab → global config
            c.Defaults.WebpQuality          = WebpQuality;
            c.Defaults.JpegQuality          = JpegQuality;
            c.Defaults.JxlQuality           = JxlQuality;
            c.Defaults.AvifQuality          = AvifQuality;
            c.Defaults.MetadataMode         = MetadataMode;
            c.Defaults.OverwriteExisting    = OverwriteExisting;
            c.Defaults.LosslessDefault      = LosslessDefault;
            c.Defaults.WebpMethod           = WebpMethod;
            c.Defaults.JxlEffort            = JxlEffort;
            c.Defaults.OutputNamingPattern  = OutputNamingPattern;
            c.Defaults.DefaultOutputDirectory = NullIfEmpty(DefaultOutputDirectory ?? string.Empty);
            _configManager.Save();
        }
        else
        {
            // Defaults tab → named profile file
            _configManager.Save(); // save binaries/processing to global

            var existing = _profileManager.Load(SelectedProfile) ?? new ProfileConfig { Name = SelectedProfile };

            // We save the full effective value into the profile, but only the fields
            // that differ from global defaults get stored (the rest remain null = inherit)
            var global = _configManager.Config.Defaults;
            existing.WebpQuality          = WebpQuality    != global.WebpQuality    ? WebpQuality    : null;
            existing.JpegQuality          = JpegQuality    != global.JpegQuality    ? JpegQuality    : null;
            existing.JxlQuality           = JxlQuality     != global.JxlQuality     ? JxlQuality     : null;
            existing.AvifQuality          = AvifQuality    != global.AvifQuality    ? AvifQuality    : null;
            existing.Metadata             = MetadataMode   != global.MetadataMode   ? MetadataMode   : null;
            existing.OverwriteExisting    = OverwriteExisting != global.OverwriteExisting ? OverwriteExisting : null;
            existing.LosslessDefault      = LosslessDefault      != global.LosslessDefault      ? LosslessDefault      : null;
            existing.WebpMethod           = WebpMethod           != global.WebpMethod           ? WebpMethod           : null;
            existing.JxlEffort            = JxlEffort            != global.JxlEffort            ? JxlEffort            : null;
            existing.OutputNamingPattern  = OutputNamingPattern  != global.OutputNamingPattern  ? OutputNamingPattern  : null;
            existing.DefaultOutputDirectory = NullIfEmpty(DefaultOutputDirectory ?? string.Empty) != global.DefaultOutputDirectory
                ? NullIfEmpty(DefaultOutputDirectory ?? string.Empty)
                : null;

            // Format filter
            var parsedFormats = ProfileFormatFilter
                .Split(',', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries)
                .Select(s => s.TrimStart('.').ToLowerInvariant())
                .Where(s => !string.IsNullOrEmpty(s))
                .Distinct()
                .ToList();
            if (ProfileFilterIsOnly)
            {
                existing.OnlyFormats = parsedFormats;
                existing.SkipFormats = [];
            }
            else
            {
                existing.SkipFormats = parsedFormats;
                existing.OnlyFormats = [];
            }

            _profileManager.Save(existing);
        }

        MessageBox.Show(
            IsDefaultProfile
                ? "Settings saved."
                : $"Profile '{SelectedProfile}' saved.",
            "Transmute",
            MessageBoxButton.OK,
            MessageBoxImage.Information);

        Saved?.Invoke(this, EventArgs.Empty);
    }

    [RelayCommand]
    private void AutoDetectAll()
    {
        var discovery = new BinaryDiscovery(new BinariesConfig());
        var all = discovery.ResolveAll();

        CwebpPath  = all["cwebp"]  ?? string.Empty;
        DwebpPath  = all["dwebp"]  ?? string.Empty;
        CjxlPath   = all["cjxl"]   ?? string.Empty;
        DjxlPath   = all["djxl"]   ?? string.Empty;
        FfmpegPath = all["ffmpeg"] ?? string.Empty;
        MagickPath = all["magick"] ?? string.Empty;

        var missing = all.Where(kvp => kvp.Value is null).Select(kvp => kvp.Key).ToList();
        if (missing.Count == 0)
        {
            MessageBox.Show("All binaries found on PATH.", "Auto-Detect Complete",
                MessageBoxButton.OK, MessageBoxImage.Information);
        }
        else
        {
            var names = string.Join(", ", missing);
            MessageBox.Show(
                $"The following binaries were not found on PATH:\n\n    {names}\n\n" +
                "Please enter their full paths in the fields above.\n" +
                "If they are not installed, use the Download Binaries button (coming soon).",
                "Some Binaries Not Found",
                MessageBoxButton.OK,
                MessageBoxImage.Warning);
        }
    }

    [RelayCommand]
    private void DownloadBinaries()
    {
        new Views.BinaryDownloadsWindow { Owner = Application.Current.MainWindow }.ShowDialog();
    }

    [RelayCommand]
    private void ResetDefaults()
    {
        if (IsDefaultProfile)
        {
            _configManager.Reset();
            LoadDefaults();
            LoadGlobalOnly();
        }
        else
        {
            var result = MessageBox.Show(
                $"Reset profile '{SelectedProfile}' to inherit everything from global defaults?",
                "Reset Profile",
                MessageBoxButton.YesNo,
                MessageBoxImage.Question,
                MessageBoxResult.No);
            if (result != MessageBoxResult.Yes) return;

            var empty = new ProfileConfig { Name = SelectedProfile };
            _profileManager.Save(empty);
            LoadDefaults();
        }
    }

    private static string? NullIfEmpty(string s) => string.IsNullOrWhiteSpace(s) ? null : s;
}
