//! What the app was working on, kept between launches.

use serde::{Deserialize, Serialize};

use crate::preset::Preset;
use crate::validation::ValidationError;

/// The desktop app's working state, restored at the next launch: the
/// configuration being edited, saved or not. Inputs aren't kept, because a
/// dropped folder is a one-off snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LastSession {
    /// May have no outputs yet: it's a draft, not a preset to run.
    pub preset: Preset,
    /// The stored preset it was loaded from or last saved as, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset_id: Option<String>,
}

impl LastSession {
    pub fn validate(&self) -> Result<(), ValidationError> {
        self.preset.validate_draft()
    }
}
