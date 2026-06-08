using System.CommandLine;
using Transmute.Core.Config;

namespace Transmute.CLI.Commands;

public static class ConfigCommand
{
    public static Command Build(ConfigManager configManager, ProfileManager profileManager)
    {
        var cmd = new Command("config", "View or modify Transmute configuration");

        cmd.AddCommand(BuildGet(configManager, profileManager));
        cmd.AddCommand(BuildSet(configManager, profileManager));
        cmd.AddCommand(BuildReset(configManager));
        cmd.AddCommand(BuildPath(configManager));

        return cmd;
    }

    private static Command BuildGet(ConfigManager configManager, ProfileManager profileManager)
    {
        var keyArg = new Argument<string?>("key", () => null,
            "Config key in section.property format (e.g. defaults.webpQuality). Omit for full config.");

        var profileOpt = new Option<string?>("--profile",
            "Read from a named profile instead of global config. Use 'Default' for global defaults.");
        profileOpt.AddAlias("-p");

        var cmd = new Command("get", "Print current config value(s)") { keyArg, profileOpt };

        cmd.SetHandler((string? key, string? profile) =>
        {
            try
            {
                if (!string.IsNullOrEmpty(profile) &&
                    !string.Equals(profile, ProfileManager.DefaultProfileName, StringComparison.OrdinalIgnoreCase))
                {
                    var value = profileManager.GetField(profile, key, configManager.Config.Defaults);
                    Console.WriteLine(value ?? "(not set)");
                }
                else
                {
                    var value = configManager.Get(key);
                    Console.WriteLine(value ?? "(not set)");
                }
            }
            catch (ArgumentException ex)
            {
                Console.Error.WriteLine($"Error: {ex.Message}");
            }
        }, keyArg, profileOpt);

        return cmd;
    }

    private static Command BuildSet(ConfigManager configManager, ProfileManager profileManager)
    {
        var keyArg   = new Argument<string>("key",   "Config key in section.property format");
        var valueArg = new Argument<string>("value", "Value to set (use 'null' to clear/reset to inherited)");

        var profileOpt = new Option<string?>("--profile",
            "Write to a named profile instead of global config. Binaries and processing keys are global-only.");
        profileOpt.AddAlias("-p");

        var cmd = new Command("set", "Set a config value") { keyArg, valueArg, profileOpt };

        cmd.SetHandler((string key, string value, string? profile) =>
        {
            try
            {
                if (!string.IsNullOrEmpty(profile) &&
                    !string.Equals(profile, ProfileManager.DefaultProfileName, StringComparison.OrdinalIgnoreCase))
                {
                    profileManager.SetField(profile, key, value);
                    Console.WriteLine($"Set {key} = {value}  [profile: {profile}]");
                    Console.WriteLine($"Profile saved to: {Path.Combine(profileManager.FolderPath, profile + ".json")}");
                }
                else
                {
                    configManager.Set(key, value);
                    Console.WriteLine($"Set {key} = {value}");
                    Console.WriteLine($"Config saved to: {ConfigManager.ResolveConfigPath()}");
                }
            }
            catch (Exception ex)
            {
                Console.Error.WriteLine($"Error: {ex.Message}");
            }
        }, keyArg, valueArg, profileOpt);

        return cmd;
    }

    private static Command BuildReset(ConfigManager configManager)
    {
        var cmd = new Command("reset", "Reset all global config to defaults");

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
            Console.WriteLine(ConfigManager.ResolveConfigPath());
        });

        return cmd;
    }
}
