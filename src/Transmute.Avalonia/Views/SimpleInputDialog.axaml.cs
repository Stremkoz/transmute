using Avalonia.Controls;
using Avalonia.Interactivity;

namespace Transmute.Avalonia.Views;

/// <summary>Text-input prompt. ShowDialog&lt;string?&gt; returns the trimmed value, or null on cancel.</summary>
public partial class SimpleInputDialog : Window
{
    /// <summary>Design-time constructor; not used at runtime.</summary>
    public SimpleInputDialog() : this("Title", "Label", "")
    {
    }

    public SimpleInputDialog(string title, string label, string defaultValue)
    {
        InitializeComponent();
        Title = title;
        LabelText.Text = label;
        InputBox.Text = defaultValue;
        Opened += (_, _) =>
        {
            InputBox.Focus();
            InputBox.SelectAll();
        };
    }

    private void OK_Click(object? sender, RoutedEventArgs e)
    {
        if (string.IsNullOrWhiteSpace(InputBox.Text)) return;
        Close(InputBox.Text.Trim());
    }

    private void Cancel_Click(object? sender, RoutedEventArgs e) => Close(null);
}
