using System.Collections.ObjectModel;
using System.Windows;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Transmute.Core.Config;

namespace Transmute.GUI.ViewModels;

public partial class ProfileManagerViewModel : ObservableObject
{
    private readonly ProfileManager _profileManager;

    public ObservableCollection<string> Profiles { get; } = new();

    [ObservableProperty] private string? _selectedProfile;

    public bool IsDefaultSelected =>
        string.IsNullOrEmpty(SelectedProfile) ||
        string.Equals(SelectedProfile, ProfileManager.DefaultProfileName, StringComparison.OrdinalIgnoreCase);

    public bool SelectedProfileHasOnlyFilter
    {
        get
        {
            if (IsDefaultSelected) return false;
            return _profileManager.Load(SelectedProfile!)?.HasOnlyFilter == true;
        }
    }

    // True if a profile was created or deleted — callers can reload the profile list
    public bool ProfilesChanged { get; private set; }

    public ProfileManagerViewModel(ProfileManager profileManager)
    {
        _profileManager = profileManager;
        RefreshList();
    }

    private void RefreshList()
    {
        Profiles.Clear();
        Profiles.Add(ProfileManager.DefaultProfileName);
        foreach (var name in _profileManager.List())
            Profiles.Add(name);
    }

    partial void OnSelectedProfileChanged(string? value)
    {
        OnPropertyChanged(nameof(IsDefaultSelected));
        OnPropertyChanged(nameof(SelectedProfileHasOnlyFilter));
    }

    [RelayCommand]
    private void CreateProfile()
    {
        var name = PromptName("New Profile", "Enter a name for the new profile:", string.Empty);
        if (string.IsNullOrWhiteSpace(name)) return;

        try
        {
            _profileManager.Create(name);
            ProfilesChanged = true;
            RefreshList();
            SelectedProfile = name;
        }
        catch (Exception ex)
        {
            MessageBox.Show(ex.Message, "Create Profile", MessageBoxButton.OK, MessageBoxImage.Warning);
        }
    }

    [RelayCommand(CanExecute = nameof(CanActOnSelected))]
    private void DuplicateProfile()
    {
        var source = SelectedProfile!;
        var defaultName = source.Equals(ProfileManager.DefaultProfileName, StringComparison.OrdinalIgnoreCase)
            ? "Copy of Default"
            : $"Copy of {source}";

        var name = PromptName("Duplicate Profile", $"Enter a name for the copy of '{source}':", defaultName);
        if (string.IsNullOrWhiteSpace(name)) return;

        try
        {
            _profileManager.Duplicate(source, name);
            ProfilesChanged = true;
            RefreshList();
            SelectedProfile = name;
        }
        catch (Exception ex)
        {
            MessageBox.Show(ex.Message, "Duplicate Profile", MessageBoxButton.OK, MessageBoxImage.Warning);
        }
    }

    [RelayCommand(CanExecute = nameof(CanActOnNamed))]
    private void RenameProfile()
    {
        var oldName = SelectedProfile!;
        var newName = PromptName("Rename Profile", $"Enter a new name for '{oldName}':", oldName);
        if (string.IsNullOrWhiteSpace(newName) || newName == oldName) return;

        try
        {
            _profileManager.Rename(oldName, newName);
            ProfilesChanged = true;
            RefreshList();
            SelectedProfile = newName;
        }
        catch (Exception ex)
        {
            MessageBox.Show(ex.Message, "Rename Profile", MessageBoxButton.OK, MessageBoxImage.Warning);
        }
    }

    [RelayCommand(CanExecute = nameof(CanActOnNamed))]
    private void DeleteProfile()
    {
        var name = SelectedProfile!;

        // Check if the profile has OnlyFormats set — warn if so
        var profile = _profileManager.Load(name);
        var extra = profile?.HasOnlyFilter == true
            ? $"\n\nNote: This profile has an OnlyFormats filter set ({string.Join(", ", profile.OnlyFormats!)})."
            : string.Empty;

        var result = MessageBox.Show(
            $"Delete profile '{name}'? This cannot be undone.{extra}",
            "Delete Profile",
            MessageBoxButton.YesNo,
            MessageBoxImage.Warning,
            MessageBoxResult.No);

        if (result != MessageBoxResult.Yes) return;

        try
        {
            _profileManager.Delete(name);
            ProfilesChanged = true;
            RefreshList();
            SelectedProfile = ProfileManager.DefaultProfileName;
        }
        catch (Exception ex)
        {
            MessageBox.Show(ex.Message, "Delete Profile", MessageBoxButton.OK, MessageBoxImage.Error);
        }
    }

    private bool CanActOnSelected() => SelectedProfile is not null;
    private bool CanActOnNamed() =>
        SelectedProfile is not null && !IsDefaultSelected;

    partial void OnSelectedProfileChanging(string? value)
    {
        DuplicateProfileCommand.NotifyCanExecuteChanged();
        RenameProfileCommand.NotifyCanExecuteChanged();
        DeleteProfileCommand.NotifyCanExecuteChanged();
    }

    private static string? PromptName(string title, string label, string defaultValue)
    {
        var dialog = new Views.SimpleInputDialog(title, label, defaultValue);
        return dialog.ShowDialog() == true ? dialog.InputValue : null;
    }
}
