using Avalonia;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.Platform.Storage;
using Avalonia.Threading;
using Avalonia.VisualTree;
using Transmute.Core.Config;
using Transmute.Avalonia.Services;
using Transmute.Avalonia.ViewModels;

namespace Transmute.Avalonia.Views;

public partial class MainWindow : Window
{
    private static readonly DataFormat<string> QueueDragFormat =
        DataFormat.CreateStringApplicationFormat("transmute/queue-entry");

    private readonly MainViewModel _vm;

    /// <summary>Design-time constructor; not used at runtime.</summary>
    public MainWindow() : this(new MainViewModel(new ConfigManager(), new ProfileManager()))
    {
    }

    public MainWindow(MainViewModel vm)
    {
        InitializeComponent();
        _vm = vm;
        DataContext = vm;

        vm.LogLines.CollectionChanged += (_, _) =>
            Dispatcher.UIThread.Post(() => LogScrollViewer.ScrollToEnd(), DispatcherPriority.Background);

        // External file/folder drops anywhere on the window or onto the queue
        AddHandler(DragDrop.DragOverEvent, Window_DragOver);
        AddHandler(DragDrop.DropEvent, Window_Drop);
        QueueList.AddHandler(DragDrop.DragOverEvent, QueueList_DragOver);
        QueueList.AddHandler(DragDrop.DropEvent, QueueList_Drop);

        KeyDown += Window_KeyDown;
    }

    // ── External drag-and-drop (files/folders from the OS) ──────────────────

    private void Window_DragOver(object? sender, DragEventArgs e)
    {
        e.DragEffects = e.DataTransfer.Contains(DataFormat.File)
            ? DragDropEffects.Copy
            : e.DataTransfer.Contains(QueueDragFormat) ? DragDropEffects.Move : DragDropEffects.None;
        e.Handled = true;
    }

    private void Window_Drop(object? sender, DragEventArgs e)
    {
        if (!e.DataTransfer.Contains(DataFormat.File)) return;
        HandleDroppedItems(e.DataTransfer.TryGetFiles());
        e.Handled = true;
    }

    private void HandleDroppedItems(IEnumerable<IStorageItem>? items)
    {
        if (items is null) return;

        var files = new List<string>();
        foreach (var item in items)
        {
            if (item.Path is not { IsAbsoluteUri: true } uri) continue;
            var path = uri.LocalPath;
            if (string.IsNullOrWhiteSpace(path)) continue;

            if (item is IStorageFolder || Directory.Exists(path))
                _vm.AddFolder(path, _vm.IncludeSubfolders);
            else if (File.Exists(path))
                files.Add(path);
        }
        if (files.Count > 0)
            _vm.AddFiles(files);
    }

    // ── Drag-to-reorder via the row gripper ──────────────────────────────────

    private async void Gripper_PointerPressed(object? sender, PointerPressedEventArgs e)
    {
        if (_vm.IsConverting) return;
        if (sender is not Control { DataContext: { } ctx }) return;

        var index = _vm.InputFiles.IndexOf(ctx);
        if (index < 0) return;

        // Multi-drag when the gripped row is part of a multi-selection
        var indices = QueueList.SelectedItems is { Count: > 1 } selected && selected.Contains(ctx)
            ? selected.Cast<object>()
                .Select(item => _vm.InputFiles.IndexOf(item))
                .Where(i => i >= 0)
                .OrderBy(i => i)
                .ToList()
            : [index];

        var dataTransfer = new DataTransfer();
        dataTransfer.Add(DataTransferItem.Create(QueueDragFormat, string.Join(",", indices)));
        e.Handled = true;
        await DragDrop.DoDragDropAsync(e, dataTransfer, DragDropEffects.Move);
    }

    private void QueueList_DragOver(object? sender, DragEventArgs e)
    {
        if (e.DataTransfer.Contains(DataFormat.File))
        {
            e.DragEffects = DragDropEffects.Copy;
        }
        else if (e.DataTransfer.Contains(QueueDragFormat))
        {
            e.DragEffects = DragDropEffects.Move;
            e.Handled = true; // keep the window-level handler from overriding
        }
        else
        {
            e.DragEffects = DragDropEffects.None;
        }
    }

    private void QueueList_Drop(object? sender, DragEventArgs e)
    {
        if (e.DataTransfer.Contains(DataFormat.File)) return; // window handler adds external files

        if (e.DataTransfer.TryGetValue(QueueDragFormat) is not { } payload) return;

        var indices = payload.Split(',').Select(int.Parse).ToList();
        if (indices.Count == 0) return;

        var toIndex = GetDropIndex(e);
        var movedItems = indices.Select(i => _vm.InputFiles[i]).ToList();

        if (indices.Count == 1)
            _vm.MoveEntry(indices[0], toIndex);
        else
            _vm.MoveEntries(indices, toIndex);

        Dispatcher.UIThread.Post(() =>
        {
            QueueList.SelectedItems?.Clear();
            foreach (var item in movedItems)
                QueueList.SelectedItems?.Add(item);
        });

        e.Handled = true;
    }

