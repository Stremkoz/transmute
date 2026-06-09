using System.IO;
using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Controls.Primitives;
using System.Windows.Documents;
using System.Windows.Input;
using System.Windows.Interop;
using System.Windows.Media;
using Microsoft.Win32;
using Transmute.GUI.ViewModels;
using Transmute.GUI.Views;

namespace Transmute.GUI;

public partial class MainWindow : Window
{
    private readonly MainViewModel _vm;
    private static readonly Brush DefaultDropBorder =
        new SolidColorBrush(Color.FromRgb(0xDC, 0xDF, 0xE6));

    // ── Drag-to-reorder state ────────────────────────────────────────────────
    private int    _dragSourceIndex = -1;
    private Point  _dragStart;
    private DropIndicatorAdorner? _dropAdorner;
    private List<int>? _dragSourceIndices;

    public MainWindow(MainViewModel vm)
    {
        InitializeComponent();
        _vm = vm;
        DataContext = vm;
        vm.LogLines.CollectionChanged += (_, _) =>
            Dispatcher.BeginInvoke(System.Windows.Threading.DispatcherPriority.Background,
                () => LogScrollViewer.ScrollToBottom());

        vm.PropertyChanged += (_, e) =>
        {
            if (e.PropertyName == nameof(MainViewModel.IsConverting) && !vm.IsConverting && !IsActive)
                FlashTaskbar();
        };
    }

    // ── Taskbar flash ────────────────────────────────────────────────────────

    [StructLayout(LayoutKind.Sequential)]
    private struct FLASHWINFO
    {
        public uint cbSize;
        public IntPtr hwnd;
        public uint dwFlags;
        public uint uCount;
        public uint dwTimeout;
    }

    private const uint FLASHW_TRAY = 2;

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool FlashWindowEx(ref FLASHWINFO pwfi);

    private void FlashTaskbar()
    {
        var hwnd = new WindowInteropHelper(this).Handle;
        if (hwnd == IntPtr.Zero) return;
        var fwi = new FLASHWINFO
        {
            cbSize    = (uint)Marshal.SizeOf<FLASHWINFO>(),
            hwnd      = hwnd,
            dwFlags   = FLASHW_TRAY,
            uCount    = 4,
            dwTimeout = 0,
        };
        FlashWindowEx(ref fwi);
    }

    // ── Split Clear button chevron ────────────────────────────────────────────

    private void ClearChevron_Click(object sender, RoutedEventArgs e)
    {
        if (sender is Button btn && btn.ContextMenu is ContextMenu cm)
        {
            cm.PlacementTarget = btn;
            cm.Placement = PlacementMode.Bottom;
            cm.IsOpen = true;
        }
    }

    // ── Drag-to-reorder ───────────────────────────────────────────────────────

    private void Gripper_MouseLeftButtonDown(object sender, MouseButtonEventArgs e)
    {
        if (_vm.IsConverting) return;
        if (sender is FrameworkElement fe && fe.DataContext is { } ctx)
        {
            var idx = _vm.InputFiles.IndexOf(ctx);
            if (idx >= 0)
            {
                _dragSourceIndex = idx;
                _dragStart = e.GetPosition(QueueList);
                _dragSourceIndices = QueueList.SelectedItems.Contains(ctx) && QueueList.SelectedItems.Count > 1
                    ? QueueList.SelectedItems.Cast<object>()
                        .Select(item => _vm.InputFiles.IndexOf(item))
                        .Where(i => i >= 0)
                        .OrderBy(i => i)
                        .ToList()
                    : null;
            }
        }
        e.Handled = true;
    }

