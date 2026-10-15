//! What an installed tool can actually do.
//!
//! Capabilities are data, probed from the real installation (versions, format
//! lists), so routing and the UI can rely on them instead of hardcoded
//! assumptions. Both the planner and the UI read them.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::format::Format;
use crate::tool::ToolId;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub reads: BTreeSet<Format>,
    pub writes: BTreeSet<Format>,
    /// Formats whose animation (all frames) this tool reads. A subset of `reads`.
    pub animated_reads: BTreeSet<Format>,
    /// Formats this tool can write as animations. A subset of `writes`.
    pub animated_writes: BTreeSet<Format>,
    /// How the tool handles metadata on the files it writes. `None` means the
    /// tool isn't characterised for this, e.g. a decoder whose output only
    /// feeds another step, with metadata handled from the original source.
    pub metadata: Option<MetadataSupport>,
    /// Caveats worth showing a user, e.g. build-dependent support.
    pub notes: Vec<String>,
}

impl Capabilities {
    /// True if the tool converts `from` to `to` in one step.
    pub fn converts(&self, from: Format, to: Format) -> bool {
        self.reads.contains(&from) && self.writes.contains(&to)
    }
}

/// The tools available for a job, with what each can do. Tools that aren't
/// installed (or aren't usable) are simply absent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Toolbox {
    tools: BTreeMap<ToolId, Capabilities>,
}

impl Toolbox {
    pub fn new(tools: impl IntoIterator<Item = (ToolId, Capabilities)>) -> Self {
        Self {
            tools: tools.into_iter().collect(),
        }
    }

    pub fn get(&self, tool: ToolId) -> Option<&Capabilities> {
        self.tools.get(&tool)
    }

    pub fn contains(&self, tool: ToolId) -> bool {
        self.tools.contains_key(&tool)
    }
}

/// Native metadata behaviour of a tool's output. Gaps are filled by the core's
/// own metadata handling (M7); this records what the tool does on its own.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataSupport {
    /// Metadata the tool carries from input to output when asked to keep it.
    pub preserves: MetadataKinds,
    /// Can write output with no metadata at all.
    pub can_strip: bool,
    /// Can keep some kinds while dropping others.
    pub selective: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataKinds {
    pub exif: bool,
    pub xmp: bool,
    pub icc: bool,
}

impl MetadataKinds {
    pub const ALL: Self = Self {
        exif: true,
        xmp: true,
        icc: true,
    };
    pub const ICC_ONLY: Self = Self {
        exif: false,
        xmp: false,
        icc: true,
    };
    pub const NONE: Self = Self {
        exif: false,
        xmp: false,
        icc: false,
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_needs_both_read_and_write() {
        let caps = Capabilities {
            reads: [Format::Png, Format::Jpeg].into(),
            writes: [Format::Webp].into(),
            ..Capabilities::default()
        };
        assert!(caps.converts(Format::Png, Format::Webp));
        assert!(!caps.converts(Format::Gif, Format::Webp));
        assert!(!caps.converts(Format::Png, Format::Jxl));
    }
}
