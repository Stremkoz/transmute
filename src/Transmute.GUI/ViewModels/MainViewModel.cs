using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Collections.Specialized;
using System.ComponentModel;
using System.IO;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using System.Windows;
using System.Windows.Media.Imaging;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Transmute.Core;
using Transmute.Core.Config;
using Transmute.Core.Models;
using Transmute.Core.Processing;

namespace Transmute.GUI.ViewModels;

// Raises a single Reset notification for batch adds instead of one per item,
// keeping the UI fast when adding hundreds of files at once.
public class BulkObservableCollection<T> : ObservableCollection<T>
{
    public void AddRange(IEnumerable<T> items)
    {
        foreach (var item in items)
            Items.Add(item);
        OnPropertyChanged(new PropertyChangedEventArgs(nameof(Count)));
        OnPropertyChanged(new PropertyChangedEventArgs("Item[]"));
        OnCollectionChanged(new NotifyCollectionChangedEventArgs(NotifyCollectionChangedAction.Reset));
    }
}

public partial class MainViewModel : ObservableObject
{
    private readonly ConfigManager _configManager;
    private readonly ProfileManager _profileManager;
    private bool _qualityUserCustomized = false;
    private bool _suppressQualityTracking = false;

    // Formats where lossless encoding is a meaningful option
    private static readonly HashSet<string> LosslessCapableFormats =
        new(StringComparer.OrdinalIgnoreCase) { "jxl", "webp" };

    [ObservableProperty] private string _targetFormat = "webp";
    [ObservableProperty] private int _quality = 85;
    [ObservableProperty] private bool _lossless = true;
    [ObservableProperty] private bool _losslessVisible = true;
    [ObservableProperty] private bool _qualityEnabled = false;
    [ObservableProperty] private MetadataMode _metadataMode = MetadataMode.PreserveAll;
    [ObservableProperty] private bool _overwriteExisting = false;
    [ObservableProperty] private string? _outputDirectory;
    [ObservableProperty] private bool _isConverting = false;
    [ObservableProperty] private int _progressValue = 0;
    [ObservableProperty] private int _progressMax = 1;
    [ObservableProperty] private string _statusText = "Drop images or folders here to get started.";
    [ObservableProperty] private bool _includeSubfolders = true;

    // Active profile — Default means use global config
    [ObservableProperty] private string _activeProfile = ProfileManager.DefaultProfileName;
    public ObservableCollection<string> Profiles { get; } = new();

    // Advanced panel — session-only, always reset to defaults on launch
    [ObservableProperty] private bool _showAdvanced = false;
    [ObservableProperty] private bool _sessionOverwrite = false;
    [ObservableProperty] private bool _sessionOnlyMode = false;  // false = skip, true = only
    [ObservableProperty] private bool _skipJpeg = false;
    [ObservableProperty] private bool _skipPng = false;
    [ObservableProperty] private bool _skipGif = false;
    [ObservableProperty] private bool _skipWebp = false;
    [ObservableProperty] private bool _skipAvif = false;
    [ObservableProperty] private bool _skipJxl = false;
    [ObservableProperty] private bool _skipHeic = false;

    public string SkipModeLabel => SessionOnlyMode ? "Only input formats:" : "Skip input formats:";
    public string SkipModeHint  => SessionOnlyMode ? "(highlighted = only)" : "(highlighted = skipped)";

    partial void OnSessionOnlyModeChanged(bool value)
    {
        OnPropertyChanged(nameof(SkipModeLabel));
        OnPropertyChanged(nameof(SkipModeHint));
    }

    public BulkObservableCollection<object> InputFiles { get; } = new();
    public ObservableCollection<string> LogLines { get; } = new();

    public static string[] SupportedFormats { get; } =
    [
        "webp", "avif", "jxl", "png", "jpg", "tiff", "heic", "bmp", "gif"
    ];

    private CancellationTokenSource? _cts;