    private void QueueList_PreviewMouseMove(object sender, MouseEventArgs e)
    {
        if (_dragSourceIndex < 0 || e.LeftButton != MouseButtonState.Pressed) return;

        var pos = e.GetPosition(QueueList);
        if (Math.Abs(pos.X - _dragStart.X) < SystemParameters.MinimumHorizontalDragDistance &&
            Math.Abs(pos.Y - _dragStart.Y) < SystemParameters.MinimumVerticalDragDistance) return;

        var item = _vm.InputFiles[_dragSourceIndex];
        DragDrop.DoDragDrop(QueueList, item, DragDropEffects.Move);

        // DoDragDrop is synchronous — clean up after it returns
        _dragSourceIndex = -1;
        _dragSourceIndices = null;
        RemoveDropIndicator();
    }

    private void QueueList_DragOver(object sender, DragEventArgs e)
    {
        if (e.Data.GetDataPresent(DataFormats.FileDrop))
        {
            e.Effects = DragDropEffects.Copy;
            e.Handled = true;
            return;
        }
        if (_dragSourceIndex < 0)
        {
            e.Effects = DragDropEffects.None;
            e.Handled = true;
            return;
        }
        e.Effects = DragDropEffects.Move;
        e.Handled = true;
        var (toIndex, y) = GetDropInfo(e);
        if (_dragSourceIndices != null)
        {
            ShowDropIndicator(y);
        }
        else
        {
            var adjusted = toIndex > _dragSourceIndex ? toIndex - 1 : toIndex;
            if (adjusted == _dragSourceIndex) RemoveDropIndicator();
            else ShowDropIndicator(y);
        }
    }

    private void QueueList_DragLeave(object sender, DragEventArgs e)
    {
        RemoveDropIndicator();
    }

    private void QueueList_Drop(object sender, DragEventArgs e)
    {
        RemoveDropIndicator();
        if (_dragSourceIndex < 0) return;

        var (toIndex, _) = GetDropInfo(e);

        if (_dragSourceIndices != null)
        {
            var movedItems = _dragSourceIndices.Select(i => _vm.InputFiles[i]).ToList();
            _vm.MoveEntries(_dragSourceIndices, toIndex);
            Dispatcher.BeginInvoke(System.Windows.Threading.DispatcherPriority.DataBind, () =>
            {
                QueueList.SelectedItems.Clear();
                foreach (var item in movedItems)
                    if (QueueList.Items.Contains(item))
                        QueueList.SelectedItems.Add(item);
            });
        }
        else
        {
            _vm.MoveEntry(_dragSourceIndex, toIndex);
        }

        _dragSourceIndex = -1;
        _dragSourceIndices = null;
        e.Handled = true;
    }

    private (int index, double y) GetDropInfo(DragEventArgs e)
    {
        var pos   = e.GetPosition(QueueList);
        var count = QueueList.Items.Count;

        for (int i = 0; i < count; i++)
        {
            if (QueueList.ItemContainerGenerator.ContainerFromIndex(i) is not ListViewItem container) continue;
            var bounds = container.TransformToAncestor(QueueList)
                .TransformBounds(new Rect(0, 0, container.ActualWidth, container.ActualHeight));
            if (pos.Y < bounds.Y + bounds.Height / 2)
                return (i, bounds.Y);
        }

        // Past the last item
        if (count > 0 && QueueList.ItemContainerGenerator.ContainerFromIndex(count - 1) is ListViewItem last)
        {
            var b = last.TransformToAncestor(QueueList)
                .TransformBounds(new Rect(0, 0, last.ActualWidth, last.ActualHeight));
            return (count, b.Bottom);
        }
        return (0, 0);
    }

    private void ShowDropIndicator(double y)
    {
        var layer = AdornerLayer.GetAdornerLayer(QueueList);
        if (layer == null) return;
        if (_dropAdorner == null)
        {
            _dropAdorner = new DropIndicatorAdorner(QueueList);
            layer.Add(_dropAdorner);
        }
        _dropAdorner.SetY(y);
    }

    private void RemoveDropIndicator()
    {
        if (_dropAdorner == null) return;
        var layer = AdornerLayer.GetAdornerLayer(QueueList);
        layer?.Remove(_dropAdorner);
        _dropAdorner = null;
    }

