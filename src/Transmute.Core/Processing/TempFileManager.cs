namespace Transmute.Core.Processing;

public class TempFileManager : IDisposable
{
    private readonly string _tempDir;
    private readonly List<string> _files = [];
    private bool _disposed;

    public TempFileManager(string? tempDir = null)
    {
        _tempDir = tempDir ?? Path.Combine(Path.GetTempPath(), "Transmute");
        Directory.CreateDirectory(_tempDir);
    }

    public string GetTempPath(string extension)
    {
        var path = Path.Combine(_tempDir, $"{Guid.NewGuid():N}.{extension.TrimStart('.')}");
        lock (_files) _files.Add(path);
        return path;
    }

    public void Cleanup()
    {
        lock (_files)
        {
            foreach (var f in _files)
            {
                try { if (File.Exists(f)) File.Delete(f); }
                catch { /* best-effort */ }
            }
            _files.Clear();
        }
    }

    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
        Cleanup();
    }
}
