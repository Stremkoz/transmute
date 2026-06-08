using System.CommandLine;
using System.Text.Json;
using System.Text.Json.Serialization;
using Transmute.Core.Config;

namespace Transmute.CLI.Commands;

public static class ProfileCommand
{
    public static Command Build(ConfigManager configManager, ProfileManager profileManager)
    {
        var cmd = new Command("profile", "Manage conversion profiles");

        cmd.AddCommand(BuildList(profileManager));
        cmd.AddCommand(BuildCreate(profileManager));
        cmd.AddCommand(BuildDuplicate(profileManager));
        cmd.AddCommand(BuildDelete(profileManager));
        cmd.AddCommand(BuildRename(profileManager));
        cmd.AddCommand(BuildShow(configManager, profileManager));
        cmd.AddCommand(BuildPath(profileManager));

        return cmd;
    }

    private static Command BuildList(ProfileManager pm)
    {
        var cmd = new Command("list", "List all profiles");
        cmd.SetHandler(() =>
        {
            Console.WriteLine($"  {ProfileManager.DefaultProfileName}  (global config)");
            var profiles = pm.List();
            foreach (var name in profiles)
                Console.WriteLine($"  {name}");

            if (profiles.Count == 0)
                Console.WriteLine("  (no named profiles — use 'profile create <name>' to add one)");
        });
        return cmd;
    }

    private static Command BuildCreate(ProfileManager pm)
    {
        var nameArg = new Argument<string>("name", "Name for the new profile");
        var cmd = new Command("create", "Create a new empty profile") { nameArg };
        cmd.SetHandler((ctx) =>
        {
            var name = ctx.ParseResult.GetValueForArgument(nameArg);
            try
            {
                pm.Create(name);
                Console.WriteLine($"Created profile '{name}'.");
                Console.WriteLine($"Edit it with: transmute config --profile {name} defaults.<key> <value>");
                Console.WriteLine($"Or find the file at: {Path.Combine(pm.FolderPath, name + ".json")}");
            }
            catch (Exception ex)
            {
                Console.Error.WriteLine($"Error: {ex.Message}");
                ctx.ExitCode = 1;
            }
        });
        return cmd;
    }

    private static Command BuildDuplicate(ProfileManager pm)
    {
        var sourceArg = new Argument<string>("source", "Profile to duplicate (or 'Default')");
        var destArg   = new Argument<string>("name",   "Name for the new profile");
        var cmd = new Command("duplicate", "Duplicate an existing profile") { sourceArg, destArg };
        cmd.AddAlias("dup");
        cmd.SetHandler((ctx) =>
        {
            var source = ctx.ParseResult.GetValueForArgument(sourceArg);
            var dest   = ctx.ParseResult.GetValueForArgument(destArg);
            try
            {
                pm.Duplicate(source, dest);
                Console.WriteLine($"Duplicated '{source}' → '{dest}'.");
            }
            catch (Exception ex)
            {
                Console.Error.WriteLine($"Error: {ex.Message}");
                ctx.ExitCode = 1;
            }
        });
        return cmd;
    }

    private static Command BuildDelete(ProfileManager pm)
    {
        var nameArg = new Argument<string>("name", "Profile to delete");
        var yesOpt  = new Option<bool>("--yes", "Skip confirmation prompt");
        yesOpt.AddAlias("-y");
        var cmd = new Command("delete", "Delete a profile") { nameArg, yesOpt };
        cmd.SetHandler((ctx) =>
        {
            var name = ctx.ParseResult.GetValueForArgument(nameArg);
            var yes  = ctx.ParseResult.GetValueForOption(yesOpt);

            if (!yes)
            {
                Console.Write($"Delete profile '{name}'? This cannot be undone. [y/N] ");
                var response = Console.ReadLine();
                if (!string.Equals(response?.Trim(), "y", StringComparison.OrdinalIgnoreCase))
                {
                    Console.WriteLine("Cancelled.");
                    return;
                }
            }

            try
            {
                pm.Delete(name);
                Console.WriteLine($"Deleted profile '{name}'.");
            }
            catch (Exception ex)
            {
                Console.Error.WriteLine($"Error: {ex.Message}");
                ctx.ExitCode = 1;
            }
        });
        return cmd;
    }

    private static Command BuildRename(ProfileManager pm)
    {
        var oldArg = new Argument<string>("old-name", "Current profile name");
        var newArg = new Argument<string>("new-name", "New profile name");
        var cmd = new Command("rename", "Rename a profile") { oldArg, newArg };
        cmd.SetHandler((ctx) =>
        {
            var oldName = ctx.ParseResult.GetValueForArgument(oldArg);
            var newName = ctx.ParseResult.GetValueForArgument(newArg);
            try
            {
                pm.Rename(oldName, newName);
                Console.WriteLine($"Renamed '{oldName}' → '{newName}'.");
            }
            catch (Exception ex)
            {
                Console.Error.WriteLine($"Error: {ex.Message}");
                ctx.ExitCode = 1;
            }
        });
        return cmd;
    }

    private static Command BuildShow(ConfigManager cm, ProfileManager pm)
    {
        var nameArg = new Argument<string>("name", "Profile to show (omit or 'Default' for global defaults)")
        {
            Arity = ArgumentArity.ZeroOrOne
        };
        var effectiveOpt = new Option<bool>("--effective",
            "Show the fully merged effective values (profile overrides applied on top of global defaults)");

        var cmd = new Command("show", "Show a profile's settings") { nameArg, effectiveOpt };
        cmd.SetHandler((ctx) =>
        {
            var name      = ctx.ParseResult.GetValueForArgument(nameArg);
            var effective = ctx.ParseResult.GetValueForOption(effectiveOpt);

            var opts = new JsonSerializerOptions
            {
                WriteIndented = true,
                DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull,
                PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
            };

            if (string.IsNullOrEmpty(name) ||
                string.Equals(name, ProfileManager.DefaultProfileName, StringComparison.OrdinalIgnoreCase))
            {
                Console.WriteLine("// Default profile — global defaults");
                Console.WriteLine(JsonSerializer.Serialize(cm.Config.Defaults, opts));
                return;
            }

            var profile = pm.Load(name);
            if (profile is null)
            {
                Console.Error.WriteLine($"Profile '{name}' not found.");
                ctx.ExitCode = 1;
                return;
            }

            if (effective)
            {
                var merged = profile.ApplyOver(cm.Config.Defaults);
                Console.WriteLine($"// Profile '{name}' — effective values (merged with global defaults)");
                Console.WriteLine(JsonSerializer.Serialize(merged, opts));
            }
            else
            {
                Console.WriteLine($"// Profile '{name}' — stored overrides (null fields inherit from global)");
                Console.WriteLine(JsonSerializer.Serialize(profile, opts));
            }

            if (profile.HasOnlyFilter)
                Console.WriteLine($"\n// Input filter: ONLY {string.Join(", ", profile.OnlyFormats)}");
            else if (profile.HasSkipFilter)
                Console.WriteLine($"\n// Input filter: SKIP {string.Join(", ", profile.SkipFormats)}");
        });
        return cmd;
    }

    private static Command BuildPath(ProfileManager pm)
    {
        var cmd = new Command("path", "Print the profiles folder path");
        cmd.SetHandler(() => Console.WriteLine(pm.FolderPath));
        return cmd;
    }
}
