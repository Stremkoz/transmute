using System.Windows;
using System.Windows.Threading;
using Transmute.Core.Config;
using Transmute.GUI.ViewModels;

namespace Transmute.GUI;

public partial class App : Application
{
    public ConfigManager ConfigManager { get; } = new();
    public ProfileManager ProfileManager { get; } = new();

    protected override void OnStartup(StartupEventArgs e)
    {
        base.OnStartup(e);
        ProfileManager.EnsureFolder();
        DispatcherUnhandledException += OnUnhandledException;
        var vm = new MainViewModel(ConfigManager, ProfileManager);
        var win = new MainWindow(vm);
        win.Show();
    }

    private static void OnUnhandledException(object sender, DispatcherUnhandledExceptionEventArgs e)
    {
        MessageBox.Show(
            $"Unhandled error:\n\n{e.Exception.GetType().Name}: {e.Exception.Message}\n\n{e.Exception.StackTrace}",
            "Transmute — Error",
            MessageBoxButton.OK,
            MessageBoxImage.Error);
        e.Handled = true; // keep the app alive so we can read the message
    }
}
