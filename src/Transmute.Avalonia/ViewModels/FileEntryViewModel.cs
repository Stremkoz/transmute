using Avalonia.Media.Imaging;
using CommunityToolkit.Mvvm.ComponentModel;

namespace Transmute.Avalonia.ViewModels;

public partial class FileEntryViewModel : ObservableObject
{
    public string Path { get; }
    public string FileName => System.IO.Path.GetFileName(Path);
    public string Size { get; }
    [ObservableProperty] private Bitmap? _thumbnail;

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
            // Skia decodes png/jpg/webp/gif/bmp; other formats fail silently and keep the placeholder
            var bmp = await Task.Run(() =>
            {
                using var stream = File.OpenRead(path);
                return Bitmap.DecodeToHeight(stream, 48);
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
