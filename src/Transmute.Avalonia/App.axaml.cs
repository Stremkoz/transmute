using Avalonia;
using Avalonia.Controls.ApplicationLifetimes;
using Avalonia.Markup.Xaml;
using Avalonia.Styling;
using Transmute.Core.Config;
using Transmute.Core.Models;
using Transmute.Avalonia.Views;

namespace Transmute.Avalonia;

public partial class App : Application
{
    public ConfigManager ConfigManager { get; } = new();
    public ProfileManager ProfileManager { get; } = new();

    public static new App Current => (App)Application.Current!;

    public override void Initialize()
    {
        AvaloniaXamlLoader.Load(this);
    }

    public override void OnFrameworkInitializationCompleted()
    {
        ProfileManager.EnsureFolder();
        ApplyTheme(ConfigManager.Config.UI.Theme);

        if (ApplicationLifetime is IClassicDesktopStyleApplicationLifetime desktop)
        {
            var vm = new ViewModels.MainViewModel(ConfigManager, ProfileManager);
            desktop.MainWindow = new MainWindow(vm);
        }

        base.OnFrameworkInitializationCompleted();
    }

    public void ApplyTheme(AppTheme theme)
    {
        RequestedThemeVariant = theme switch
        {
            AppTheme.Light => ThemeVariant.Light,
            AppTheme.Dark  => ThemeVariant.Dark,
            _              => ThemeVariant.Default, // follows the OS, updates live
        };
    }
}
