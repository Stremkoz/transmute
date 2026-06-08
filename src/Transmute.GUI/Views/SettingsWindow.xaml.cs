using System.Windows;
using Microsoft.Win32;
using Transmute.GUI.ViewModels;

namespace Transmute.GUI.Views;

public partial class SettingsWindow : Window
{
    public SettingsWindow(SettingsViewModel vm)
    {
        InitializeComponent();
        DataContext = vm;
        vm.Saved += (_, _) => DialogResult = true;
    }

    private void BrowseDefaultOutput_Click(object sender, RoutedEventArgs e)
    {
        var dlg = new OpenFolderDialog { Title = "Select default output folder" };
        if (dlg.ShowDialog() == true && DataContext is SettingsViewModel vm)
            vm.DefaultOutputDirectory = dlg.FolderName;
    }

    private void ClearDefaultOutput_Click(object sender, RoutedEventArgs e)
    {
        if (DataContext is SettingsViewModel vm)
            vm.DefaultOutputDirectory = string.Empty;
    }
}
