using System;
using System.Linq;
using System.Windows;
using Microsoft.Win32;
using Transmute.Core.Models;

namespace Transmute.GUI;

public static class ThemeManager
{
    private static AppTheme _active = AppTheme.System;

    public static void Apply(AppTheme theme, ResourceDictionary appResources)
    {
        _active = theme;
        var isDark = theme switch
        {
            AppTheme.Dark  => true,
            AppTheme.Light => false,
            _              => IsSystemDark()
        };
        SwapThemeDict(appResources, isDark ? "Dark" : "Light");
    }

    public static void ReapplyIfSystem(ResourceDictionary appResources)
    {
        if (_active == AppTheme.System)
            Apply(AppTheme.System, appResources);
    }

    public static bool IsSystemDark()
    {
        try
        {
            var val = Registry.GetValue(
                @"HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
                "AppsUseLightTheme", 1);
            return val is int i && i == 0;
        }
        catch { return false; }
    }

    private static void SwapThemeDict(ResourceDictionary resources, string name)
    {
        var old = resources.MergedDictionaries
            .FirstOrDefault(d => d.Source is { } s &&
                (s.OriginalString.EndsWith("Light.xaml") || s.OriginalString.EndsWith("Dark.xaml")));

        var newDict = new ResourceDictionary
        {
            Source = new Uri($"pack://application:,,,/Themes/{name}.xaml")
        };

        if (old != null)
        {
            var idx = resources.MergedDictionaries.IndexOf(old);
            resources.MergedDictionaries[idx] = newDict;
        }
        else
        {
            resources.MergedDictionaries.Insert(0, newDict);
        }
    }
}
