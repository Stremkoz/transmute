using System.Windows;
using Transmute.Core.Config;
using Transmute.GUI.ViewModels;

namespace Transmute.GUI.Views;

public partial class ProfileManagerWindow : Window
{
    private readonly ProfileManagerViewModel _vm;
    private readonly ProfileManager _profileManager;

    public string? SelectedProfileOnClose { get; private set; }

    public ProfileManagerWindow(ProfileManagerViewModel vm, ProfileManager profileManager)
    {
        InitializeComponent();
        _vm = vm;
        _profileManager = profileManager;
        DataContext = vm;

        vm.PropertyChanged += (_, e) =>
        {
            if (e.PropertyName == nameof(vm.SelectedProfile))
                UpdateOnlyFilterWarning();
        };

        UpdateOnlyFilterWarning();
    }

    private void UpdateOnlyFilterWarning()
    {
        var name = _vm.SelectedProfile;
        if (string.IsNullOrEmpty(name) ||
            string.Equals(name, ProfileManager.DefaultProfileName, StringComparison.OrdinalIgnoreCase))
        {
            OnlyFilterWarning.Visibility = Visibility.Collapsed;
            return;
        }

        var profile = _profileManager.Load(name);
        if (profile?.HasOnlyFilter == true)
        {
            var formats = string.Join(", ", profile.OnlyFormats);
            OnlyFilterText.Text = $"This profile will ignore all formats not listed: {formats}";
            OnlyFilterWarning.Visibility = Visibility.Visible;
        }
        else
        {
            OnlyFilterWarning.Visibility = Visibility.Collapsed;
        }
    }

    private void OpenFolder_Click(object sender, RoutedEventArgs e)
    {
        _profileManager.EnsureFolder();
        try { System.Diagnostics.Process.Start("explorer.exe", _profileManager.FolderPath); }
        catch { /* silently ignore */ }
    }

    private void Close_Click(object sender, RoutedEventArgs e)
    {
        SelectedProfileOnClose = _vm.SelectedProfile;
        DialogResult = _vm.ProfilesChanged;
        Close();
    }
}
