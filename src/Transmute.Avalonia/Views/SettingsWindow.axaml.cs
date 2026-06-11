using Avalonia.Controls;
using Avalonia.Interactivity;
using Avalonia.Platform.Storage;
using Transmute.Core.Config;
using Transmute.Avalonia.ViewModels;

namespace Transmute.Avalonia.Views;

public partial class SettingsWindow : Window
{
    private readonly SettingsViewModel _vm;

    /// <summary>Design-time constructor; not used at runtime.</summary>
    public SettingsWindow() : this(new SettingsViewModel(
        new ConfigManager(), new ProfileManager(), ProfileManager.DefaultProfileName))
    {
    }

    public SettingsWindow(SettingsViewModel vm)
    {
        InitializeComponent();
        _vm = vm;
        DataContext = vm;
    }

    private async void BrowseDefaultOutput_Click(object? sender, RoutedEventArgs e)
    {
        var folders = await StorageProvider.OpenFolderPickerAsync(new FolderPickerOpenOptions
        {
            Title = "Select default output folder",
            AllowMultiple = false,
        });

        if (folders.FirstOrDefault()?.Path is { IsAbsoluteUri: true } uri)
            _vm.DefaultOutputDirectory = uri.LocalPath;
    }

    private void ClearDefaultOutput_Click(object? sender, RoutedEventArgs e) =>
        _vm.DefaultOutputDirectory = string.Empty;
}
