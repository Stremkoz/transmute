using System.Windows;
using Transmute.Core.Config;
using Transmute.GUI.ViewModels;

namespace Transmute.GUI;

public partial class App : Application
{
    public ConfigManager ConfigManager { get; } = new();

    protected override void OnStartup(StartupEventArgs e)
    {
        base.OnStartup(e);
        var vm = new MainViewModel(ConfigManager);
        var win = new MainWindow(vm);
        win.Show();
    }
}