    public MainViewModel(ConfigManager configManager, ProfileManager profileManager)
    {
        _configManager = configManager;
        _profileManager = profileManager;

        RefreshProfiles();

        var defaults = configManager.Config.Defaults;
        _suppressQualityTracking = true;
        _quality = defaults.WebpQuality;
        _suppressQualityTracking = false;
        _metadataMode = defaults.MetadataMode;
        _overwriteExisting = defaults.OverwriteExisting;
        _outputDirectory = defaults.DefaultOutputDirectory;
        // webp is the default format — initialize lossless state for it
        _lossless = defaults.LosslessDefault;
        _losslessVisible = LosslessCapableFormats.Contains(_targetFormat);
        _qualityEnabled = !_losslessVisible || !_lossless;
        InputFiles.CollectionChanged += (_, _) => ConvertCommand.NotifyCanExecuteChanged();
    }

    public void RefreshProfiles()
    {
        var current = ActiveProfile;
        Profiles.Clear();
        Profiles.Add(ProfileManager.DefaultProfileName);
        foreach (var name in _profileManager.List())
            Profiles.Add(name);

        // Keep active selection if it still exists, otherwise fall back to Default
        ActiveProfile = Profiles.Contains(current) ? current : ProfileManager.DefaultProfileName;
    }

    partial void OnActiveProfileChanged(string value)
    {
        // Load effective defaults for this profile and apply them to the session
        var profile = string.Equals(value, ProfileManager.DefaultProfileName, StringComparison.OrdinalIgnoreCase)
            ? null
            : _profileManager.Load(value);
        var defaults = profile?.ApplyOver(_configManager.Config.Defaults) ?? _configManager.Config.Defaults;

        _suppressQualityTracking = true;
        _qualityUserCustomized = false;

        var qualityForFormat = TargetFormat.ToLowerInvariant() switch
        {
            "webp"          => defaults.WebpQuality,
            "jpg" or "jpeg" => defaults.JpegQuality,
            "jxl"           => defaults.JxlQuality,
            "avif"          => defaults.AvifQuality,
            _               => Quality
        };
        Quality = qualityForFormat;
        _suppressQualityTracking = false;

        if (LosslessVisible)
            Lossless = defaults.LosslessDefault;

        MetadataMode = defaults.MetadataMode;
        OverwriteExisting = defaults.OverwriteExisting;
        OutputDirectory = defaults.DefaultOutputDirectory;
    }

    // Returns the effective DefaultsConfig for the current active profile
    private DefaultsConfig EffectiveDefaults()
    {
        var profile = string.Equals(ActiveProfile, ProfileManager.DefaultProfileName, StringComparison.OrdinalIgnoreCase)
            ? null
            : _profileManager.Load(ActiveProfile);
        return profile?.ApplyOver(_configManager.Config.Defaults) ?? _configManager.Config.Defaults;
    }

    partial void OnTargetFormatChanged(string value)
    {
        if (!_qualityUserCustomized)
        {
            _suppressQualityTracking = true;
            var d = EffectiveDefaults();
            Quality = value.ToLowerInvariant() switch
            {
                "webp"          => d.WebpQuality,
                "jpg" or "jpeg" => d.JpegQuality,
                "jxl"           => d.JxlQuality,
                "avif"          => d.AvifQuality,
                _               => 90
            };
            _suppressQualityTracking = false;
        }
        _qualityUserCustomized = false;

        LosslessVisible = LosslessCapableFormats.Contains(value);
        if (LosslessVisible)
            Lossless = EffectiveDefaults().LosslessDefault;

        QualityEnabled = !LosslessVisible || !Lossless;
    }

    partial void OnLosslessChanged(bool value)
    {
        QualityEnabled = !LosslessVisible || !value;
    }

    partial void OnQualityChanged(int value)
    {
        if (!_suppressQualityTracking)
            _qualityUserCustomized = true;
    }

    public void AddFiles(IEnumerable<string> paths)
    {
        var existing = InputFiles.OfType<FileEntryViewModel>().Select(f => f.Path).ToHashSet();
        var newEntries = paths
            .Where(p => File.Exists(p) && !existing.Contains(p))
            .Select(p => (object)new FileEntryViewModel(p))
            .ToList();

        if (newEntries.Count > 0)
            InputFiles.AddRange(newEntries);

        UpdateStatus();
    }

    public void AddFolder(string path, bool includeSubfolders)
    {
        if (InputFiles.OfType<FolderEntryViewModel>().Any(f => f.Path == path))
            return;
        InputFiles.Add(new FolderEntryViewModel(path, includeSubfolders));
        UpdateStatus();
    }