    private sealed class DropIndicatorAdorner : Adorner
    {
        private double _y;
        private static readonly Pen Pen = new(Brushes.DodgerBlue, 2)
        {
            StartLineCap = PenLineCap.Round,
            EndLineCap   = PenLineCap.Round,
        };

        public DropIndicatorAdorner(UIElement element) : base(element)
        {
            IsHitTestVisible = false;
            Pen.Freeze();
        }

        public void SetY(double y) { _y = y; InvalidateVisual(); }

        protected override void OnRender(DrawingContext dc)
        {
            const double margin = 10;
            dc.DrawLine(Pen, new Point(margin, _y), new Point(ActualWidth - margin, _y));
            dc.DrawEllipse(Brushes.DodgerBlue, null, new Point(margin, _y), 4, 4);
        }
    }

    // ── Existing drag-and-drop (files/folders from outside) ──────────────────

    private void Window_DragEnter(object sender, DragEventArgs e)
    {
        e.Effects = e.Data.GetDataPresent(DataFormats.FileDrop)
            ? DragDropEffects.Copy
            : DragDropEffects.None;
    }

    private void Window_Drop(object sender, DragEventArgs e)
    {
        if (e.Data.GetData(DataFormats.FileDrop) is string[] paths)
            HandleDroppedPaths(paths);
    }

    private void DropZone_DragEnter(object sender, DragEventArgs e)
    {
        if (sender is Border b)
        {
            b.BorderBrush = Brushes.DodgerBlue;
            b.BorderThickness = new Thickness(2);
        }
        e.Effects = e.Data.GetDataPresent(DataFormats.FileDrop)
            ? DragDropEffects.Copy
            : DragDropEffects.None;
    }

    private void DropZone_DragLeave(object sender, DragEventArgs e)
    {
        if (sender is Border b)
        {
            b.BorderBrush = DefaultDropBorder;
            b.BorderThickness = new Thickness(1);
        }
    }

    private void DropZone_Drop(object sender, DragEventArgs e)
    {
        DropZone_DragLeave(sender, e);
        if (e.Data.GetData(DataFormats.FileDrop) is string[] paths)
            HandleDroppedPaths(paths);
    }

    private void HandleDroppedPaths(string[] paths)
    {
        var files   = paths.Where(File.Exists).ToArray();
        var folders = paths.Where(Directory.Exists).ToArray();
        if (files.Length > 0) _vm.AddFiles(files);
        foreach (var folder in folders)
            _vm.AddFolder(folder, _vm.IncludeSubfolders);
    }

    // ── Toolbar buttons ───────────────────────────────────────────────────────

    private void AddFiles_Click(object sender, RoutedEventArgs e)
    {
        var dlg = new OpenFileDialog
        {
            Multiselect = true,
            Title = "Select images to convert",
            Filter = "Image files|*.jpg;*.jpeg;*.png;*.webp;*.avif;*.jxl;*.tiff;*.tif;" +
                     "*.gif;*.bmp;*.heic;*.heif;*.hdr;*.jp2|All files|*.*"
        };
        if (dlg.ShowDialog() == true)
            _vm.AddFiles(dlg.FileNames);
    }

    private void AddFolder_Click(object sender, RoutedEventArgs e)
    {
        var dlg = new OpenFolderDialog
        {
            Title = "Select folder to convert",
            Multiselect = true,
        };
        if (dlg.ShowDialog() == true)
        {
            foreach (var folder in dlg.FolderNames)
                _vm.AddFolder(folder, _vm.IncludeSubfolders);
        }
    }

    private void Window_PreviewKeyDown(object sender, KeyEventArgs e)
    {
        if (e.Key == Key.Delete && !_vm.IsConverting && QueueList.SelectedItems.Count > 0)
        {
            _vm.RemoveEntries(QueueList.SelectedItems.Cast<object>().ToList());
            e.Handled = true;
        }
    }

    // ── Queue context menu ────────────────────────────────────────────────────

