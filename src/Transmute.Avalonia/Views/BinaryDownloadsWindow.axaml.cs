using Avalonia.Controls;
using Avalonia.Interactivity;
using Transmute.Avalonia.Services;
using Transmute.Avalonia.ViewModels;

namespace Transmute.Avalonia.Views;

public partial class BinaryDownloadsWindow : Window
{
    public BinaryDownloadsWindow()
    {
        InitializeComponent();
        DataContext = new BinaryDownloadsViewModel();
    }

    private void OpenUrl_Click(object? sender, RoutedEventArgs e)
    {
        if (sender is Button { Tag: string url })
            Platform.OpenUrl(url);
    }

    private void Close_Click(object? sender, RoutedEventArgs e) => Close();
}
