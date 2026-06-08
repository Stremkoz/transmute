using System.CommandLine;
using Transmute.Core;
using Transmute.Core.Config;
using Transmute.Core.Routing;

namespace Transmute.CLI.Commands;

public static class InfoCommand
{
    public static Command Build(ConfigManager configManager)
    {
        var fileArg = new Argument<FileInfo>("file", "File to inspect");
        fileArg.ExistingOnly();

        var cmd = new Command("info", "Show format info and which backend would handle a file") { fileArg };

        cmd.SetHandler((FileInfo file) =>
        {
            var config = configManager.Config;
            var (_, router, temp, _) = TransmuteFactory.Create(config);
            using (temp)
            {
                var ext = Path.GetExtension(file.Name).TrimStart('.').ToLowerInvariant();
                var affinity = FormatRegistry.GetAffinity(ext);
                var isVideo = FormatRegistry.IsVideoContainer(ext);
                var isAnim = FormatRegistry.IsAnimated(ext);

                Console.WriteLine($"File:      {file.FullName}");
                Console.WriteLine($"Size:      {file.Length:N0} bytes");
                Console.WriteLine($"Extension: .{ext}");
                Console.WriteLine($"Backend affinity: {affinity}");

                if (isVideo) Console.WriteLine("  ⚠ Treated as video container (routed to ffmpeg)");
                if (isAnim) Console.WriteLine("  ⚠ Treated as animated format (routed to ffmpeg)");
            }
        }, fileArg);

        return cmd;
    }
}
