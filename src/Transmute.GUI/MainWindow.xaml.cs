using System.IO;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Media;
using Microsoft.Win32;
using Transmute.GUI.ViewModels;

namespace Transmute.GUI;

public partial class MainWindow : Window
{
    private readonly MainViewModel _vm;
    private static readonly Brush DefaultDropBorder =
        new SolidColorBrush(Color.FromRgb(0xDC, 0xDF, 0xE6));

    public MainWindow(MainViewModel vm)
    {
        InitializeComponent();
        _vm = vm;
        DataContext = vm;
        vm.LogLines.CollectionChanged += (_, _) =>
            Dispatcher.BeginInvoke(System.Windows.Threading.DispatcherPriority.Background,
                () => LogScrollViewer.ScrollToBottom());
    }

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
        var files = paths.Where(File.Exists).ToArray();
        var folders = paths.Where(Directory.Exists).ToArray();

        if (files.Length > 0)
            _vm.AddFiles(files);

        foreach (var folder in folders)
            _vm.AddFolder(folder, _vm.IncludeSubfolders);
    }

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
        if (e.Key == Key.Delete && QueueList.SelectedItem is { } item)
        {
            _vm.RemoveEntryCommand.Execute(item);
            e.Handled = true;
        }
    }

    private void BrowseOutputDir_Click(object sender, RoutedEventArgs e)
    {
        var dlg = new OpenFolderDialog { Title = "Select output folder for this session" };
        if (dlg.ShowDialog() == true)
            _vm.OutputDirectory = dlg.FolderName;
    }

    private void OpenSettings_Click(object sender, RoutedEventArgs e)
    {
        try
        {
            var settingsVm = new SettingsViewModel(((App)Application.Current).ConfigManager);
            var win = new Views.SettingsWindow(settingsVm) { Owner = this };
            win.ShowDialog();
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
}
