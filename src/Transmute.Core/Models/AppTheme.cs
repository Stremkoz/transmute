using System.Text.Json.Serialization;

namespace Transmute.Core.Models;

[JsonConverter(typeof(JsonStringEnumConverter))]
public enum AppTheme { System = 0, Light = 1, Dark = 2 }
