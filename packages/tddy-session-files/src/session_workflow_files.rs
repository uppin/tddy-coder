//! Allowlisted workflow file listing and UTF-8 reads under a session directory.

use std::path::Path;

use tddy_rpc::Status;

/// Fixed allowlist of workflow basenames (server-side only). Order is stable for documentation;
/// listing returns entries in this order for files that exist and resolve under the session dir.
const WORKFLOW_FILE_ALLOWLIST: &[&str] = &["changeset.yaml", ".session.yaml", "PRD.md", "TODO.md"];

fn basename_is_allowlisted(basename: &str) -> bool {
    WORKFLOW_FILE_ALLOWLIST.contains(&basename)
}

/// Rejects traversal, separators, and basenames outside the allowlist.
fn validate_workflow_basename(basename: &str) -> Result<(), Status> {
    if basename.is_empty() {
        log::debug!("validate_workflow_basename: empty basename");
        return Err(Status::invalid_argument("basename is required"));
    }
    if basename.contains("..") || basename.contains('/') || basename.contains('\\') {
        log::debug!(
            "validate_workflow_basename: rejected unsafe basename segment: {:?}",
            basename
        );
        return Err(Status::invalid_argument(
            "basename must be a single path segment without traversal",
        ));
    }
    if !basename_is_allowlisted(basename) {
        log::debug!(
            "validate_workflow_basename: basename not in allowlist: {:?}",
            basename
        );
        return Err(Status::invalid_argument(
            "basename is not an allowlisted workflow file",
        ));
    }
    Ok(())
}

/// Lists basenames of workflow/plan files that exist under `session_dir` using a server-side allowlist.
///
/// Only regular files (or symlinks to files) whose canonical path stays under the canonical session
/// directory are returned. Sensitive files such as `.env` are never allowlisted.
pub fn list_allowlisted_workflow_basenames(session_dir: &Path) -> Result<Vec<String>, Status> {
    log::debug!(
        "list_allowlisted_workflow_basenames: session_dir={:?}",
        session_dir
    );
    if !session_dir.is_dir() {
        log::debug!("list_allowlisted_workflow_basenames: session_dir is not a directory");
        return Err(Status::failed_precondition(
            "session directory does not exist or is not a directory",
        ));
    }

    let session_root = session_dir.canonicalize().map_err(|e| {
        log::debug!(
            "list_allowlisted_workflow_basenames: canonicalize session_dir failed: {}",
            e
        );
        Status::failed_precondition("session directory is not accessible")
    })?;

    let mut out = Vec::new();
    for name in WORKFLOW_FILE_ALLOWLIST {
        let path = session_dir.join(name);
        if !path.exists() {
            continue;
        }
        let canonical = match path.canonicalize() {
            Ok(p) => p,
            Err(e) => {
                log::debug!(
                    "list_allowlisted_workflow_basenames: skip {:?} (canonicalize failed: {})",
                    name,
                    e
                );
                continue;
            }
        };
        if !canonical.starts_with(&session_root) {
            log::warn!(
                "list_allowlisted_workflow_basenames: skipping {:?} — resolves outside session dir ({:?})",
                name,
                canonical
            );
            continue;
        }
        let meta = match std::fs::metadata(&canonical) {
            Ok(m) => m,
            Err(e) => {
                log::debug!(
                    "list_allowlisted_workflow_basenames: metadata for {:?} failed: {}",
                    canonical,
                    e
                );
                continue;
            }
        };
        if meta.is_file() {
            out.push((*name).to_string());
        }
    }

    log::info!(
        "list_allowlisted_workflow_basenames: returning {} workflow file(s) for {:?}",
        out.len(),
        session_root
    );
    Ok(out)
}

/// Reads an allowlisted workflow file as UTF-8 text. Basename is validated against the allowlist
/// and path traversal; the resolved path must remain under the canonical session directory.
pub fn read_allowlisted_workflow_file_utf8(
    session_dir: &Path,
    basename: &str,
) -> Result<String, Status> {
    log::debug!(
        "read_allowlisted_workflow_file_utf8: session_dir={:?} basename={:?}",
        session_dir.display(),
        basename
    );
    validate_workflow_basename(basename)?;

    if !session_dir.is_dir() {
        return Err(Status::failed_precondition(
            "session directory does not exist or is not a directory",
        ));
    }

    let session_root = session_dir.canonicalize().map_err(|e| {
        log::debug!(
            "read_allowlisted_workflow_file_utf8: canonicalize session_dir failed: {}",
            e
        );
        Status::failed_precondition("session directory is not accessible")
    })?;

    let joined = session_dir.join(basename);
    let canonical_file = joined.canonicalize().map_err(|e| {
        log::debug!(
            "read_allowlisted_workflow_file_utf8: canonicalize {:?} failed: {}",
            joined,
            e
        );
        Status::not_found("workflow file not found")
    })?;

    if !canonical_file.starts_with(&session_root) {
        log::warn!(
            "read_allowlisted_workflow_file_utf8: rejected path outside session: {:?} (session root {:?})",
            canonical_file,
            session_root
        );
        return Err(Status::permission_denied(
            "resolved path escapes session directory",
        ));
    }

    let meta = std::fs::metadata(&canonical_file).map_err(|e| {
        log::debug!(
            "read_allowlisted_workflow_file_utf8: metadata failed: {}",
            e
        );
        Status::not_found("workflow file not found")
    })?;
    if !meta.is_file() {
        return Err(Status::failed_precondition("not a regular file"));
    }

    let content = std::fs::read_to_string(&canonical_file).map_err(|e| {
        log::error!(
            "read_allowlisted_workflow_file_utf8: read_to_string {:?} failed: {}",
            canonical_file,
            e
        );
        Status::internal(format!("failed to read workflow file: {}", e))
    })?;

    log::info!(
        "read_allowlisted_workflow_file_utf8: read {} UTF-8 chars from {:?}",
        content.len(),
        basename
    );
    Ok(content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn list_allowlisted_workflow_basenames_includes_allowlisted_fixture_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("changeset.yaml"), "g: 1\n").unwrap();
        fs::write(dir.path().join(".env"), "SECRET=x").unwrap();
        let basenames = list_allowlisted_workflow_basenames(dir.path()).expect("expected Ok");
        assert!(
            basenames.iter().any(|b| b == "changeset.yaml"),
            "must list changeset.yaml when present"
        );
        assert!(
            !basenames.iter().any(|b| b == ".env"),
            "must not list sensitive basenames"
        );
    }

    #[test]
    fn read_allowlisted_workflow_file_utf8_returns_exact_bytes_for_changeset_yaml() {
        let dir = tempfile::tempdir().unwrap();
        let golden = "root: from-unit-test\n";
        fs::write(dir.path().join("changeset.yaml"), golden).unwrap();
        let got = read_allowlisted_workflow_file_utf8(dir.path(), "changeset.yaml").unwrap();
        assert_eq!(got, golden);
    }
}
