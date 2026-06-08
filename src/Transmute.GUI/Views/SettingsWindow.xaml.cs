using System.Windows;
using Transmute.GUI.ViewModels;

namespace Transmute.GUI.Views;

public partial class SettingsWindow : Window
{
    public SettingsWindow(SettingsViewModel vm)
    {
        InitializeComponent();
        DataContext = vm;
    }
}
