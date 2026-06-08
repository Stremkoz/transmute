using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Collections.Specialized;
using System.ComponentModel;
using System.IO;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using System.Windows;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Transmute.Core;
using Transmute.Core.Config;
using Transmute.Core.Models;

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
    private bool _qualityUserCustomized = false;
    private bool _suppressQualityTracking = false;

    [ObservableProperty] private string _targetFormat = "webp";
    [ObservableProperty] private int _quality = 85;
    [ObservableProperty] private bool _preserveMetadata = true;
    [ObservableProperty] private bool _overwriteExisting = false;
    [ObservableProperty] private string? _outputDirectory;
    [ObservableProperty] private bool _isConverting = false;
    [ObservableProperty] private int _progressValue = 0;
    [ObservableProperty] private int _progressMax = 1;
    [ObservableProperty] private string _statusText = "Drop images or folders here to get started.";
    [ObservableProperty] private bool _includeSubfolders = true;

    public BulkObservableCollection<object> InputFiles { get; } = new();
    public ObservableCollection<string> LogLines { get; } = new();

    public static string[] SupportedFormats { get; } =
    [
        "webp", "avif", "jxl", "png", "jpg", "tiff", "heic", "bmp", "gif"
    ];

    private CancellationTokenSource? _cts;

    public MainViewModel(ConfigManager configManager)
    {
        _configManager = configManager;
        var defaults = configManager.Config.Defaults;
        _suppressQualityTracking = true;
        _quality = defaults.WebpQuality;
        _suppressQualityTracking = false;
        _preserveMetadata = defaults.PreserveMetadata;
        _overwriteExisting = defaults.OverwriteExisting;
        InputFiles.CollectionChanged += (_, _) => ConvertCommand.NotifyCanExecuteChanged();
    }

    partial void OnTargetFormatChanged(string value)
    {
        if (!_qualityUserCustomized)
        {
            _suppressQualityTracking = true;
            Quality = _configManager.Config.Defaults switch
            {
                { } d when value == "webp" => d.WebpQuality,
                { } d when value is "jpg" or "jpeg" => d.JpegQuality,
                { } d when value == "jxl" => d.JxlQuality,
                { } d when value == "avif" => d.AvifQuality,
                _ => 90
            };
            _suppressQualityTracking = false;
        }
        _qualityUserCustomized = false;
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
        LogLines.Clear();
        StatusText = "Drop images or folders here to get started.";
    }

    [RelayCommand(CanExecute = nameof(CanConvert))]
    private async Task ConvertAsync()
    {
        if (InputFiles.Count == 0) return;

        _cts = new CancellationTokenSource();
        IsConverting = true;
        ProgressValue = 0;
        LogLines.Clear();
        StatusText = "Preparing files...";

        var options = new ConversionOptions
        {
            Quality = Quality,
            PreserveMetadata = PreserveMetadata,
            Overwrite = OverwriteExisting,
            OutputDirectory = OutputDirectory,
            OutputNamingPattern = _configManager.Config.Defaults.OutputNamingPattern,
        };

        var config = _configManager.Config;
        var (engine, _, temp, _) = TransmuteFactory.Create(config);

        using (temp)
        {
            // Expand all queue entries (files + folders) into a flat job list
            var jobs = new List<ConversionJob>();
            foreach (var entry in InputFiles)
            {
                IEnumerable<string> filePaths = entry switch
                {
                    FileEntryViewModel fvm => [fvm.Path],
                    FolderEntryViewModel folderVm => folderVm.GetImagePaths(),
                    _ => []
                };

                foreach (var path in filePaths)
                    jobs.Add(new ConversionJob
                    {
                        InputPath = path,
                        OutputPath = engine.ResolveOutputPath(path, TargetFormat, options),
                        OutputFormat = TargetFormat,
                        Options = options,
                    });
            }

            ProgressMax = Math.Max(1, jobs.Count);
            StatusText = $"Converting {jobs.Count:N0} file(s)...";

            var progress = new Progress<ConversionProgress>(p =>
            {
                Application.Current?.Dispatcher.Invoke(() =>
                {
                    ProgressValue = p.Completed;
                    if (p.LastResult is { } r)
                    {
                        var line = r.Success
                            ? $"✓ {Path.GetFileName(r.InputPath)} → {Path.GetFileName(r.OutputPath)} [{r.BackendUsed}] {r.Elapsed.TotalSeconds:F2}s"
                            : $"✗ {Path.GetFileName(r.InputPath)}: {r.Error}";
                        LogLines.Add(line);
                        StatusText = p.Completed < p.Total
                            ? $"Converting... ({p.Completed:N0} / {p.Total:N0})"
                            : $"Done: {p.Completed - p.Failed:N0} succeeded, {p.Failed:N0} failed.";
                    }
                });
            });

            try
            {
                await engine.ConvertAllAsync(jobs, progress, _cts.Token);
            }
            catch (OperationCanceledException)
            {
                StatusText = "Conversion cancelled.";
                LogLines.Add("— Cancelled —");
            }
        }

        ProgressValue = ProgressMax;
        IsConverting = false;
        ConvertCommand.NotifyCanExecuteChanged();
    }

    private bool CanConvert() => !IsConverting && InputFiles.Count > 0;

    [RelayCommand]
    private void Cancel() => _cts?.Cancel();

    partial void OnIsConvertingChanged(bool value) =>
        ConvertCommand.NotifyCanExecuteChanged();

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

    public FileEntryViewModel(string path)
    {
        Path = path;
        var info = new FileInfo(path);
        Size = info.Exists ? FormatSize(info.Length) : "?";
    }

    private static string FormatSize(long bytes) => bytes switch
    {
        < 1024 => $"{bytes} B",
        < 1024 * 1024 => $"{bytes / 1024.0:F1} KB",
        _ => $"{bytes / (1024.0 * 1024):F1} MB",
    };
}
