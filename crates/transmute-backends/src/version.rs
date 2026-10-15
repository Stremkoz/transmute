//! Tool version detection.

use std::fmt;

use transmute_core::tool::ToolId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Version {
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    /// Parses the numeric prefix of a version token: `v0.11.2`, `8.18.1`,
    /// `n7.1`, `7.1.1-46`. Needs at least `major.minor`; anything that isn't
    /// (such as a dated git build `2026-05-11-git-…`) yields `None`.
    pub fn parse_lenient(token: &str) -> Option<Self> {
        let token = token.trim_start_matches(['v', 'n']);
        let numeric: &str = token
            .split(|c: char| !c.is_ascii_digit() && c != '.')
            .next()?;
        let mut parts = numeric.split('.').map(str::parse::<u32>);
        let major = parts.next()?.ok()?;
        let minor = parts.next()?.ok()?;
        let patch = match parts.next() {
            Some(part) => part.ok()?,
            None => 0,
        };
        Some(Self::new(major, minor, patch))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// What a tool reported about its version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolVersion {
    /// The version text as the tool printed it, e.g. `v0.11.2` or a git build id.
    pub raw: String,
    /// Parsed numeric version, when the tool uses one.
    pub parsed: Option<Version>,
}

impl fmt::Display for ToolVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.raw)
    }
}

/// Arguments that make `tool` print its version.
pub fn version_args(tool: ToolId) -> &'static [&'static str] {
    match tool {
        ToolId::Cjxl | ToolId::Djxl | ToolId::Vips | ToolId::Vipsheader => &["--version"],
        ToolId::Cwebp
        | ToolId::Dwebp
        | ToolId::Gif2webp
        | ToolId::Magick
        | ToolId::Ffmpeg
        | ToolId::Ffprobe => &["-version"],
    }
}

/// The oldest version known to support everything the adapters use. Older
/// versions are still used, but flagged as outdated.
pub fn minimum_version(tool: ToolId) -> Option<Version> {
    match tool {
        // Effort 10 and the current lossless-JPEG flags.
        ToolId::Cjxl | ToolId::Djxl => Some(Version::new(0, 10, 0)),
        ToolId::Cwebp | ToolId::Dwebp | ToolId::Gif2webp => Some(Version::new(1, 0, 0)),
        // The `keep` save option for selective metadata.
        ToolId::Vips | ToolId::Vipsheader => Some(Version::new(8, 15, 0)),
        ToolId::Magick => Some(Version::new(7, 0, 0)),
        // Commonly installed as dated git builds without a version number.
        ToolId::Ffmpeg | ToolId::Ffprobe => None,
    }
}

/// Extracts the version from a tool's version output, or `None` if the output
/// doesn't look like this tool at all.
pub fn parse_version(tool: ToolId, output: &str) -> Option<ToolVersion> {
    let first_line = output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?;
    let words: Vec<&str> = first_line.split_whitespace().collect();

    let raw = match tool {
        // "cjxl v0.11.2 332feb1 [AVX2,SSE2]"
        ToolId::Cjxl | ToolId::Djxl => word_after(&words, tool.program_name())?,
        // "1.6.0"
        ToolId::Cwebp | ToolId::Dwebp => words.first().copied()?,
        // "WebP Encoder version: 1.6.0"
        ToolId::Gif2webp => word_after(&words, "version:")?,
        // "vips-8.18.1"
        ToolId::Vips | ToolId::Vipsheader => first_line.strip_prefix("vips-")?,
        // "Version: ImageMagick 7.1.1-46 Q16-HDRI x64 …"
        ToolId::Magick => word_after(&words, "ImageMagick")?,
        // "ffmpeg version 7.1 Copyright …" or "ffmpeg version 2026-05-11-git-… Copyright …"
        ToolId::Ffmpeg | ToolId::Ffprobe => {
            (words.first() == Some(&tool.program_name())).then_some(())?;
            word_after(&words, "version")?
        }
    };

    let parsed = Version::parse_lenient(raw);
    // Tools whose version is always numeric must parse, or this isn't them.
    if parsed.is_none() && minimum_version(tool).is_some() {
        return None;
    }
    Some(ToolVersion {
        raw: raw.to_owned(),
        parsed,
    })
}

