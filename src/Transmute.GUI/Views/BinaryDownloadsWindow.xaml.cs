using System.Diagnostics;
using System.Windows;
using System.Windows.Controls;

namespace Transmute.GUI.Views;

public partial class BinaryDownloadsWindow : Window
{
    public BinaryDownloadsWindow()
    {
        InitializeComponent();
    }

    private void OpenUrl_Click(object sender, RoutedEventArgs e)
    {
        if (sender is Button btn && btn.Tag is string url)
            Process.Start(new ProcessStartInfo { FileName = url, UseShellExecute = true });
    }

    private void Close_Click(object sender, RoutedEventArgs e) => Close();
}
