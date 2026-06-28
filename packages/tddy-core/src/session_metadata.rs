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
    /// When true, the workflow is waiting on the user (plan/doc approval, clarifications, etc.).
    #[serde(default)]
    pub pending_elicitation: bool,
    /// Optional parent session when this session was created as a chain child (PRD: session chaining).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_session_id: Option<String>,
    /// Session type: "tool" (default/empty) or "claude-cli". Absent in legacy files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_type: Option<String>,
    /// Model id for claude-cli sessions (e.g. "claude-opus-4-8"). Absent in legacy files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Granular activity status reported by per-worktree claude-cli hooks (e.g. "Running",
    /// "WaitingForInput"). Absent for tool sessions and legacy files. Set by `ReportSessionStatus`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activity_status: Option<String>,
    /// Per-session token authorising claude-cli hooks to call `ReportSessionStatus`. Generated at
    /// session-start, persisted here, and baked into the worktree hook command. Absent for tool
    /// sessions and legacy files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hook_token: Option<String>,
    /// When true, the claude-cli session runs inside a platform sandbox (darwin Seatbelt).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sandbox: Option<bool>,
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
    /// When set, `.session.yaml` records the stacked-from parent session id.
    pub previous_session_id: Option<String>,
    /// Session type: "tool" (default/empty) or "claude-cli".
    pub session_type: Option<String>,
    /// Model id for claude-cli sessions.
    pub model: Option<String>,
    /// Initial granular activity status for claude-cli sessions. Usually `None` for tool sessions.
    pub activity_status: Option<String>,
    /// Per-session hook token for claude-cli sessions. `None` for tool sessions.
    pub hook_token: Option<String>,
    /// When true, the claude-cli session runs inside a platform sandbox (darwin Seatbelt).
    pub sandbox: Option<bool>,
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
        pending_elicitation: false,
        previous_session_id: opts.previous_session_id,
        session_type: opts.session_type,
        model: opts.model,
        activity_status: opts.activity_status,
        hook_token: opts.hook_token,
        sandbox: opts.sandbox,
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

