using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Transmute.Core.Config;
using Transmute.Avalonia.Services;
using Transmute.Avalonia.Views;

namespace Transmute.Avalonia.ViewModels;

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

    public string OnlyFilterWarningText
    {
        get
        {
            if (IsDefaultSelected) return string.Empty;
            var profile = _profileManager.Load(SelectedProfile!);
            return profile?.HasOnlyFilter == true
                ? $"This profile will ignore all formats not listed: {string.Join(", ", profile.OnlyFormats)}"
                : string.Empty;
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
        OnPropertyChanged(nameof(OnlyFilterWarningText));
        // Fire AFTER value is set so CanActOnNamed reads the correct SelectedProfile
        DuplicateProfileCommand.NotifyCanExecuteChanged();
        RenameProfileCommand.NotifyCanExecuteChanged();
        DeleteProfileCommand.NotifyCanExecuteChanged();
    }

    [RelayCommand]
    private async Task CreateProfileAsync()
    {
        var name = await PromptNameAsync("New Profile", "Enter a name for the new profile:", string.Empty);
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
            await MessageDialog.ShowAsync(ex.Message, "Create Profile", MessageIcon.Warning);
        }
    }

    [RelayCommand(CanExecute = nameof(CanActOnSelected))]
    private async Task DuplicateProfileAsync()
    {
        var source = SelectedProfile!;
        var defaultName = source.Equals(ProfileManager.DefaultProfileName, StringComparison.OrdinalIgnoreCase)
            ? "Copy of Default"
            : $"Copy of {source}";

        var name = await PromptNameAsync("Duplicate Profile", $"Enter a name for the copy of '{source}':", defaultName);
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
            await MessageDialog.ShowAsync(ex.Message, "Duplicate Profile", MessageIcon.Warning);
        }
    }

    [RelayCommand(CanExecute = nameof(CanActOnNamed))]
    private async Task RenameProfileAsync()
    {
        var oldName = SelectedProfile!;
        var newName = await PromptNameAsync("Rename Profile", $"Enter a new name for '{oldName}':", oldName);
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
            await MessageDialog.ShowAsync(ex.Message, "Rename Profile", MessageIcon.Warning);
        }
    }

    [RelayCommand(CanExecute = nameof(CanActOnNamed))]
    private async Task DeleteProfileAsync()
    {
        var name = SelectedProfile!;

        // Check if the profile has OnlyFormats set — warn if so
        var profile = _profileManager.Load(name);
        var extra = profile?.HasOnlyFilter == true
            ? $"\n\nNote: This profile has an OnlyFormats filter set ({string.Join(", ", profile.OnlyFormats)})."
            : string.Empty;

        var confirmed = await MessageDialog.ConfirmAsync(
            $"Delete profile '{name}'? This cannot be undone.{extra}",
            "Delete Profile",
            MessageIcon.Warning);

        if (!confirmed) return;

        try
        {
            _profileManager.Delete(name);
            ProfilesChanged = true;
            RefreshList();
            SelectedProfile = ProfileManager.DefaultProfileName;
        }
        catch (Exception ex)
        {
            await MessageDialog.ShowAsync(ex.Message, "Delete Profile", MessageIcon.Error);
        }
    }

    [RelayCommand]
    private void OpenProfilesFolder()
    {
        _profileManager.EnsureFolder();
        Platform.OpenFolder(_profileManager.FolderPath);
    }

    private bool CanActOnSelected() => SelectedProfile is not null;
    private bool CanActOnNamed() =>
        SelectedProfile is not null && !IsDefaultSelected;

    private static async Task<string?> PromptNameAsync(string title, string label, string defaultValue)
    {
        var owner = MessageDialog.GetActiveWindow();
        if (owner is null) return null;
        var dialog = new SimpleInputDialog(title, label, defaultValue);
        return await dialog.ShowDialog<string?>(owner);
    }
}
