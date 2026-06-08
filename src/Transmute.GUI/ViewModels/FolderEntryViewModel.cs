using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Threading.Tasks;
using CommunityToolkit.Mvvm.ComponentModel;

namespace Transmute.GUI.ViewModels;

public partial class FolderEntryViewModel : ObservableObject
{
    public static readonly HashSet<string> ImageExtensions = new(StringComparer.OrdinalIgnoreCase)
    {
        ".jpg", ".jpeg", ".png", ".webp", ".avif", ".jxl", ".tiff", ".tif",
        ".gif", ".bmp", ".heic", ".heif", ".svg", ".hdr", ".jp2", ".j2k"
    };

    [ObservableProperty] private string _countText = "Counting...";

    public string Path { get; }
    public string FolderName => System.IO.Path.GetFileName(Path.TrimEnd('\\', '/'));
    public bool IncludeSubfolders { get; }

    public FolderEntryViewModel(string path, bool includeSubfolders)
    {
        Path = path;
        IncludeSubfolders = includeSubfolders;
        _ = CountAsync();
    }

    private async Task CountAsync()
    {
        var count = await Task.Run(() =>
            Directory.EnumerateFiles(Path, "*",
                IncludeSubfolders ? SearchOption.AllDirectories : SearchOption.TopDirectoryOnly)
            .Count(f => ImageExtensions.Contains(System.IO.Path.GetExtension(f))));

        CountText = $"{count:N0} image{(count == 1 ? "" : "s")}" +
                    (IncludeSubfolders ? " · subfolders included" : "");
    }

    public IEnumerable<string> GetImagePaths() =>
        Directory.EnumerateFiles(Path, "*",
            IncludeSubfolders ? SearchOption.AllDirectories : SearchOption.TopDirectoryOnly)
        .Where(f => ImageExtensions.Contains(System.IO.Path.GetExtension(f)));
}
