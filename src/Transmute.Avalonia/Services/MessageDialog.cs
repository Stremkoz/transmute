using Avalonia.Controls;
using Avalonia.Controls.ApplicationLifetimes;
using Avalonia.Layout;
using Avalonia.Media;

namespace Transmute.Avalonia.Services;

public enum MessageIcon { Info, Warning, Error, Question }

/// <summary>
/// Async replacement for WPF's MessageBox. Avalonia has no built-in message box,
/// so this builds a small owned dialog window on the fly.
/// </summary>
public static class MessageDialog
{
    public static Task ShowAsync(string message, string title, MessageIcon icon = MessageIcon.Info) =>
        ShowCoreAsync(message, title, icon, ["OK"], defaultIndex: 0, cancelIndex: 0);

    /// <summary>Yes/No confirmation. Returns true only when Yes is clicked.</summary>
    public static async Task<bool> ConfirmAsync(string message, string title, MessageIcon icon = MessageIcon.Question)
    {
        var result = await ShowCoreAsync(message, title, icon, ["Yes", "No"], defaultIndex: 1, cancelIndex: 1);
        return result == 0;
    }

    private static async Task<int> ShowCoreAsync(
        string message, string title, MessageIcon icon, string[] buttons, int defaultIndex, int cancelIndex)
    {
        var owner = GetActiveWindow();

        var dialog = new Window
        {
            Title = title,
            SizeToContent = SizeToContent.WidthAndHeight,
            MaxWidth = 480,
            MinWidth = 320,
            CanResize = false,
            WindowStartupLocation = WindowStartupLocation.CenterOwner,
            ShowInTaskbar = false,
            Background = GetBrush("AppBackground"),
        };

        var glyph = icon switch
        {
            MessageIcon.Warning  => "⚠",
            MessageIcon.Error    => "✕",
            MessageIcon.Question => "?",
            _                    => "ℹ",
        };
        var glyphBrush = icon switch
        {
            MessageIcon.Warning => Brushes.Orange,
            MessageIcon.Error   => Brushes.IndianRed,
            _                   => Brushes.CornflowerBlue,
        };

        var buttonPanel = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            HorizontalAlignment = HorizontalAlignment.Right,
            Spacing = 8,
        };

        int result = cancelIndex;
        for (var i = 0; i < buttons.Length; i++)
        {
            var index = i;
            var button = new Button
            {
                Content = buttons[i],
                MinWidth = 80,
                HorizontalContentAlignment = HorizontalAlignment.Center,
                IsDefault = i == defaultIndex,
                IsCancel = i == cancelIndex,
            };
            if (i == 0) button.Classes.Add("primary");
            else button.Classes.Add("secondary");
            button.Click += (_, _) => { result = index; dialog.Close(); };
            buttonPanel.Children.Add(button);
        }

        dialog.Content = new StackPanel
        {
            Margin = new global::Avalonia.Thickness(20, 16),
            Spacing = 16,
            Children =
            {
                new StackPanel
                {
                    Orientation = Orientation.Horizontal,
                    Spacing = 12,
                    Children =
                    {
                        new TextBlock
                        {
                            Text = glyph, FontSize = 22, Foreground = glyphBrush,
                            VerticalAlignment = VerticalAlignment.Top,
                        },
                        new TextBlock
                        {
                            Text = message, TextWrapping = TextWrapping.Wrap,
                            MaxWidth = 380, FontSize = 12.5,
                            Foreground = GetBrush("TextPrimaryBrush"),
                            VerticalAlignment = VerticalAlignment.Center,
                        },
                    },
                },
                buttonPanel,
            },
        };

        if (owner is not null)
            await dialog.ShowDialog(owner);
        else
        {
            // No owner available (startup error etc.) — show standalone and wait for close
            var tcs = new TaskCompletionSource();
            dialog.Closed += (_, _) => tcs.SetResult();
            dialog.Show();
            await tcs.Task;
        }

        return result;
    }

    public static Window? GetActiveWindow()
    {
        if (global::Avalonia.Application.Current?.ApplicationLifetime
            is not IClassicDesktopStyleApplicationLifetime desktop)
            return null;
        return desktop.Windows.FirstOrDefault(w => w.IsActive) ?? desktop.MainWindow;
    }

    private static IBrush? GetBrush(string key)
    {
        var app = global::Avalonia.Application.Current;
        if (app is not null && app.TryGetResource(key, app.ActualThemeVariant, out var value) && value is IBrush brush)
            return brush;
        return null;
    }
}