    [RelayCommand]
    private void RemoveEntry(object? entry)
    {
        if (entry is not null)
            InputFiles.Remove(entry);
        UpdateStatus();
    }

    [RelayCommand]
    private void ClearFiles()
    {
        InputFiles.Clear();
        StatusText = "Drop images or folders here to get started.";
    }

    [RelayCommand]
    private void ClearLog() => LogLines.Clear();

    [RelayCommand]
    private void ClearCompleted()
    {
        var toRemove = LogLines
            .Where(l => l.StartsWith("✓") || l.StartsWith("⊘") || l.StartsWith("─"))
            .ToList();
        foreach (var line in toRemove) LogLines.Remove(line);
    }

    [RelayCommand]
    private void ClearAll()
    {
        InputFiles.Clear();
        LogLines.Clear();
        StatusText = "Drop images or folders here to get started.";
    }

    public void MoveEntry(int fromIndex, int toIndex)
    {
        if (fromIndex < 0 || fromIndex >= InputFiles.Count) return;
        if (toIndex  < 0) toIndex = 0;
        if (toIndex  > InputFiles.Count) toIndex = InputFiles.Count;
        // ObservableCollection.Move's newIndex is post-removal; adjust when moving forward
        var adjusted = toIndex > fromIndex ? toIndex - 1 : toIndex;
        if (adjusted != fromIndex)
            InputFiles.Move(fromIndex, adjusted);
    }

    [RelayCommand]
    private void ClearOutputDirectory() =>
        OutputDirectory = EffectiveDefaults().DefaultOutputDirectory;

