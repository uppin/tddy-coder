//! Session directories and shared path helpers (goal-agnostic).
//! TDD artifact writers and structured output types live in `tddy-workflow-recipes`.

use crate::error::WorkflowError;
use std::fs;
use std::path::{Path, PathBuf};

/// Inject a "Related Documents" section with relative links to peer .md files.
pub fn inject_cross_references(content: &str, session_dir: &Path, self_name: &str) -> String {
    let mut peers: Vec<String> = fs::read_dir(session_dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            if name.ends_with(".md") && name != self_name {
                Some(format!("[{}](./{})", name, name))
            } else {
                None
            }
        })
        .collect();
    peers.sort();
    if peers.is_empty() {
        return content.to_string();
    }
    let mut out = content.to_string();
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("\n## Related Documents\n\n");
    for p in &peers {
        out.push_str(&format!("- {}\n", p));
    }
    out
}

/// Generate a directory name from the feature description: YYYY-MM-DD-<slug>.
pub fn slugify_directory_name(feature: &str) -> String {
    let date = format_date_today();
    let slug = slugify(feature, 50);
    format!("{}-{}", date, slug)
}

/// Root directory for plan markdown and other workflow artifacts — `session_dir/artifacts/`.
#[inline]
pub fn plan_artifacts_root(session_dir: &Path) -> PathBuf {
    tddy_workflow::session_artifacts_root(session_dir)
}

fn format_date_today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

fn slugify(s: &str, max_len: usize) -> String {
    let mut out = String::with_capacity(s.len().min(max_len));
    let mut prev_space = false;
    for c in s.chars().take(max_len) {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_lowercase().next().unwrap_or(c));
            prev_space = false;
        } else if (c.is_whitespace() || c == '-' || c == '_') && !prev_space && !out.is_empty() {
            out.push('-');
            prev_space = true;
        }
    }
    out.trim_matches('-').to_string()
}

/// Write the session ID to `.session` in the output directory.
pub fn write_session_file(output_dir: &Path, session_id: &str) -> Result<(), WorkflowError> {
    let session_path = output_dir.join(".session");
    fs::write(&session_path, session_id).map_err(|e| WorkflowError::WriteFailed(e.to_string()))?;
    Ok(())
}

/// Read the session ID from `.session` in the plan directory.
pub fn read_session_file(session_dir: &Path) -> Result<String, WorkflowError> {
    let session_path = session_dir.join(".session");
    fs::read_to_string(&session_path).map_err(|e| WorkflowError::SessionMissing(format!("{}", e)))
}

/// Write the implementation session ID to `.impl-session` in the plan directory.
pub fn write_impl_session_file(session_dir: &Path, session_id: &str) -> Result<(), WorkflowError> {
    let session_path = session_dir.join(".impl-session");
    fs::write(&session_path, session_id).map_err(|e| WorkflowError::WriteFailed(e.to_string()))?;
    Ok(())
}

/// Read the implementation session ID from `.impl-session` in the plan directory.
pub fn read_impl_session_file(session_dir: &Path) -> Result<String, WorkflowError> {
    let session_path = session_dir.join(".impl-session");
    fs::read_to_string(&session_path).map_err(|e| WorkflowError::SessionMissing(format!("{}", e)))
}

/// Subdirectory name for session directories under a base path.
pub const SESSIONS_SUBDIR: &str = "sessions";

/// Profile-aware default for the tddy data dir root.
/// - debug: `Some("tmp/.tddy")` (repo-local, CWD-relative)
/// - release: `None` (callers resolve per-user `$HOME/.tddy`)
pub fn default_tddy_data_dir() -> Option<PathBuf> {
    default_tddy_data_dir_for(cfg!(debug_assertions))
}

/// Testable split: `debug=true` → `Some("tmp/.tddy")`, `debug=false` → `None`.
pub fn default_tddy_data_dir_for(debug: bool) -> Option<PathBuf> {
    debug.then(|| PathBuf::from("tmp").join(".tddy"))
}

/// Create a session directory at `{base}/sessions/{uuid-v7}/` and return its path.
pub fn create_session_dir_in(base: &Path) -> Result<PathBuf, WorkflowError> {
    use uuid::Uuid;
    let id = Uuid::now_v7();
    create_session_dir_with_id(base, &id.to_string())
}

/// Create a session directory at `{base}/sessions/{id}/` using the given session id.
pub fn create_session_dir_with_id(base: &Path, session_id: &str) -> Result<PathBuf, WorkflowError> {
    let sessions_dir = base.join(SESSIONS_SUBDIR);
    let session_dir = sessions_dir.join(session_id);
    fs::create_dir_all(&session_dir).map_err(|e| WorkflowError::WriteFailed(e.to_string()))?;
    Ok(session_dir)
}

/// Create a session directory at `{base}/sessions/{id}/`.
///
/// Historically some call sites treated `base` as “already the sessions folder”; the unified
/// contract (CLI, daemon, RPC) is always `{base}/sessions/<session_id>/`, same as
/// [`create_session_dir_with_id`].
pub fn create_session_dir_under(base: &Path, session_id: &str) -> Result<PathBuf, WorkflowError> {
    create_session_dir_with_id(base, session_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_tddy_data_dir_debug_is_tmp_tddy() {
        assert_eq!(default_tddy_data_dir_for(true), Some(PathBuf::from("tmp").join(".tddy")));
    }

    #[test]
    fn default_tddy_data_dir_release_is_none() {
        assert_eq!(default_tddy_data_dir_for(false), None);
    }

    #[test]
    fn plan_artifacts_root_is_under_session_dir() {
        let root = std::env::temp_dir().join(format!(
            "tddy-plan-artifact-sessions-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let sessions = root.join("sessions");
        let sid = sessions.join("a97addd3-c31b-442b-a6b0-a63abe99e11d");
        std::fs::create_dir_all(&sid).unwrap();
        assert_eq!(plan_artifacts_root(&sid), sid.join("artifacts"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn create_session_dir_in_uses_explicit_base_not_repo_path() {
        let sessions_home =
            std::env::temp_dir().join(format!("tddy-core-sessions-home-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&sessions_home);
        let repo =
            std::env::temp_dir().join(format!("tddy-plan-artifact-repo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&repo);
        std::fs::create_dir_all(&repo).unwrap();
        let got = create_session_dir_in(&sessions_home).unwrap();
        assert!(
            !got.starts_with(&repo),
            "session dir must not be derived from repo path"
        );
        let _ = std::fs::remove_dir_all(&repo);
        let _ = std::fs::remove_dir_all(&sessions_home);
    }
}