    private void QueueList_PreviewMouseRightButtonDown(object sender, MouseButtonEventArgs e)
    {
        var element = e.OriginalSource as DependencyObject;
        while (element is not null and not ListViewItem)
            element = VisualTreeHelper.GetParent(element);

        if (element is ListViewItem lvi && lvi.DataContext is { } clickedItem
            && !QueueList.SelectedItems.Contains(clickedItem))
        {
            QueueList.SelectedItem = clickedItem;
        }
    }

    private void QueueList_ContextMenuOpening(object sender, ContextMenuEventArgs e)
    {
        if (QueueList.ContextMenu is not ContextMenu cm) return;
        int sel = QueueList.SelectedItems.Count;
        foreach (var mi in cm.Items.OfType<MenuItem>())
        {
            switch (mi.Tag as string)
            {
                case "remove":
                    mi.IsEnabled = sel > 0;
                    mi.Header = sel > 1 ? $"Remove ({sel} items)" : "Remove";
                    break;
                case "openfolder":
                    mi.IsEnabled = sel == 1;
                    break;
                case "selectall":
                case "clear":
                    mi.IsEnabled = _vm.InputFiles.Count > 0;
                    break;
            }
        }
    }

    private void QueueCtx_Click(object sender, RoutedEventArgs e)
    {
        if (sender is not MenuItem mi) return;
        switch (mi.Tag as string)
        {
            case "remove":
                _vm.RemoveEntries(QueueList.SelectedItems.Cast<object>().ToList());
                break;
            case "openfolder":
                string? dir = QueueList.SelectedItem switch
                {
                    FileEntryViewModel fvm   => System.IO.Path.GetDirectoryName(fvm.Path),
                    FolderEntryViewModel fld => fld.Path,
                    _                        => null
                };
                if (dir != null)
                    try { System.Diagnostics.Process.Start("explorer.exe", $"\"{dir}\""); }
                    catch { }
                break;
            case "selectall":
                QueueList.SelectAll();
                break;
            case "clear":
                _vm.ClearFilesCommand.Execute(null);
                break;
        }
    }

    private void BrowseOutputDir_Click(object sender, RoutedEventArgs e)
    {
        var dlg = new OpenFolderDialog { Title = "Select output folder for this session" };
        if (dlg.ShowDialog() == true)
            _vm.OutputDirectory = dlg.FolderName;
    }

    private void OpenOutputDir_Click(object sender, RoutedEventArgs e)
    {
        var dir = _vm.OutputDirectory;
        if (string.IsNullOrEmpty(dir)) return;
        try { System.Diagnostics.Process.Start("explorer.exe", dir); }
        catch { /* folder may not exist yet */ }
    }

    private void OpenSettings_Click(object sender, RoutedEventArgs e)
    {
        var app = (App)Application.Current;
        try
        {
            var settingsVm = new SettingsViewModel(app.ConfigManager, app.ProfileManager, _vm.ActiveProfile);
            var win = new SettingsWindow(settingsVm) { Owner = this };
            if (win.ShowDialog() == true)
            {
                if (settingsVm.SelectedProfile != _vm.ActiveProfile)
                    _vm.ActiveProfile = settingsVm.SelectedProfile;
            }
        }
        catch (Exception ex)
        {
            MessageBox.Show(
                $"Failed to open Settings:\n\n{ex.GetType().Name}: {ex.Message}",
                "Transmute — Error",
                MessageBoxButton.OK,
                MessageBoxImage.Error);
        }
    }

    private void OpenProfiles_Click(object sender, RoutedEventArgs e)
    {
        var app = (App)Application.Current;
        var pmVm = new ProfileManagerViewModel(app.ProfileManager);
        pmVm.SelectedProfile = _vm.ActiveProfile;

        var win = new ProfileManagerWindow(pmVm, app.ProfileManager) { Owner = this };
        win.ShowDialog();

        _vm.RefreshProfiles();
        if (win.SelectedProfileOnClose is { } selected)
            _vm.ActiveProfile = selected;
    }
}