    [RelayCommand(CanExecute = nameof(CanConvert))]
    private async Task ConvertAsync()
    {
        if (InputFiles.Count == 0) return;

        _cts = new CancellationTokenSource();
        IsConverting = true;
        ProgressValue = 0;
        LogLines.Clear();
        StatusText = "Preparing files...";
        var totalSw = System.Diagnostics.Stopwatch.StartNew();

        var defaults = EffectiveDefaults();
        var options = new ConversionOptions
        {
            Quality = Quality,
            Lossless = LosslessVisible && Lossless,
            WebpMethod = defaults.WebpMethod,
            JxlEffort = defaults.JxlEffort,
            Metadata = MetadataMode,
            Overwrite = OverwriteExisting || SessionOverwrite,
            OutputDirectory = OutputDirectory,
            OutputNamingPattern = defaults.OutputNamingPattern,
        };

        var config = _configManager.Config;
        var (engine, _, temp, _) = TransmuteFactory.Create(config);
        IReadOnlyList<Transmute.Core.Models.ConversionResult>? completedResults = null;

        using (temp)
        {
            // Expand all queue entries into a flat filtered path list, then build jobs with counter
            var allPaths = new List<string>();
            foreach (var entry in InputFiles)
            {
                IEnumerable<string> filePaths = entry switch
                {
                    FileEntryViewModel fvm => [fvm.Path],
                    FolderEntryViewModel folderVm => folderVm.GetImagePaths(),
                    _ => []
                };
                foreach (var path in filePaths)
                    if (!IsFormatSkipped(path)) allPaths.Add(path);
            }

            // Warn if any queued files are already in the target format
            var sameFormatPaths = allPaths
                .Where(p => Path.GetExtension(p).TrimStart('.').Equals(TargetFormat, StringComparison.OrdinalIgnoreCase))
                .ToHashSet(StringComparer.OrdinalIgnoreCase);

            if (sameFormatPaths.Count > 0)
            {
                var msg = sameFormatPaths.Count == 1
                    ? $"1 file in the queue is already {TargetFormat.ToUpperInvariant()}. Overwrite it?"
                    : $"{sameFormatPaths.Count} files in the queue are already {TargetFormat.ToUpperInvariant()}. Overwrite them?";

                var answer = MessageBox.Show(msg, "Transmute — Same Format",
                    MessageBoxButton.YesNo, MessageBoxImage.Warning, MessageBoxResult.No);

                if (answer == MessageBoxResult.No)
                {
                    allPaths = allPaths.Where(p => !sameFormatPaths.Contains(p)).ToList();
                    sameFormatPaths.Clear();
                }
            }

            var jobs = allPaths
                .Select((path, i) => new ConversionJob
                {
                    InputPath = path,
                    OutputPath = engine.ResolveOutputPath(path, TargetFormat, options, i + 1, allPaths.Count),
                    OutputFormat = TargetFormat,
                    Options = sameFormatPaths.Contains(path) ? options with { Overwrite = true } : options,
                }).ToList();

            ProgressMax = Math.Max(1, jobs.Count);
            StatusText = $"Converting {jobs.Count:N0} file(s)...";

            var progress = new Progress<ConversionProgress>(p =>
            {
                Application.Current?.Dispatcher.Invoke(() =>
                {
                    ProgressValue = p.Completed;
                    if (p.LastResult is { } r)
                    {
                        string line;
                        if (r.Success)
                        {
                            var sizePart = FormatSizeDelta(r.InputBytes, r.OutputBytes);
                            line = $"✓ {Path.GetFileName(r.InputPath)} → {Path.GetFileName(r.OutputPath)} [{r.BackendUsed}]{sizePart} {r.Elapsed.TotalSeconds:F2}s";
                        }
                        else if (r.Skipped)
                        {
                            line = $"⊘ {Path.GetFileName(r.InputPath)}: skipped (output already exists)";
                        }
                        else
                        {
                            line = $"✗ {Path.GetFileName(r.InputPath)}: {r.Error}";
                        }
                        LogLines.Add(line);
                        StatusText = p.Completed < p.Total
                            ? $"Converting... ({p.Completed:N0} / {p.Total:N0})"
                            : $"Done: {p.Completed - p.Failed - p.Skipped:N0} succeeded, {p.Skipped:N0} skipped, {p.Failed:N0} failed — {totalSw.Elapsed.TotalSeconds:F1}s";
                    }
                });
            });

            try
            {
                var results = await engine.ConvertAllAsync(jobs, progress, _cts.Token);
                completedResults = results;
                totalSw.Stop();

                var succeeded = results.Where(r => r.Success).ToList();
                var totalIn = succeeded.Sum(r => r.InputBytes ?? 0);
                var totalOut = succeeded.Sum(r => r.OutputBytes ?? 0);
                if (succeeded.Count > 0 && totalIn > 0)
                    LogLines.Add($"─── Total: {succeeded.Count} file(s)  {FormatSizeDelta(totalIn, totalOut).Trim()} ───");

                // Write log file if enabled
                if (config.Log.Enabled && results.Count > 0)
                {
                    try
                    {
                        var logDir = !string.IsNullOrEmpty(options.OutputDirectory)
                            ? options.OutputDirectory
                            : Path.GetDirectoryName(results[0].OutputPath) ?? ".";
                        var logPath = LogWriter.Write(results, logDir, config.Log.Format, totalSw.Elapsed);
                        LogLines.Add($"📄 Log written: {logPath}");
                    }
                    catch (Exception ex)
                    {
                        LogLines.Add($"⚠ Log file error: {ex.Message}");
                    }
                }
            }
            catch (OperationCanceledException)
            {
                StatusText = $"Cancelled after {totalSw.Elapsed.TotalSeconds:F1}s.";
                LogLines.Add("— Cancelled —");
            }
        }

        ProgressValue = ProgressMax;
        IsConverting = false;
        ConvertCommand.NotifyCanExecuteChanged();

        // Completion sound — only for non-cancelled jobs, only when enabled
        if (completedResults is not null && config.UI.PlaySoundOnCompletion)
        {
            var hasProblems = completedResults.Any(r =>
                (!r.Success && !r.Skipped) || r.FallbackNote is not null);
            if (hasProblems)
                System.Media.SystemSounds.Exclamation.Play();
            else
                System.Media.SystemSounds.Asterisk.Play();
        }
    }

    private bool CanConvert() => !IsConverting && InputFiles.Count > 0;

    [RelayCommand]
    private void Cancel() => _cts?.Cancel();

    partial void OnIsConvertingChanged(bool value) =>
        ConvertCommand.NotifyCanExecuteChanged();