/// Atomically update the `activity_status` field in an existing `.session.yaml`.
///
/// Reads the metadata, sets `activity_status = Some(status.to_string())`, bumps `updated_at`,
/// and writes it back. All other fields are preserved.
///
/// Used by the `ReportSessionStatus` gRPC handler to record the latest hook-reported status.
pub fn update_activity_status(
    session_dir: &Path,
    status: &str,
) -> Result<(), crate::WorkflowError> {
    let mut metadata = read_session_metadata(session_dir)?;
    metadata.activity_status = Some(status.to_string());
    metadata.updated_at = chrono::Utc::now().to_rfc3339();
    write_session_metadata(session_dir, &metadata)
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
                previous_session_id: None,
                session_type: None,
                model: None,

                activity_status: None,
                hook_token: None,
                sandbox: None,
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
        assert!(!read.pending_elicitation);
        assert!(read.previous_session_id.is_none());

        let _ = fs::remove_dir_all(&tmp);
    }

    /// **claude_cli_metadata_round_trip** — `.session.yaml` must preserve `session_type` and
    /// `model` through a write/read cycle.
    #[test]
    fn claude_cli_metadata_round_trip() {
        let tmp = std::env::temp_dir().join(format!(
            "tddy-session-meta-claude-cli-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&tmp);
        let sid = "01900000-0000-7000-8000-000000000cli";
        let session_dir = tmp.join("sessions").join(sid);
        fs::create_dir_all(&session_dir).unwrap();

        write_initial_tool_session_metadata(
            &session_dir,
            InitialToolSessionMetadataOpts {
                project_id: "proj-claude".to_string(),
                repo_path: Some("/tmp/worktrees/claude-cli-01900000".to_string()),
                pid: Some(7777),
                tool: None,
                livekit_room: None,
                previous_session_id: None,
                session_type: Some("claude-cli".to_string()), // NEW FIELD — compile error
                model: Some("claude-sonnet-4-6".to_string()), // NEW FIELD — compile error

                activity_status: None,
                hook_token: None,
                sandbox: None,
            },
        )
        .unwrap();

        let read = read_session_metadata(&session_dir).unwrap();
        assert_eq!(
            read.session_type.as_deref(),
            Some("claude-cli"),
            "session_type must survive write/read round-trip"
        );
        assert_eq!(
            read.model.as_deref(),
            Some("claude-sonnet-4-6"),
            "model must survive write/read round-trip"
        );

        // Verify that legacy .session.yaml without session_type/model still deserializes (backward
        // compatibility: both fields must have #[serde(default)]).
        let legacy_yaml = format!(
            r#"session_id: {sid}
project_id: proj-legacy
created_at: "2026-01-01T00:00:00Z"
updated_at: "2026-01-01T00:00:00Z"
status: active
"#
        );
        let legacy: SessionMetadata = serde_yaml::from_str(&legacy_yaml)
            .expect("legacy .session.yaml without session_type/model must deserialize");
        assert!(
            legacy.session_type.is_none(),
            "session_type must default to None for legacy sessions"
        );
        assert!(
            legacy.model.is_none(),
            "model must default to None for legacy sessions"
        );

        let _ = fs::remove_dir_all(&tmp);
    }

    /// `activity_status` survives a write/read round-trip through `.session.yaml`.
    #[test]
    fn activity_status_round_trips_through_session_yaml() {
        let tmp =
            std::env::temp_dir().join(format!("tddy-activity-status-rt-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        let session_dir = tmp.join("sessions").join("sess-act-rt");
        fs::create_dir_all(&session_dir).unwrap();

        write_initial_tool_session_metadata(
            &session_dir,
            InitialToolSessionMetadataOpts {
                project_id: "proj-rt".to_string(),
                session_type: Some("claude-cli".to_string()),
                model: Some("claude-sonnet-4-6".to_string()),
                activity_status: Some("Running".to_string()),
                hook_token: None,
                ..Default::default()
            },
        )
        .unwrap();

        let read = read_session_metadata(&session_dir).unwrap();
        assert_eq!(
            read.activity_status.as_deref(),
            Some("Running"),
            "activity_status must survive write/read round-trip"
        );

        let _ = fs::remove_dir_all(&tmp);
    }

    /// `hook_token` is omitted from the YAML when `None` (no key present in file).
    #[test]
    fn hook_token_omitted_when_none() {
        let tmp = std::env::temp_dir().join(format!("tddy-hook-token-none-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        let session_dir = tmp.join("sessions").join("sess-ht-none");
        fs::create_dir_all(&session_dir).unwrap();

        write_initial_tool_session_metadata(
            &session_dir,
            InitialToolSessionMetadataOpts {
                project_id: "proj-ht".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        let yaml_text =
            std::fs::read_to_string(session_dir.join(SESSION_METADATA_FILENAME)).unwrap();
        assert!(
            !yaml_text.contains("hook_token"),
            "hook_token must not appear in YAML when None; got:\n{yaml_text}"
        );
        assert!(
            !yaml_text.contains("activity_status"),
            "activity_status must not appear in YAML when None; got:\n{yaml_text}"
        );

        let _ = fs::remove_dir_all(&tmp);
    }

    /// Legacy `.session.yaml` without `activity_status` or `hook_token` must still deserialise
    /// (both fields have `#[serde(default)]`).
    #[test]
    fn legacy_session_yaml_without_new_fields_deserializes() {
        let yaml = r#"session_id: old-sess
project_id: proj-legacy
created_at: "2026-01-01T00:00:00Z"
updated_at: "2026-01-01T00:00:00Z"
status: active
"#;
        let meta: SessionMetadata =
            serde_yaml::from_str(yaml).expect("legacy YAML must deserialise");
        assert!(
            meta.activity_status.is_none(),
            "activity_status must default to None"
        );
        assert!(meta.hook_token.is_none(), "hook_token must default to None");
    }

    /// `update_activity_status` must overwrite only `activity_status` and bump `updated_at`;
    /// all other fields (including `status`) must be unchanged.
    #[test]
    fn update_activity_status_overwrites_only_status_and_bumps_updated_at() {
        let tmp = std::env::temp_dir().join(format!("tddy-upd-act-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        let session_dir = tmp.join("sessions").join("sess-upd");
        fs::create_dir_all(&session_dir).unwrap();

        let original_updated_at = "2026-06-13T10:00:00Z";
        write_initial_tool_session_metadata(
            &session_dir,
            InitialToolSessionMetadataOpts {
                project_id: "proj-upd".to_string(),
                session_type: Some("claude-cli".to_string()),
                activity_status: Some("Started".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        // Manually set a known `updated_at` for comparison.
        {
            let mut meta = read_session_metadata(&session_dir).unwrap();
            meta.updated_at = original_updated_at.to_string();
            write_session_metadata(&session_dir, &meta).unwrap();
        }

        update_activity_status(&session_dir, "WaitingForInput")
            .expect("update_activity_status must succeed");

        let updated = read_session_metadata(&session_dir).unwrap();
        assert_eq!(
            updated.activity_status.as_deref(),
            Some("WaitingForInput"),
            "activity_status must be updated to WaitingForInput"
        );
        assert_eq!(
            updated.status, "active",
            "session status field must remain 'active'"
        );
        assert_ne!(
            updated.updated_at, original_updated_at,
            "updated_at must be bumped by update_activity_status"
        );

        let _ = fs::remove_dir_all(&tmp);
    }

    /// **chain_child_metadata_records_previous_session_id** — `.session.yaml` must allow optional
    /// `previous_session_id` on [`SessionMetadata`] (PRD: chain child observability).
    #[test]
    fn chain_child_metadata_records_previous_session_id() {
        let sid = "018f1234-5678-7abc-8def-123456789abc";
        let prev = "019f1234-5678-7abc-8def-123456789abc";
        let yaml = format!(
            r#"session_id: {sid}
project_id: proj-chain
created_at: "2026-05-01T12:00:00Z"
updated_at: "2026-05-01T12:00:00Z"
status: active
previous_session_id: {prev}
"#
        );
        assert!(
            serde_yaml::from_str::<SessionMetadata>(&yaml).is_ok(),
            "SessionMetadata must accept optional previous_session_id and deserialize from .session.yaml (PRD session chaining)"
        );
    }
}
