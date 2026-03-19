//! Read session metadata from user's ~/.tddy/sessions/.

use std::path::Path;

use tddy_core::{read_session_metadata, SessionMetadata};

/// Session entry for listing (from .session.yaml).
#[derive(Debug, Clone)]
pub struct SessionEntry {
    pub session_id: String,
    pub created_at: String,
    pub status: String,
    pub repo_path: String,
    pub pid: Option<u32>,
    pub is_active: bool,
}

/// Check if a process with the given PID is alive.
#[cfg(unix)]
fn is_pid_alive(pid: u32) -> bool {
    let ret = unsafe { libc::kill(pid as i32, 0) };
    ret == 0
}

#[cfg(not(unix))]
fn is_pid_alive(_pid: u32) -> bool {
    false
}

/// List sessions in the given sessions base directory.
/// Each subdir with .session.yaml is returned.
/// is_active is true when pid is set and the process is alive.
pub fn list_sessions_in_dir(sessions_base: &Path) -> anyhow::Result<Vec<SessionEntry>> {
    let mut result = Vec::new();
    let entries = match std::fs::read_dir(sessions_base) {
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
            pid: metadata.pid,
            is_active,
        });
    }

    Ok(result)
}
