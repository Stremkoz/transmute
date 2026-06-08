using System.Windows;
using System.Windows.Threading;
using Microsoft.Win32;
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

        // Apply saved theme before any window is shown
        ThemeManager.Apply(ConfigManager.Config.UI.Theme, Resources);

        // Watch for Windows system theme changes
        SystemEvents.UserPreferenceChanged += (_, args) =>
        {
            if (args.Category == UserPreferenceCategory.General)
                Dispatcher.Invoke(() => ThemeManager.ReapplyIfSystem(Resources));
        };

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
        e.Handled = true;
    }
}