    private bool IsFormatSkipped(string path)
    {
        var ext = Path.GetExtension(path).TrimStart('.').ToLowerInvariant();

        // Normalise jpg/jpeg to a single lookup
        bool ChipFor(string e) => e switch
        {
            "jpg" or "jpeg"  => SkipJpeg,
            "png"            => SkipPng,
            "gif"            => SkipGif,
            "webp"           => SkipWebp,
            "avif"           => SkipAvif,
            "jxl"            => SkipJxl,
            "heic" or "heif" => SkipHeic,
            _                => false
        };

        bool anyChipChecked = SkipJpeg || SkipPng || SkipGif || SkipWebp || SkipAvif || SkipJxl || SkipHeic;

        if (anyChipChecked)
        {
            bool thisChipChecked = ChipFor(ext);
            // In "Only" mode: pass only checked formats (skip all others)
            // In "Skip" mode: skip checked formats
            return SessionOnlyMode ? !thisChipChecked : thisChipChecked;
        }

        // No session chips active — fall through to profile filter
        var profile = string.Equals(ActiveProfile, ProfileManager.DefaultProfileName, StringComparison.OrdinalIgnoreCase)
            ? null
            : _profileManager.Load(ActiveProfile);

        if (profile?.HasOnlyFilter == true)
        {
            var onlySet = profile.OnlyFormats
                .Select(s => s.TrimStart('.').ToLowerInvariant())
                .ToHashSet(StringComparer.OrdinalIgnoreCase);
            return !onlySet.Contains(ext);
        }

        if (profile?.HasSkipFilter == true)
        {
            var skipSet = profile.SkipFormats
                .Select(s => s.TrimStart('.').ToLowerInvariant())
                .ToHashSet(StringComparer.OrdinalIgnoreCase);
            return skipSet.Contains(ext);
        }

        return false;
    }

    private static string FormatSizeDelta(long? inputBytes, long? outputBytes)
    {
        if (inputBytes is null || outputBytes is null || inputBytes == 0) return string.Empty;
        var inMb = inputBytes.Value / (1024.0 * 1024);
        var outMb = outputBytes.Value / (1024.0 * 1024);
        var pct = (outputBytes.Value - inputBytes.Value) * 100.0 / inputBytes.Value;
        var sign = pct < 0 ? "" : "+";
        return $"  {inMb:F1}MB→{outMb:F1}MB ({sign}{pct:F0}%)";
    }

    private void UpdateStatus()
    {
        var fileCount = InputFiles.OfType<FileEntryViewModel>().Count();
        var folderCount = InputFiles.OfType<FolderEntryViewModel>().Count();
        StatusText = (fileCount, folderCount) switch
        {
            (0, 0) => "Drop images or folders here to get started.",
            (_, 0) => $"{fileCount:N0} file{(fileCount == 1 ? "" : "s")} queued.",
            (0, _) => $"{folderCount} folder{(folderCount == 1 ? "" : "s")} queued.",
            _ => $"{fileCount:N0} file{(fileCount == 1 ? "" : "s")} and {folderCount} folder{(folderCount == 1 ? "" : "s")} queued."
        };
    }
}

public partial class FileEntryViewModel : ObservableObject
{
    public string Path { get; }
    public string FileName => System.IO.Path.GetFileName(Path);
    public string Size { get; }
    [ObservableProperty] private BitmapSource? _thumbnail;

    public FileEntryViewModel(string path)
    {
        Path = path;
        var info = new FileInfo(path);
        Size = info.Exists ? FormatSize(info.Length) : "?";
        _ = LoadThumbnailAsync(path);
    }

    private async Task LoadThumbnailAsync(string path)
    {
        try
        {
            var bmp = await Task.Run(() =>
            {
                var bi = new BitmapImage();
                bi.BeginInit();
                bi.UriSource = new Uri(path);
                bi.DecodePixelHeight = 48;
                bi.CacheOption = BitmapCacheOption.OnLoad;
                bi.EndInit();
                bi.Freeze();
                return (BitmapSource)bi;
            });
            Thumbnail = bmp;
        }
        catch { /* unsupported format or bad file — placeholder stays */ }
    }

    private static string FormatSize(long bytes) => bytes switch
    {
        < 1024 => $"{bytes} B",
        < 1024 * 1024 => $"{bytes / 1024.0:F1} KB",
        _ => $"{bytes / (1024.0 * 1024):F1} MB",
    };
}
