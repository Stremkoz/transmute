using System.Windows;
using System.Windows.Input;

namespace Transmute.GUI.Views;

public partial class SimpleInputDialog : Window
{
    public string InputValue => InputBox.Text.Trim();

    public SimpleInputDialog(string title, string label, string defaultValue)
    {
        InitializeComponent();
        Title = title;
        LabelText.Text = label;
        InputBox.Text = defaultValue;
        Loaded += (_, _) =>
        {
            InputBox.Focus();
            InputBox.SelectAll();
        };
    }

    private void OK_Click(object sender, RoutedEventArgs e)
    {
        if (string.IsNullOrWhiteSpace(InputBox.Text)) return;
        DialogResult = true;
    }

    private void Cancel_Click(object sender, RoutedEventArgs e) =>
        DialogResult = false;

    private void InputBox_KeyDown(object sender, KeyEventArgs e)
    {
        if (e.Key == Key.Enter) OK_Click(sender, e);
    }
}
