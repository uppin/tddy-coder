//! Read session metadata from user's `~/.tddy/sessions/<session_id>/`.

use std::path::Path;

use tddy_core::output::SESSIONS_SUBDIR;
use tddy_core::{read_session_metadata, SessionMetadata};

/// Session entry for listing (from .session.yaml).
#[derive(Debug, Clone)]
pub struct SessionEntry {
    pub session_id: String,
    pub created_at: String,
    pub status: String,
    pub repo_path: String,
    pub project_id: String,
    pub pid: Option<u32>,
    pub is_active: bool,
    pub tool: String,
    pub session_type: String,
    pub updated_at: String,
    pub livekit_room: String,
    pub previous_session_id: String,
}

/// Check if a process with the given PID is alive (same semantics as listing sessions).
#[cfg(unix)]
pub(crate) fn is_pid_alive(pid: u32) -> bool {
    let ret = unsafe { libc::kill(pid as i32, 0) };
    ret == 0
}

/// Non-Unix stub: treat every PID as not alive so `is_active` is always false in listings.
/// Session delete therefore does not use `kill(2)` semantics; callers on non-Unix targets should
/// treat process state as best-effort only.
#[cfg(not(unix))]
pub(crate) fn is_pid_alive(_pid: u32) -> bool {
    false
}

/// List sessions under `{sessions_base}/sessions/`.
/// Each subdir with `.session.yaml` is returned.
/// is_active is true when pid is set and the process is alive.
pub fn list_sessions_in_dir(sessions_base: &Path) -> anyhow::Result<Vec<SessionEntry>> {
    let mut result = Vec::new();
    let sessions_root = sessions_base.join(SESSIONS_SUBDIR);
    let entries = match std::fs::read_dir(&sessions_root) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(e.into()),
    };

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let session_dir = path;
        let metadata_path = session_dir.join(tddy_core::SESSION_METADATA_FILENAME);
        if !metadata_path.exists() {
            continue;
        }
        let metadata: SessionMetadata = match read_session_metadata(&session_dir) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let is_active = metadata.pid.map(is_pid_alive).unwrap_or(false);
        result.push(SessionEntry {
            session_id: metadata.session_id,
            created_at: metadata.created_at,
            status: metadata.status,
            repo_path: metadata.repo_path.unwrap_or_default(),
            project_id: metadata.project_id,
            pid: metadata.pid,
            is_active,
            tool: metadata.tool.unwrap_or_default(),
            session_type: metadata.session_type.unwrap_or_default(),
            updated_at: metadata.updated_at,
            livekit_room: metadata.livekit_room.unwrap_or_default(),
            previous_session_id: metadata.previous_session_id.unwrap_or_default(),
        });
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tddy_core::output::SESSIONS_SUBDIR;
    use tddy_core::{write_session_metadata, SessionMetadata};
    use std::fs;

    #[test]
    fn list_sessions_returns_new_metadata_fields() {
        let temp = tempfile::tempdir().unwrap();
        let session_id = "test-reader-session-001";
        let session_dir = temp.path().join(SESSIONS_SUBDIR).join(session_id);
        fs::create_dir_all(&session_dir).unwrap();

        let metadata = SessionMetadata {
            session_id: session_id.to_string(),
            project_id: "".to_string(),
            created_at: "2026-06-21T10:00:00Z".to_string(),
            updated_at: "2026-06-21T11:00:00Z".to_string(),
            status: "active".to_string(),
            repo_path: Some("/tmp/my-repo".to_string()),
            pid: None,
            tool: Some("tddy-coder".to_string()),
            livekit_room: Some("room-abc".to_string()),
            pending_elicitation: false,
            previous_session_id: Some("prev-session-000".to_string()),
            session_type: Some("tool".to_string()),
            model: None,
            activity_status: None,
            hook_token: None,
        };
        write_session_metadata(&session_dir, &metadata).unwrap();

        let entries = list_sessions_in_dir(temp.path()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].tool, "tddy-coder");
        assert_eq!(entries[0].livekit_room, "room-abc");
        assert_eq!(entries[0].updated_at, "2026-06-21T11:00:00Z");
        assert_eq!(entries[0].session_type, "tool");
        assert_eq!(entries[0].previous_session_id, "prev-session-000");
    }
}
