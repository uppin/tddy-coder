//! Session directories and shared path helpers (goal-agnostic).
//! TDD artifact writers and structured output types live in `tddy-workflow-recipes`.

use crate::error::WorkflowError;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

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

/// Allocates a new session directory when no `session_dir` / `session_base`+`session_id` is in
/// context yet: `{tddy_data_dir}/sessions/{uuid-v7}/`. The name does not derive from feature
/// text; UUID v7 is time-ordered so canonical hyphenated form sorts lexicographically by creation time.
pub fn new_session_dir() -> Result<PathBuf, WorkflowError> {
    create_session_dir_in(&tddy_data_dir_path()?)
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

/// Environment variable to override the session base path. When set, sessions go to
/// `{TDDY_SESSIONS_DIR}/sessions/{uuid-v7}/` instead of `$HOME/.tddy/sessions/{uuid-v7}/`.
/// Use in tests to avoid writing to production ~/.tddy (e.g. set to `$TMPDIR/tddy-test`).
pub const TDDY_SESSIONS_DIR_ENV: &str = "TDDY_SESSIONS_DIR";

static TDDY_DATA_DIR_OVERRIDE: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();

fn tddy_data_dir_override_mutex() -> &'static Mutex<Option<PathBuf>> {
    TDDY_DATA_DIR_OVERRIDE.get_or_init(|| Mutex::new(None))
}

/// Set the Tddy data directory root from YAML (`tddy_data_dir`) or CLI (`--tddy-data-dir`).
/// `None` clears the override so resolution falls through to `$HOME/.tddy` when the env var is unset.
/// Called by tddy-coder after merging config. Ignored for resolution when [`TDDY_SESSIONS_DIR_ENV`] is set.
pub fn set_tddy_data_dir_override(path: Option<PathBuf>) {
    *tddy_data_dir_override_mutex()
        .lock()
        .expect("tddy data dir override lock") = path;
}

/// Resolve the Tddy data directory (parent of the `sessions/` subdir).
///
/// Order: `TDDY_SESSIONS_DIR` → in-process override ([`set_tddy_data_dir_override`]) → `$HOME/.tddy` (Unix).
pub fn tddy_data_dir_path() -> Result<PathBuf, WorkflowError> {
    if let Ok(path) = std::env::var(TDDY_SESSIONS_DIR_ENV) {
        return Ok(PathBuf::from(path));
    }
    if let Some(path) = tddy_data_dir_override_mutex()
        .lock()
        .expect("tddy data dir override lock")
        .clone()
    {
        return Ok(path);
    }
    #[cfg(unix)]
    {
        let home = std::env::var("HOME").map_err(|_| {
            WorkflowError::WriteFailed("HOME not set; set TDDY_SESSIONS_DIR or HOME".into())
        })?;
        Ok(PathBuf::from(home).join(".tddy"))
    }
    #[cfg(not(unix))]
    {
        Err(WorkflowError::WriteFailed(
            "TDDY_SESSIONS_DIR or HOME (Unix) required".into(),
        ))
    }
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
    #[serial_test::serial]
    fn tddy_data_dir_path_prefers_env_over_in_process_override() {
        let via_env = std::env::temp_dir().join(format!("tddy-sbp-env-{}", std::process::id()));
        let via_override =
            std::env::temp_dir().join(format!("tddy-sbp-override-{}", std::process::id()));
        std::env::set_var(TDDY_SESSIONS_DIR_ENV, &via_env);
        set_tddy_data_dir_override(Some(via_override));
        assert_eq!(tddy_data_dir_path().unwrap(), via_env);
        std::env::remove_var(TDDY_SESSIONS_DIR_ENV);
        set_tddy_data_dir_override(None);
    }

    #[test]
    #[serial_test::serial]
    fn tddy_data_dir_path_uses_override_when_env_unset() {
        std::env::remove_var(TDDY_SESSIONS_DIR_ENV);
        let via_override =
            std::env::temp_dir().join(format!("tddy-sbp-only-override-{}", std::process::id()));
        set_tddy_data_dir_override(Some(via_override.clone()));
        assert_eq!(tddy_data_dir_path().unwrap(), via_override);
        set_tddy_data_dir_override(None);
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
