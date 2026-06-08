using System.CommandLine;
using Transmute.Core;
using Transmute.Core.Config;

namespace Transmute.CLI.Commands;

public static class BackendsCommand
{
    public static Command Build(ConfigManager configManager)
    {
        var cmd = new Command("backends", "List all backends and their availability");

        cmd.SetHandler(() =>
        {
            var config = configManager.Config;
            var (_, router, temp, backends) = TransmuteFactory.Create(config);
            using (temp)
            {
                Console.WriteLine($"{"Backend",-32} {"Status",-12} {"Details"}");
                Console.WriteLine(new string('-', 70));

                foreach (var b in backends)
                {
                    var status = b.IsAvailable ? "Available" : "Not found";
                    var color = b.IsAvailable ? ConsoleColor.Green : ConsoleColor.Red;

                    Console.Write($"  {b.Name,-30} ");
                    Console.ForegroundColor = color;
                    Console.Write($"{status,-12}");
                    Console.ResetColor();

                    var inFmts = string.Join(", ", b.SupportedInputFormats.OrderBy(x => x).Take(8));
                    if (b.SupportedInputFormats.Count > 8) inFmts += "...";
                    Console.WriteLine($" reads: {inFmts}");
                }
            }
        });

        return cmd;
    }
}
