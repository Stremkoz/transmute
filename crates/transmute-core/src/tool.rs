//! Identities of the external tools Transmute can drive.

use std::env::consts::EXE_SUFFIX;

use serde::{Deserialize, Serialize};

/// An external program Transmute knows how to discover and invoke.
///
/// Encoders and decoders are separate entries so that, for example, having
/// `cwebp` without `dwebp` still enables WebP encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
pub enum ToolId {
    Cjxl,
    Djxl,
    Cwebp,
    Dwebp,
    Gif2webp,
    Vips,
    Vipsheader,
    Magick,
    Ffmpeg,
    Ffprobe,
}

impl ToolId {
    pub const ALL: [ToolId; 10] = [
        ToolId::Cjxl,
        ToolId::Djxl,
        ToolId::Cwebp,
        ToolId::Dwebp,
        ToolId::Gif2webp,
        ToolId::Vips,
        ToolId::Vipsheader,
        ToolId::Magick,
        ToolId::Ffmpeg,
        ToolId::Ffprobe,
    ];

    /// The program's base name, as users see it and as it appears on `PATH`.
    pub fn program_name(self) -> &'static str {
        match self {
            ToolId::Cjxl => "cjxl",
            ToolId::Djxl => "djxl",
            ToolId::Cwebp => "cwebp",
            ToolId::Dwebp => "dwebp",
            ToolId::Gif2webp => "gif2webp",
            ToolId::Vips => "vips",
            ToolId::Vipsheader => "vipsheader",
            ToolId::Magick => "magick",
            ToolId::Ffmpeg => "ffmpeg",
            ToolId::Ffprobe => "ffprobe",
        }
    }

    /// The executable file name on the current platform (`cwebp.exe` on Windows).
    pub fn executable_file_name(self) -> String {
        format!("{}{EXE_SUFFIX}", self.program_name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn program_names_are_unique() {
        let names: HashSet<_> = ToolId::ALL.iter().map(|t| t.program_name()).collect();
        assert_eq!(names.len(), ToolId::ALL.len());
    }

    #[test]
    fn executable_file_name_matches_platform() {
        let expected = if cfg!(windows) { "cjxl.exe" } else { "cjxl" };
        assert_eq!(ToolId::Cjxl.executable_file_name(), expected);
    }
}
