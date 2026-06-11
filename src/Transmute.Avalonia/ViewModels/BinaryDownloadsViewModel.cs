namespace Transmute.Avalonia.ViewModels;

/// <summary>Provides platform-specific install instructions for the optional binary backends.</summary>
public class BinaryDownloadsViewModel
{
    public string CwebpInstructions { get; }
    public string VipsInstructions { get; }
    public string CjxlInstructions { get; }
    public string FfmpegInstructions { get; }
    public string MagickInstructions { get; }
    public string PathHeader { get; }
    public string PathInstructionsPrimary { get; }
    public string PathInstructionsSecondary { get; }

    public BinaryDownloadsViewModel()
    {
        if (OperatingSystem.IsWindows())
        {
            CwebpInstructions = "Download the Windows zip. Extract cwebp.exe and dwebp.exe to a folder like C:\\Tools\\.";
            VipsInstructions = "Download the Windows binary (vips-dev-w64-all-*.zip). Unlike the other tools, vips.exe must stay beside its DLLs — extract the entire bin\\ folder to something like C:\\Tools\\vips\\, so the path to the exe becomes C:\\Tools\\vips\\vips.exe.";
            CjxlInstructions = "Download the Windows zip. Extract cjxl.exe and djxl.exe to a folder like C:\\Tools\\.";
            FfmpegInstructions = "Choose a Windows build (gyan.dev or BtbN are recommended). Extract ffmpeg.exe from the bin\\ folder to C:\\Tools\\.";
            MagickInstructions = "Use the Windows installer — it handles PATH automatically. The exe will be named magick.exe.";
            PathHeader = "Adding to PATH (for Auto-Detect)";
            PathInstructionsPrimary = "Win + S → search 'Environment Variables' → Edit the system environment variables → Environment Variables… → under System variables, select Path → Edit → New.";
            PathInstructionsSecondary = "Add C:\\Tools for single EXEs (cwebp, cjxl, ffmpeg, magick), or C:\\Tools\\vips for libvips. Click OK on all dialogs, then use Auto-Detect above.";
        }
        else if (OperatingSystem.IsMacOS())
        {
            CwebpInstructions = "Install via Homebrew: brew install webp. Or download the macOS binaries and place cwebp/dwebp in /usr/local/bin (Intel) or /opt/homebrew/bin (Apple Silicon).";
            VipsInstructions = "Install via Homebrew: brew install vips. The vips binary will be on PATH automatically.";
            CjxlInstructions = "Install via Homebrew: brew install jpeg-xl. Or download the static binaries and place cjxl/djxl in /usr/local/bin or /opt/homebrew/bin.";
            FfmpegInstructions = "Install via Homebrew: brew install ffmpeg.";
            MagickInstructions = "Install via Homebrew: brew install imagemagick. The binary will be named magick.";
            PathHeader = "Adding to PATH";
            PathInstructionsPrimary = "Homebrew installs are added to PATH automatically (/usr/local/bin on Intel, /opt/homebrew/bin on Apple Silicon).";
            PathInstructionsSecondary = "For manually downloaded binaries, place them in one of those folders and make them executable with chmod +x, then use Auto-Detect above.";
        }
        else
        {
            CwebpInstructions = "Install via your distro's package manager, e.g. sudo apt install webp (Debian/Ubuntu) or sudo dnf install libwebp-tools (Fedora). Or download the Linux binaries and place cwebp/dwebp in ~/.local/bin or /usr/local/bin.";
            VipsInstructions = "Install via your distro's package manager, e.g. sudo apt install libvips-tools (Debian/Ubuntu) or sudo dnf install vips-tools (Fedora). The vips binary will be on PATH automatically.";
            CjxlInstructions = "Install via your distro's package manager, e.g. sudo apt install libjxl-tools (Debian/Ubuntu). Or download the static binaries and place cjxl/djxl in ~/.local/bin or /usr/local/bin.";
            FfmpegInstructions = "Install via your distro's package manager, e.g. sudo apt install ffmpeg (Debian/Ubuntu) or sudo dnf install ffmpeg (Fedora, may need RPM Fusion).";
            MagickInstructions = "Install via your distro's package manager, e.g. sudo apt install imagemagick. The binary may be named magick or convert depending on version.";
            PathHeader = "Adding to PATH (~/.bashrc, ~/.zshrc, etc.)";
            PathInstructionsPrimary = "Most package-manager installs put binaries straight on PATH — just restart the app afterwards and use Auto-Detect above.";
            PathInstructionsSecondary = "For manually downloaded binaries, place them in ~/.local/bin (per-user, usually already on PATH) or /usr/local/bin (system-wide, requires sudo), then make them executable with chmod +x.";
        }
    }
}
