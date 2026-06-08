using System.CommandLine;
using Transmute.Core.Config;

namespace Transmute.CLI.Commands;

public static class ConfigCommand
{
    public static Command Build(ConfigManager configManager)
    {
        var cmd = new Command("config", "View or modify Transmute configuration");

        cmd.AddCommand(BuildGet(configManager));
        cmd.AddCommand(BuildSet(configManager));
        cmd.AddCommand(BuildReset(configManager));
        cmd.AddCommand(BuildPath(configManager));

        return cmd;
    }

    private static Command BuildGet(ConfigManager configManager)
    {
        var keyArg = new Argument<string?>("key", () => null,
            "Config key in section.property format (e.g. binaries.cwebp). Omit for full config.");

        var cmd = new Command("get", "Print current config value(s)") { keyArg };

        cmd.SetHandler((string? key) =>
        {
            try
            {
                var value = configManager.Get(key);
                Console.WriteLine(value ?? "(not set)");
            }
            catch (ArgumentException ex)
            {
                Console.Error.WriteLine($"Error: {ex.Message}");
            }
        }, keyArg);

        return cmd;
    }

    private static Command BuildSet(ConfigManager configManager)
    {
        var keyArg = new Argument<string>("key", "Config key in section.property format");
        var valueArg = new Argument<string>("value", "Value to set (use 'null' to clear)");

        var cmd = new Command("set", "Set a config value") { keyArg, valueArg };

        cmd.SetHandler((string key, string value) =>
        {
            try
            {
                configManager.Set(key, value);
                Console.WriteLine($"Set {key} = {value}");
                Console.WriteLine($"Config saved to: {ConfigManager.DefaultConfigPath}");
            }
            catch (Exception ex)
            {
                Console.Error.WriteLine($"Error: {ex.Message}");
            }
        }, keyArg, valueArg);

        return cmd;
    }

    private static Command BuildReset(ConfigManager configManager)
    {
        var cmd = new Command("reset", "Reset all config to defaults");

        cmd.SetHandler(() =>
        {
            configManager.Reset();
            Console.WriteLine("Config reset to defaults.");
        });

        return cmd;
    }

    private static Command BuildPath(ConfigManager configManager)
    {
        var cmd = new Command("path", "Show the config file location");

        cmd.SetHandler(() =>
        {
            Console.WriteLine(ConfigManager.DefaultConfigPath);
        });

        return cmd;
    }
}
