//! Session metadata for daemon session discovery.
//!
//! Stored as `.session.yaml` in each session directory.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// Session metadata written to .session.yaml for daemon session listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionMetadata {
    pub session_id: String,
    pub project_id: String,
    pub created_at: String,
    pub updated_at: String,
    pub status: String,
    #[serde(default)]
    pub repo_path: Option<String>,
    #[serde(default)]
    pub pid: Option<u32>,
    #[serde(default)]
    pub tool: Option<String>,
    #[serde(default)]
    pub livekit_room: Option<String>,
}

pub const SESSION_METADATA_FILENAME: &str = ".session.yaml";

/// Write session metadata to the session directory.
pub fn write_session_metadata(
    session_dir: &Path,
    metadata: &SessionMetadata,
) -> Result<(), crate::WorkflowError> {
    let path = session_dir.join(SESSION_METADATA_FILENAME);
    let contents = serde_yaml::to_string(metadata)
        .map_err(|e| crate::WorkflowError::WriteFailed(e.to_string()))?;
    std::fs::write(&path, contents)
        .map_err(|e| crate::WorkflowError::WriteFailed(e.to_string()))?;
    Ok(())
}

/// Read session metadata from the session directory.
pub fn read_session_metadata(session_dir: &Path) -> Result<SessionMetadata, crate::WorkflowError> {
    let path = session_dir.join(SESSION_METADATA_FILENAME);
    let contents = std::fs::read_to_string(&path)
        .map_err(|e| crate::WorkflowError::WriteFailed(e.to_string()))?;
    serde_yaml::from_str(&contents).map_err(|e| crate::WorkflowError::WriteFailed(e.to_string()))
}
