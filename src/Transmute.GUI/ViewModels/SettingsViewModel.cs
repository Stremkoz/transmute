using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Windows;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Transmute.Core.Config;
using Transmute.Core.Discovery;

namespace Transmute.GUI.ViewModels;

public partial class SettingsViewModel : ObservableObject
{
    private readonly ConfigManager _configManager;

    // Binary paths
    [ObservableProperty] private string _cwebpPath = string.Empty;
    [ObservableProperty] private string _dwebpPath = string.Empty;
    [ObservableProperty] private string _cjxlPath = string.Empty;
    [ObservableProperty] private string _djxlPath = string.Empty;
    [ObservableProperty] private string _ffmpegPath = string.Empty;
    [ObservableProperty] private string _magickPath = string.Empty;

    // Processing
    [ObservableProperty] private int _maxParallelJobs;
    [ObservableProperty] private int _vipsConcurrency;
    [ObservableProperty] private string _tempDirectory = string.Empty;

    // Defaults
    [ObservableProperty] private int _webpQuality;
    [ObservableProperty] private int _jpegQuality;
    [ObservableProperty] private int _jxlQuality;
    [ObservableProperty] private int _avifQuality;
    [ObservableProperty] private bool _preserveMetadata;
    [ObservableProperty] private bool _overwriteExisting;
    [ObservableProperty] private bool _losslessDefault;
    [ObservableProperty] private int _webpMethod;
    [ObservableProperty] private int _jxlEffort;
    [ObservableProperty] private string _outputNamingPattern = string.Empty;
    [ObservableProperty] private string? _defaultOutputDirectory;

    public string ConfigFilePath => _configManager.ConfigPath;
    public string ConfigMode => _configManager.IsPortable ? "Portable (beside exe)" : "Installed (AppData\\Roaming)";

    public SettingsViewModel(ConfigManager configManager)
    {
        _configManager = configManager;
        LoadFromConfig();
    }

    private void LoadFromConfig()
    {
        var c = _configManager.Config;
        CwebpPath = c.Binaries.Cwebp ?? string.Empty;
        DwebpPath = c.Binaries.Dwebp ?? string.Empty;
        CjxlPath = c.Binaries.Cjxl ?? string.Empty;
        DjxlPath = c.Binaries.Djxl ?? string.Empty;
        FfmpegPath = c.Binaries.Ffmpeg ?? string.Empty;
        MagickPath = c.Binaries.Magick ?? string.Empty;

        MaxParallelJobs = c.Processing.MaxParallelJobs;
        VipsConcurrency = c.Processing.VipsConcurrency;
        TempDirectory = c.Processing.TempDirectory ?? string.Empty;

        WebpQuality = c.Defaults.WebpQuality;
        JpegQuality = c.Defaults.JpegQuality;
        JxlQuality = c.Defaults.JxlQuality;
        AvifQuality = c.Defaults.AvifQuality;
        PreserveMetadata = c.Defaults.PreserveMetadata;
        OverwriteExisting = c.Defaults.OverwriteExisting;
        LosslessDefault = c.Defaults.LosslessDefault;
        WebpMethod = c.Defaults.WebpMethod;
        JxlEffort = c.Defaults.JxlEffort;
        OutputNamingPattern = c.Defaults.OutputNamingPattern;
        DefaultOutputDirectory = c.Defaults.DefaultOutputDirectory ?? string.Empty;
    }

    [RelayCommand]
    private void Save()
    {
        var c = _configManager.Config;
        c.Binaries.Cwebp = NullIfEmpty(CwebpPath);
        c.Binaries.Dwebp = NullIfEmpty(DwebpPath);
        c.Binaries.Cjxl = NullIfEmpty(CjxlPath);
        c.Binaries.Djxl = NullIfEmpty(DjxlPath);
        c.Binaries.Ffmpeg = NullIfEmpty(FfmpegPath);
        c.Binaries.Magick = NullIfEmpty(MagickPath);

        c.Processing.MaxParallelJobs = MaxParallelJobs;
        c.Processing.VipsConcurrency = VipsConcurrency;
        c.Processing.TempDirectory = NullIfEmpty(TempDirectory);

        c.Defaults.WebpQuality = WebpQuality;
        c.Defaults.JpegQuality = JpegQuality;
        c.Defaults.JxlQuality = JxlQuality;
        c.Defaults.AvifQuality = AvifQuality;
        c.Defaults.PreserveMetadata = PreserveMetadata;
        c.Defaults.OverwriteExisting = OverwriteExisting;
        c.Defaults.LosslessDefault = LosslessDefault;
        c.Defaults.WebpMethod = WebpMethod;
        c.Defaults.JxlEffort = JxlEffort;
        c.Defaults.OutputNamingPattern = OutputNamingPattern;
        c.Defaults.DefaultOutputDirectory = NullIfEmpty(DefaultOutputDirectory ?? string.Empty);

        _configManager.Save();
        MessageBox.Show("Settings saved.", "Transmute", MessageBoxButton.OK, MessageBoxImage.Information);
    }

    [RelayCommand]
    private void AutoDetectAll()
    {
        var discovery = new BinaryDiscovery(new BinariesConfig());
        var all = discovery.ResolveAll();

        CwebpPath = all["cwebp"] ?? string.Empty;
        DwebpPath = all["dwebp"] ?? string.Empty;
        CjxlPath = all["cjxl"] ?? string.Empty;
        DjxlPath = all["djxl"] ?? string.Empty;
        FfmpegPath = all["ffmpeg"] ?? string.Empty;
        MagickPath = all["magick"] ?? string.Empty;

        var missing = all.Where(kvp => kvp.Value is null).Select(kvp => kvp.Key).ToList();
        if (missing.Count == 0)
        {
            MessageBox.Show(
                "All binaries found on PATH.",
                "Auto-Detect Complete",
                MessageBoxButton.OK,
                MessageBoxImage.Information);
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
        _configManager.Reset();
        LoadFromConfig();
    }

    private static string? NullIfEmpty(string s) => string.IsNullOrWhiteSpace(s) ? null : s;
}
