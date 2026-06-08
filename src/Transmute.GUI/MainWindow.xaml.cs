using System.Windows;
using Microsoft.Win32;
using Transmute.GUI.ViewModels;

namespace Transmute.GUI;

public partial class MainWindow : Window
{
    private readonly MainViewModel _vm;

    public MainWindow(MainViewModel vm)
    {
        InitializeComponent();
        _vm = vm;
        DataContext = vm;
    }

    private void Window_DragEnter(object sender, DragEventArgs e)
    {
        e.Effects = e.Data.GetDataPresent(DataFormats.FileDrop) ? DragDropEffects.Copy : DragDropEffects.None;
    }

    private void Window_Drop(object sender, DragEventArgs e)
    {
        if (e.Data.GetData(DataFormats.FileDrop) is string[] files)
            _vm.AddFiles(files);
    }

    private void DropZone_DragEnter(object sender, DragEventArgs e)
    {
        if (sender is System.Windows.Controls.Border border)
            border.BorderBrush = System.Windows.Media.Brushes.DodgerBlue;
        e.Effects = e.Data.GetDataPresent(DataFormats.FileDrop) ? DragDropEffects.Copy : DragDropEffects.None;
    }

    private void DropZone_DragLeave(object sender, DragEventArgs e)
    {
        if (sender is System.Windows.Controls.Border border)
            border.BorderBrush = new System.Windows.Media.SolidColorBrush(
                System.Windows.Media.Color.FromRgb(0xD0, 0xD0, 0xD0));
    }

    private void DropZone_Drop(object sender, DragEventArgs e)
    {
        DropZone_DragLeave(sender, e);
        if (e.Data.GetData(DataFormats.FileDrop) is string[] files)
            _vm.AddFiles(files);
    }

    private void AddFiles_Click(object sender, RoutedEventArgs e)
    {
        var dlg = new OpenFileDialog
        {
            Multiselect = true,
            Title = "Select images to convert",
            Filter = "Image files|*.jpg;*.jpeg;*.png;*.webp;*.avif;*.jxl;*.tiff;*.tif;*.gif;*.bmp;*.heic;*.heif|All files|*.*"
        };

        if (dlg.ShowDialog() == true)
            _vm.AddFiles(dlg.FileNames);
    }

    private void OpenSettings_Click(object sender, RoutedEventArgs e)
    {
        var settingsVm = new SettingsViewModel(((App)Application.Current).ConfigManager);
        var win = new Views.SettingsWindow(settingsVm) { Owner = this };
        win.ShowDialog();
    }
}
