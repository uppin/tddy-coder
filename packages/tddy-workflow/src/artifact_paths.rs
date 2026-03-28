//! Resolve paths under `session_dir/artifacts/` per workflow manifest.

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

/// Canonical path for the primary planning document for **new** writes (`session_dir/artifacts/<basename>`).
#[inline]
pub fn primary_planning_artifact_path_for_basename(session_dir: &Path, basename: &str) -> PathBuf {
    let p = session_artifacts_root(session_dir).join(basename);
    log::debug!(
        "[tddy_workflow] primary_planning_artifact_path_for_basename basename={:?} -> {:?}",
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

/// Resolves which on-disk file to read for the primary planning document (prefers `artifacts/`, then legacy UUID-root, then session root).
pub fn resolve_existing_primary_planning_document(
    session_dir: &Path,
    basename: &str,
) -> Option<PathBuf> {
    let artifacts_path = session_artifacts_root(session_dir).join(basename);
    if artifacts_path.is_file() {
        log::info!(
            "[tddy_workflow] primary planning document (artifacts): {:?}",
            artifacts_path
        );
        return Some(artifacts_path);
    }
    if let Some(p) = artifact_at_sessions_uuid_root(session_dir, basename) {
        log::info!(
            "[tddy_workflow] primary planning document (legacy uuid root): {:?}",
            p
        );
        return Some(p);
    }
    let flat = session_dir.join(basename);
    if flat.is_file() {
        log::info!(
            "[tddy_workflow] primary planning document (session root): {:?}",
            flat
        );
        return Some(flat);
    }
    log::debug!(
        "[tddy_workflow] no primary planning document basename={:?} under {:?}",
        basename,
        session_dir
    );
    None
}

/// Message substituted when the primary planning document is missing or cannot be read as UTF-8.
pub const PRIMARY_PLANNING_DOCUMENT_READ_PLACEHOLDER: &str =
    "Could not read primary planning document";

/// Reads UTF-8 from the resolved primary planning document path, if the file exists and is readable.
pub fn read_primary_planning_document_utf8(session_dir: &Path, basename: &str) -> Option<String> {
    resolve_existing_primary_planning_document(session_dir, basename)
        .and_then(|p| std::fs::read_to_string(p).ok())
}

/// Like [`read_primary_planning_document_utf8`], but returns [`PRIMARY_PLANNING_DOCUMENT_READ_PLACEHOLDER`]
/// when the file is missing or unreadable.
pub fn read_primary_planning_document_utf8_or_placeholder(
    session_dir: &Path,
    basename: &str,
) -> String {
    read_primary_planning_document_utf8(session_dir, basename)
        .unwrap_or_else(|| PRIMARY_PLANNING_DOCUMENT_READ_PLACEHOLDER.to_string())
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
    fn primary_planning_artifact_lives_under_artifacts_subdir() {
        let dir =
            std::env::temp_dir().join(format!("tddy-wf-primary-artifact-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("artifacts")).unwrap();
        let path = primary_planning_artifact_path_for_basename(&dir, "PRD.md");
        assert_eq!(
            path,
            dir.join("artifacts").join("PRD.md"),
            "primary path must be session_dir/artifacts/<basename> for default layout"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn nested_under_sessions_uuid_prefers_uuid_root_when_no_artifacts_file() {
        let root =
            std::env::temp_dir().join(format!("tddy-session-prd-nested-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let uuid = root
            .join("sessions")
            .join("a97addd3-c31b-442b-a6b0-a63abe99e11d");
        let nested = uuid.join("2026-03-24-feature");
        fs::create_dir_all(&nested).unwrap();
        fs::write(uuid.join("PRD.md"), "FULL\n").unwrap();
        fs::write(nested.join("PRD.md"), "legacy-nested-only\n").unwrap();

        let path = resolve_existing_primary_planning_document(&nested, "PRD.md").unwrap();
        assert_eq!(path, uuid.join("PRD.md"));
        assert_eq!(fs::read_to_string(&path).unwrap(), "FULL\n");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn session_dir_at_uuid_uses_that_prd_when_no_artifacts() {
        let root =
            std::env::temp_dir().join(format!("tddy-session-prd-at-uuid-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let uuid = root.join("sessions").join("uuid-here");
        fs::create_dir_all(&uuid).unwrap();
        fs::write(uuid.join("PRD.md"), "at-uuid\n").unwrap();

        let path = resolve_existing_primary_planning_document(&uuid, "PRD.md").unwrap();
        assert_eq!(path, uuid.join("PRD.md"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn path_without_sessions_segment_uses_session_root_file() {
        let root =
            std::env::temp_dir().join(format!("tddy-session-prd-flat-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("PRD.md"), "only\n").unwrap();
        let path = resolve_existing_primary_planning_document(&root, "PRD.md").unwrap();
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
        let path = resolve_existing_primary_planning_document(&dir, "PRD.md").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "in-artifacts\n");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_primary_planning_utf8_matches_resolve_and_read() {
        let dir = std::env::temp_dir().join(format!("tddy-wf-read-utf8-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("artifacts")).unwrap();
        fs::write(dir.join("artifacts").join("PRD.md"), "hello").unwrap();
        assert_eq!(
            read_primary_planning_document_utf8(&dir, "PRD.md").as_deref(),
            Some("hello")
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