fn word_after<'a>(words: &[&'a str], marker: &str) -> Option<&'a str> {
    let index = words.iter().position(|word| *word == marker)?;
    words.get(index + 1).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Version output captured from real installations (Windows, 2026-10).
    const SAMPLES: &[(ToolId, &str, &str, Option<Version>)] = &[
        (
            ToolId::Cjxl,
            "cjxl v0.11.2 332feb1 [AVX2,SSE2]\nCopyright (c) the JPEG XL Project",
            "v0.11.2",
            Some(Version::new(0, 11, 2)),
        ),
        (
            ToolId::Djxl,
            "djxl v0.11.2 332feb1 [AVX2,SSE2]",
            "v0.11.2",
            Some(Version::new(0, 11, 2)),
        ),
        (
            ToolId::Cwebp,
            "1.6.0\nlibsharpyuv: 0.4.2",
            "1.6.0",
            Some(Version::new(1, 6, 0)),
        ),
        (ToolId::Dwebp, "1.6.0", "1.6.0", Some(Version::new(1, 6, 0))),
        (
            ToolId::Gif2webp,
            "WebP Encoder version: 1.6.0\nWebP Mux version: 1.6.0",
            "1.6.0",
            Some(Version::new(1, 6, 0)),
        ),
        (
            ToolId::Vips,
            "vips-8.18.1",
            "8.18.1",
            Some(Version::new(8, 18, 1)),
        ),
        (
            ToolId::Vipsheader,
            "vips-8.18.1",
            "8.18.1",
            Some(Version::new(8, 18, 1)),
        ),
        (
            ToolId::Magick,
            "Version: ImageMagick 7.1.1-46 Q16-HDRI x64 8209e84:20250318 https://imagemagick.org",
            "7.1.1-46",
            Some(Version::new(7, 1, 1)),
        ),
        (
            ToolId::Ffmpeg,
            "ffmpeg version 2026-05-11-git-17bc88e67f-essentials_build-www.gyan.dev Copyright (c) 2000-2026",
            "2026-05-11-git-17bc88e67f-essentials_build-www.gyan.dev",
            None,
        ),
        (
            ToolId::Ffprobe,
            "ffprobe version n7.1 Copyright (c) 2007-2024",
            "n7.1",
            Some(Version::new(7, 1, 0)),
        ),
    ];

    #[test]
    fn parses_real_version_output() {
        for (tool, output, raw, parsed) in SAMPLES {
            let version =
                parse_version(*tool, output).unwrap_or_else(|| panic!("{tool:?} not parsed"));
            assert_eq!(version.raw, *raw, "{tool:?}");
            assert_eq!(version.parsed, *parsed, "{tool:?}");
        }
    }

    #[test]
    fn unrelated_output_is_rejected() {
        assert_eq!(parse_version(ToolId::Cjxl, "Usage: something else"), None);
        assert_eq!(parse_version(ToolId::Cwebp, "not a version"), None);
        assert_eq!(parse_version(ToolId::Vips, ""), None);
        assert_eq!(parse_version(ToolId::Ffmpeg, "ffprobe version 7.1"), None);
    }

    #[test]
    fn lenient_parsing() {
        assert_eq!(
            Version::parse_lenient("v0.11"),
            Some(Version::new(0, 11, 0))
        );
        assert_eq!(Version::parse_lenient("2026-05-11-git"), None);
        assert_eq!(Version::parse_lenient("abc"), None);
        assert!(Version::new(0, 10, 0) < Version::new(0, 11, 2));
    }

    #[test]
    fn every_tool_has_version_args() {
        for tool in ToolId::ALL {
            assert!(!version_args(tool).is_empty());
        }
    }
}
