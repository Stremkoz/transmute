using Avalonia.Controls;
using Avalonia.Interactivity;
using Transmute.Core.Config;
using Transmute.Avalonia.ViewModels;

namespace Transmute.Avalonia.Views;

public partial class ProfileManagerWindow : Window
{
    private readonly ProfileManagerViewModel _vm;

    public string? SelectedProfileOnClose { get; private set; }

    /// <summary>Design-time constructor; not used at runtime.</summary>
    public ProfileManagerWindow() : this(new ProfileManagerViewModel(new ProfileManager()))
    {
    }

    public ProfileManagerWindow(ProfileManagerViewModel vm)
    {
        InitializeComponent();
        _vm = vm;
        DataContext = vm;
    }

    private void Close_Click(object? sender, RoutedEventArgs e)
    {
        SelectedProfileOnClose = _vm.SelectedProfile;
        Close(_vm.ProfilesChanged);
    }
}