    private int GetDropIndex(DragEventArgs e)
    {
        var pos = e.GetPosition(QueueList);
        var count = _vm.InputFiles.Count;

        for (int i = 0; i < count; i++)
        {
            if (QueueList.ContainerFromIndex(i) is not Control container) continue;
            var topLeft = container.TranslatePoint(new global::Avalonia.Point(0, 0), QueueList);
            if (topLeft is null) continue;
            if (pos.Y < topLeft.Value.Y + container.Bounds.Height / 2)
                return i;
        }
        return count;
    }

    // ── Keyboard ─────────────────────────────────────────────────────────────

    private void Window_KeyDown(object? sender, KeyEventArgs e)
    {
        if (e.Key == Key.Delete && !_vm.IsConverting && QueueList.SelectedItems is { Count: > 0 } selected)
        {
            _vm.RemoveEntries(selected.Cast<object>().ToList());
            e.Handled = true;
        }
    }

    // ── Queue context menu ────────────────────────────────────────────────────

    private void QueueContextMenu_Opened(object? sender, RoutedEventArgs e)
    {
        int sel = QueueList.SelectedItems?.Count ?? 0;
        CtxRemove.IsEnabled = sel > 0;
        CtxRemove.Header = sel > 1 ? $"Remove ({sel} items)" : "Remove";
        CtxOpenFolder.IsEnabled = sel == 1;
        CtxSelectAll.IsEnabled = _vm.InputFiles.Count > 0;
        CtxClear.IsEnabled = _vm.InputFiles.Count > 0;
    }

    private void QueueCtxRemove_Click(object? sender, RoutedEventArgs e)
    {
        if (QueueList.SelectedItems is { Count: > 0 } selected)
            _vm.RemoveEntries(selected.Cast<object>().ToList());
    }

    private void QueueCtxOpenFolder_Click(object? sender, RoutedEventArgs e)
    {
        string? dir = QueueList.SelectedItem switch
        {
            FileEntryViewModel fvm   => Path.GetDirectoryName(fvm.Path),
            FolderEntryViewModel fld => fld.Path,
            _                        => null
        };
        if (dir is not null)
            Platform.OpenFolder(dir);
    }

    private void QueueCtxSelectAll_Click(object? sender, RoutedEventArgs e) =>
        QueueList.SelectAll();

    // ── Toolbar / pickers ─────────────────────────────────────────────────────

    private async void AddFiles_Click(object? sender, RoutedEventArgs e)
    {
        var files = await StorageProvider.OpenFilePickerAsync(new FilePickerOpenOptions
        {
            Title = "Select images to convert",
            AllowMultiple = true,
            FileTypeFilter =
            [
                new FilePickerFileType("Image files")
                {
                    Patterns =
                    [
                        "*.jpg", "*.jpeg", "*.png", "*.webp", "*.avif", "*.jxl", "*.tiff", "*.tif",
                        "*.gif", "*.bmp", "*.heic", "*.heif", "*.hdr", "*.jp2"
                    ]
                },
                FilePickerFileTypes.All,
            ],
        });

        var paths = files
            .Select(f => f.Path is { IsAbsoluteUri: true } uri ? uri.LocalPath : null)
            .Where(p => !string.IsNullOrWhiteSpace(p))
            .Cast<string>()
            .ToList();
        if (paths.Count > 0)
            _vm.AddFiles(paths);
    }

    private async void AddFolder_Click(object? sender, RoutedEventArgs e)
    {
        var folders = await StorageProvider.OpenFolderPickerAsync(new FolderPickerOpenOptions
        {
            Title = "Select folder to convert",
            AllowMultiple = true,
        });

        foreach (var folder in folders)
            if (folder.Path is { IsAbsoluteUri: true } uri)
                _vm.AddFolder(uri.LocalPath, _vm.IncludeSubfolders);
    }

    private async void BrowseOutputDir_Click(object? sender, RoutedEventArgs e)
    {
        var folders = await StorageProvider.OpenFolderPickerAsync(new FolderPickerOpenOptions
        {
            Title = "Select output folder for this session",
            AllowMultiple = false,
        });

        if (folders.FirstOrDefault()?.Path is { IsAbsoluteUri: true } uri)
            _vm.OutputDirectory = uri.LocalPath;
    }

    private async void OpenSettings_Click(object? sender, RoutedEventArgs e)
    {
        var app = App.Current;
        try
        {
            var settingsVm = new SettingsViewModel(app.ConfigManager, app.ProfileManager, _vm.ActiveProfile);
            var win = new SettingsWindow(settingsVm);
            await win.ShowDialog(this);

            if (settingsVm.SelectedProfile != _vm.ActiveProfile)
                _vm.ActiveProfile = settingsVm.SelectedProfile;
        }
        catch (Exception ex)
        {
            await MessageDialog.ShowAsync(
                $"Failed to open Settings:\n\n{ex.GetType().Name}: {ex.Message}",
                "Transmute — Error",
                MessageIcon.Error);
        }
    }

    private async void OpenProfiles_Click(object? sender, RoutedEventArgs e)
    {
        var app = App.Current;
        var pmVm = new ProfileManagerViewModel(app.ProfileManager)
        {
            SelectedProfile = _vm.ActiveProfile,
        };

        var win = new ProfileManagerWindow(pmVm);
        await win.ShowDialog(this);

        _vm.RefreshProfiles();
        if (win.SelectedProfileOnClose is { } selected)
            _vm.ActiveProfile = selected;
    }
}
