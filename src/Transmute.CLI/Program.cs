using System.CommandLine;
using Transmute.CLI.Commands;
using Transmute.Core.Config;

var configPath = args.SkipWhile(a => a != "--config").Skip(1).FirstOrDefault();
var configManager = new ConfigManager(configPath);

var profileManager = new ProfileManager();
profileManager.EnsureFolder();

var root = new RootCommand("Transmute — smart image format converter")
{
    ConvertCommand.Build(configManager, profileManager),
    WatchCommand.Build(configManager, profileManager),
    InfoCommand.Build(configManager),
    BackendsCommand.Build(configManager),
    ConfigCommand.Build(configManager, profileManager),
    ProfileCommand.Build(configManager, profileManager),
};

root.AddGlobalOption(new Option<string?>("--config", "Path to config file"));

return await root.InvokeAsync(args);
