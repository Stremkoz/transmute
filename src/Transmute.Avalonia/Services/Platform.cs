using System.Diagnostics;
using System.Runtime.InteropServices;

namespace Transmute.Avalonia.Services;

public static class Platform
{
    /// <summary>Opens a folder in the OS file manager. Silently ignores failures.</summary>
    public static void OpenFolder(string path)
    {
        try
        {
            if (OperatingSystem.IsWindows())
                Process.Start("explorer.exe", $"\"{path}\"");
            else if (OperatingSystem.IsMacOS())
                Process.Start("open", path);
            else
                Process.Start("xdg-open", path);
        }
        catch { /* folder may not exist yet, or no file manager available */ }
    }

    /// <summary>Opens a URL in the default browser. Silently ignores failures.</summary>
    public static void OpenUrl(string url)
    {
        try
        {
            Process.Start(new ProcessStartInfo(url) { UseShellExecute = true });
        }
        catch { }
    }

    [DllImport("user32.dll")]
    private static extern bool MessageBeep(uint uType);

    private const uint MB_ICONASTERISK = 0x40;     // success chime
    private const uint MB_ICONEXCLAMATION = 0x30;  // warning sound

    // Freedesktop sound theme directories, checked in order on Linux.
    private static readonly string[] LinuxSoundThemeDirs =
    [
        "/usr/share/sounds/freedesktop/stereo",
        "/usr/share/sounds/ubuntu/stereo",
    ];

    /// <summary>Completion sound — uses the OS's notification sounds. Silently does nothing if unavailable.</summary>
    public static void PlayCompletionSound(bool hadProblems)
    {
        try
        {
            if (OperatingSystem.IsWindows())
            {
                MessageBeep(hadProblems ? MB_ICONEXCLAMATION : MB_ICONASTERISK);
            }
            else if (OperatingSystem.IsMacOS())
            {
                var sound = hadProblems ? "/System/Library/Sounds/Basso.aiff" : "/System/Library/Sounds/Glass.aiff";
                Process.Start(new ProcessStartInfo("afplay") { ArgumentList = { sound } });
            }
            else
            {
                PlayLinuxSound(hadProblems);
            }
        }
        catch { }
    }

    private static void PlayLinuxSound(bool hadProblems)
    {
        var soundName = hadProblems ? "dialog-warning" : "complete";

        // Prefer paplay with a sound file from the freedesktop/ubuntu theme.
        foreach (var dir in LinuxSoundThemeDirs)
        {
            var path = Path.Combine(dir, soundName + ".oga");
            if (File.Exists(path))
            {
                Process.Start(new ProcessStartInfo("paplay") { ArgumentList = { path } });
                return;
            }
        }

        // Fall back to canberra, which resolves theme sounds by name.
        Process.Start(new ProcessStartInfo("canberra-gtk-play") { ArgumentList = { "-i", soundName } });
    }
}
