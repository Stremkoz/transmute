using System.Text.Json.Serialization;

namespace Transmute.Core.Models;

[JsonConverter(typeof(JsonStringEnumConverter))]
public enum MetadataMode
{
    PreserveAll  = 0,  // Archival — no data loss (default)
    StripAll     = 1,  // Privacy-focused / web optimisation
    ColorProfile = 2,  // ICC profile only — avoids colour shifts without leaking EXIF
    Copyright    = 3,  // EXIF + XMP + IPTC copyright/author fields only
}
