//! Resolve paths under `session_dir/artifacts/` for workflow session artifacts.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// Default subdirectory for session-scoped workflow outputs (new sessions).
#[inline]
pub fn session_artifacts_root(session_dir: &Path) -> PathBuf {
    let root = session_dir.join("artifacts");
    log::debug!(
        "[tddy_workflow] session_artifacts_root session_dir={:?} -> {:?}",
        session_dir,
        root
    );
    root
}

/// Canonical path for a new write: `session_dir/artifacts/<basename>`.
#[inline]
pub fn canonical_artifact_write_path(session_dir: &Path, basename: &str) -> PathBuf {
    let p = session_artifacts_root(session_dir).join(basename);
    log::debug!(
        "[tddy_workflow] canonical_artifact_write_path basename={:?} -> {:?}",
        basename,
        p
    );
    p
}

/// When `session_dir` is nested under `.../sessions/<uuid>/...`, returns `<uuid>/<basename>` if that file exists (legacy layout).
fn artifact_at_sessions_uuid_root(session_dir: &Path, basename: &str) -> Option<PathBuf> {
    let mut current = session_dir.to_path_buf();
    loop {
        let parent = current.parent()?;
        if parent.file_name() == Some(OsStr::new("sessions")) {
            let p = current.join(basename);
            if p.is_file() {
                return Some(p);
            }
            return None;
        }
        if parent == current {
            break;
        }
        current = parent.to_path_buf();
    }
    None
}

/// Resolves an existing file for `basename` (prefers `artifacts/`, then legacy UUID-root, then session root).
pub fn resolve_existing_session_artifact(session_dir: &Path, basename: &str) -> Option<PathBuf> {
    let artifacts_path = session_artifacts_root(session_dir).join(basename);
    if artifacts_path.is_file() {
        log::info!(
            "[tddy_workflow] session artifact (artifacts): {:?}",
            artifacts_path
        );
        return Some(artifacts_path);
    }
    if let Some(p) = artifact_at_sessions_uuid_root(session_dir, basename) {
        log::info!(
            "[tddy_workflow] session artifact (legacy uuid root): {:?}",
            p
        );
        return Some(p);
    }
    let flat = session_dir.join(basename);
    if flat.is_file() {
        log::info!(
            "[tddy_workflow] session artifact (session root): {:?}",
            flat
        );
        return Some(flat);
    }
    log::debug!(
        "[tddy_workflow] no session artifact basename={:?} under {:?}",
        basename,
        session_dir
    );
    None
}

/// Message substituted when the artifact is missing or cannot be read as UTF-8.
pub const SESSION_ARTIFACT_READ_PLACEHOLDER: &str = "Could not read session artifact";

/// Reads UTF-8 from the resolved artifact path, if the file exists and is readable.
pub fn read_session_artifact_utf8(session_dir: &Path, basename: &str) -> Option<String> {
    resolve_existing_session_artifact(session_dir, basename)
        .and_then(|p| std::fs::read_to_string(p).ok())
}

/// Like [`read_session_artifact_utf8`], but returns [`SESSION_ARTIFACT_READ_PLACEHOLDER`] when missing or unreadable.
pub fn read_session_artifact_utf8_or_placeholder(session_dir: &Path, basename: &str) -> String {
    read_session_artifact_utf8(session_dir, basename)
        .unwrap_or_else(|| SESSION_ARTIFACT_READ_PLACEHOLDER.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn session_artifacts_root_appends_artifacts_segment() {
        let dir =
            std::env::temp_dir().join(format!("tddy-wf-artifacts-root-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let root = session_artifacts_root(&dir);
        assert_eq!(
            root,
            dir.join("artifacts"),
            "session_artifacts_root must return session_dir/artifacts/"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn canonical_write_path_under_artifacts_subdir() {
        let dir =
            std::env::temp_dir().join(format!("tddy-wf-canonical-artifact-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("artifacts")).unwrap();
        let path = canonical_artifact_write_path(&dir, "PRD.md");
        assert_eq!(
            path,
            dir.join("artifacts").join("PRD.md"),
            "canonical path must be session_dir/artifacts/<basename> for default layout"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn nested_under_sessions_uuid_prefers_uuid_root_when_no_artifacts_file() {
        let root = std::env::temp_dir().join(format!(
            "tddy-session-artifact-nested-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let uuid = root
            .join("sessions")
            .join("a97addd3-c31b-442b-a6b0-a63abe99e11d");
        let nested = uuid.join("2026-03-24-feature");
        fs::create_dir_all(&nested).unwrap();
        fs::write(uuid.join("PRD.md"), "FULL\n").unwrap();
        fs::write(nested.join("PRD.md"), "legacy-nested-only\n").unwrap();

        let path = resolve_existing_session_artifact(&nested, "PRD.md").unwrap();
        assert_eq!(path, uuid.join("PRD.md"));
        assert_eq!(fs::read_to_string(&path).unwrap(), "FULL\n");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn session_dir_at_uuid_uses_that_file_when_no_artifacts() {
        let root = std::env::temp_dir().join(format!(
            "tddy-session-artifact-at-uuid-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let uuid = root.join("sessions").join("uuid-here");
        fs::create_dir_all(&uuid).unwrap();
        fs::write(uuid.join("PRD.md"), "at-uuid\n").unwrap();

        let path = resolve_existing_session_artifact(&uuid, "PRD.md").unwrap();
        assert_eq!(path, uuid.join("PRD.md"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn path_without_sessions_segment_uses_session_root_file() {
        let root =
            std::env::temp_dir().join(format!("tddy-session-artifact-flat-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("PRD.md"), "only\n").unwrap();
        let path = resolve_existing_session_artifact(&root, "PRD.md").unwrap();
        assert_eq!(path, root.join("PRD.md"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn artifacts_subdir_wins_over_legacy_flat() {
        let dir =
            std::env::temp_dir().join(format!("tddy-wf-artifacts-wins-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("artifacts")).unwrap();
        fs::write(dir.join("artifacts").join("PRD.md"), "in-artifacts\n").unwrap();
        fs::write(dir.join("PRD.md"), "flat\n").unwrap();
        let path = resolve_existing_session_artifact(&dir, "PRD.md").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "in-artifacts\n");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_session_artifact_utf8_matches_resolve_and_read() {
        let dir = std::env::temp_dir().join(format!("tddy-wf-read-utf8-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("artifacts")).unwrap();
        fs::write(dir.join("artifacts").join("PRD.md"), "hello").unwrap();
        assert_eq!(
            read_session_artifact_utf8(&dir, "PRD.md").as_deref(),
            Some("hello")
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
