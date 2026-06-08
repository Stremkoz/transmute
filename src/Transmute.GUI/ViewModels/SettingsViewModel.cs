using System.IO;
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
    [ObservableProperty] private string _tempDirectory = string.Empty;

    // Defaults
    [ObservableProperty] private int _webpQuality;
    [ObservableProperty] private int _jpegQuality;
    [ObservableProperty] private int _jxlQuality;
    [ObservableProperty] private int _avifQuality;
    [ObservableProperty] private bool _preserveMetadata;
    [ObservableProperty] private bool _overwriteExisting;
    [ObservableProperty] private string _outputNamingPattern = string.Empty;

    public string ConfigFilePath => ConfigManager.DefaultConfigPath;

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
        TempDirectory = c.Processing.TempDirectory ?? string.Empty;

        WebpQuality = c.Defaults.WebpQuality;
        JpegQuality = c.Defaults.JpegQuality;
        JxlQuality = c.Defaults.JxlQuality;
        AvifQuality = c.Defaults.AvifQuality;
        PreserveMetadata = c.Defaults.PreserveMetadata;
        OverwriteExisting = c.Defaults.OverwriteExisting;
        OutputNamingPattern = c.Defaults.OutputNamingPattern;
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
        c.Processing.TempDirectory = NullIfEmpty(TempDirectory);

        c.Defaults.WebpQuality = WebpQuality;
        c.Defaults.JpegQuality = JpegQuality;
        c.Defaults.JxlQuality = JxlQuality;
        c.Defaults.AvifQuality = AvifQuality;
        c.Defaults.PreserveMetadata = PreserveMetadata;
        c.Defaults.OverwriteExisting = OverwriteExisting;
        c.Defaults.OutputNamingPattern = OutputNamingPattern;

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
    }

    [RelayCommand]
    private void ResetDefaults()
    {
        _configManager.Reset();
        LoadFromConfig();
    }

    private static string? NullIfEmpty(string s) => string.IsNullOrWhiteSpace(s) ? null : s;
}
