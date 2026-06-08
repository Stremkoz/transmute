using System.Collections.ObjectModel;
using System.IO;
using System.Threading;
using System.Threading.Tasks;
using System.Windows;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Transmute.Core;
using Transmute.Core.Config;
using Transmute.Core.Models;

namespace Transmute.GUI.ViewModels;

public partial class MainViewModel : ObservableObject
{
    private readonly ConfigManager _configManager;

    [ObservableProperty] private string _targetFormat = "webp";
    [ObservableProperty] private int _quality = 85;
    [ObservableProperty] private bool _preserveMetadata = true;
    [ObservableProperty] private bool _overwriteExisting = false;
    [ObservableProperty] private string? _outputDirectory;
    [ObservableProperty] private bool _isConverting = false;
    [ObservableProperty] private int _progressValue = 0;
    [ObservableProperty] private int _progressMax = 1;
    [ObservableProperty] private string _statusText = "Drop files here to get started.";

    public ObservableCollection<FileEntryViewModel> InputFiles { get; } = [];
    public ObservableCollection<string> LogLines { get; } = [];

    public static string[] SupportedFormats { get; } =
    [
        "webp", "avif", "jxl", "png", "jpg", "tiff", "heic", "bmp", "gif"
    ];

    private CancellationTokenSource? _cts;

    public MainViewModel(ConfigManager configManager)
    {
        _configManager = configManager;
        var defaults = configManager.Config.Defaults;
        _quality = defaults.WebpQuality;
        _preserveMetadata = defaults.PreserveMetadata;
        _overwriteExisting = defaults.OverwriteExisting;
        InputFiles.CollectionChanged += (_, _) => ConvertCommand.NotifyCanExecuteChanged();
    }

    partial void OnTargetFormatChanged(string value)
    {
        Quality = _configManager.Config.Defaults switch
        {
            { } d when value == "webp" => d.WebpQuality,
            { } d when value is "jpg" or "jpeg" => d.JpegQuality,
            { } d when value == "jxl" => d.JxlQuality,
            { } d when value == "avif" => d.AvifQuality,
            _ => 90
        };
    }

    public void AddFiles(IEnumerable<string> paths)
    {
        foreach (var path in paths)
        {
            if (File.Exists(path) && InputFiles.All(f => f.Path != path))
                InputFiles.Add(new FileEntryViewModel(path));
        }
        StatusText = $"{InputFiles.Count} file(s) queued.";
    }

    [RelayCommand]
    private void RemoveFile(FileEntryViewModel? entry)
    {
        if (entry is not null)
            InputFiles.Remove(entry);
        StatusText = $"{InputFiles.Count} file(s) queued.";
    }

    [RelayCommand]
    private void ClearFiles()
    {
        InputFiles.Clear();
        StatusText = "Files cleared.";
    }

    [RelayCommand(CanExecute = nameof(CanConvert))]
    private async Task ConvertAsync()
    {
        if (InputFiles.Count == 0) return;

        _cts = new CancellationTokenSource();
        IsConverting = true;
        ProgressValue = 0;
        ProgressMax = InputFiles.Count;
        LogLines.Clear();
        StatusText = "Converting...";

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
            var jobs = InputFiles.Select(f => new ConversionJob
            {
                InputPath = f.Path,
                OutputPath = engine.ResolveOutputPath(f.Path, TargetFormat, options),
                OutputFormat = TargetFormat,
                Options = options,
            }).ToList();

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
                            ? $"Converting... ({p.Completed}/{p.Total})"
                            : $"Done: {p.Completed - p.Failed} succeeded, {p.Failed} failed.";
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
