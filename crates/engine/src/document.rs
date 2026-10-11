//! Native file status, independent of document history and transient selection.

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use viamind_doc::Project;

use crate::EventCursor;

/// Save to the active file or a supplied native path.
#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SaveProject {
    /// Destination path. Omit to replace the current active file.
    #[serde(default)]
    pub path: Option<PathBuf>,
    /// Allow replacing a supplied destination. Defaults to false; saving the active file replaces it.
    #[serde(default)]
    pub overwrite: bool,
}

/// Shared native file status. Saving and selection do not create undo steps.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DocumentStatus {
    /// Absolute active path, or none for an unnamed document.
    pub path: Option<PathBuf>,
    /// Whether the current project differs from the exact saved baseline.
    pub dirty: bool,
}

/// Successful persistence of a particular snapshot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SaveResult {
    /// Absolute saved path, now the active file.
    pub path: PathBuf,
    /// Whether newer edits differ from the saved snapshot.
    pub dirty: bool,
    /// Cursor paired with the snapshot actually written to disk.
    pub saved_cursor: EventCursor,
}

#[derive(Debug)]
pub(crate) struct DocumentState {
    pub(crate) saved: Project,
    pub(crate) path: Option<PathBuf>,
    pub(crate) dirty: bool,
}

impl DocumentState {
    pub(crate) fn new(project: Project) -> Self {
        Self { saved: project, path: None, dirty: false }
    }

    pub(crate) fn status(&self) -> DocumentStatus {
        DocumentStatus { path: self.path.clone(), dirty: self.dirty }
    }
}
