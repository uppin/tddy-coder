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

/// Options for [`write_initial_tool_session_metadata`] (CLI, gRPC daemon, LiveKit, TUI).
#[derive(Debug, Clone, Default)]
pub struct InitialToolSessionMetadataOpts {
    pub project_id: String,
    pub repo_path: Option<String>,
    pub pid: Option<u32>,
    pub tool: Option<String>,
    pub livekit_room: Option<String>,
}

/// Writes `.session.yaml` for a newly created session directory.
///
/// `session_id` is taken from `session_dir`'s final path segment so it stays aligned with the
/// on-disk layout (`…/sessions/<id>/`).
pub fn write_initial_tool_session_metadata(
    session_dir: &Path,
    opts: InitialToolSessionMetadataOpts,
) -> Result<(), crate::WorkflowError> {
    let session_id = session_dir
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            crate::WorkflowError::WriteFailed(
                "write_initial_tool_session_metadata: session_dir has no usable basename"
                    .to_string(),
            )
        })?
        .to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let metadata = SessionMetadata {
        session_id,
        project_id: opts.project_id,
        created_at: now.clone(),
        updated_at: now,
        status: "active".to_string(),
        repo_path: opts.repo_path,
        pid: opts.pid,
        tool: opts.tool,
        livekit_room: opts.livekit_room,
    };
    write_session_metadata(session_dir, &metadata)
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn write_initial_tool_session_metadata_uses_dir_basename_as_session_id() {
        let tmp =
            std::env::temp_dir().join(format!("tddy-session-meta-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        let sid = "018f1234-5678-7abc-8def-123456789abc";
        let session_dir = tmp.join("sessions").join(sid);
        fs::create_dir_all(&session_dir).unwrap();

        write_initial_tool_session_metadata(
            &session_dir,
            InitialToolSessionMetadataOpts {
                project_id: "proj-1".to_string(),
                repo_path: Some("/repo".to_string()),
                pid: Some(4242),
                tool: Some("tddy-coder".to_string()),
                livekit_room: None,
            },
        )
        .unwrap();

        let read = read_session_metadata(&session_dir).unwrap();
        assert_eq!(read.session_id, sid);
        assert_eq!(read.project_id, "proj-1");
        assert_eq!(read.status, "active");
        assert_eq!(read.repo_path.as_deref(), Some("/repo"));
        assert_eq!(read.pid, Some(4242));
        assert_eq!(read.tool.as_deref(), Some("tddy-coder"));
        assert!(read.livekit_room.is_none());

        let _ = fs::remove_dir_all(&tmp);
    }
}
