//! `ConnectionService.ReadHostDocument` resolver — fetch the bytes of a document that already
//! exists on a connected host, for materializing a `HostDocumentRef` during `StartSession`.
//!
//! The owning daemon resolves the scope root under **its own** `os_user` mapping (the referencing
//! client's host grants no access) and refuses a `relative_path` that escapes. Unary (not
//! streaming) so the RPC is forwardable over the LiveKit common room; binary (attachments may be
//! images/PDFs), so it does not reuse the UTF-8 readers (`ReadSessionWorkflowFile`,
//! `ReadWorktreeFile`). A file over [`MAX_HOST_DOCUMENT_BYTES`] is refused with
//! `INVALID_ARGUMENT` rather than truncated — a truncated attachment is useless, and staging
//! exists for larger docs (chunked, no single-message limit).
//!
//! Product contract: `docs/ft/coder/session-attachments.md` + the amendment
//! `docs/ft/coder/session-attachments.md` § Start-session materialization.

use std::path::{Component, Path, PathBuf};

use tddy_core::read_session_metadata;
use tddy_core::session_lifecycle::{unified_session_dir_path, validate_session_id_segment};
use tddy_rpc::Status;
use tddy_service::proto::connection::HostDocumentScope;

use crate::project_storage;
use crate::session_file_upload::{contained_canonical_dir, validate_segment};
use crate::user_sessions_path::sessions_base_for_user;
use crate::worktree_files::git_listed_files;

/// Hard cap on a single `ReadHostDocument` response. Matches gRPC's default max message size so
/// the unary response stays within transport limits. Larger documents must be staged (chunked,
/// no single-message limit) rather than fetched via `HostDocumentRef`.
pub const MAX_HOST_DOCUMENT_BYTES: usize = 4 * 1024 * 1024;

/// The bytes of a resolved host document. `byte_size` equals `data.len()` — an over-cap file is
/// refused, not truncated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostDocumentBytes {
    pub data: Vec<u8>,
    pub byte_size: u64,
}

fn validate_relative_path(relative_path: &str) -> Result<(), Status> {
    if relative_path.is_empty() {
        return Err(Status::invalid_argument("relative_path must not be empty"));
    }
    if relative_path.starts_with('/') || relative_path.starts_with('\\') {
        return Err(Status::invalid_argument("relative_path must be relative"));
    }
    for comp in Path::new(relative_path).components() {
        match comp {
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(Status::invalid_argument("relative_path must not traverse"));
            }
            Component::CurDir => {
                return Err(Status::invalid_argument(
                    "relative_path must not contain '.' segments",
                ));
            }
            Component::Normal(_) => {}
        }
    }
    Ok(())
}

fn validate_session_upload_relative_path(relative_path: &str) -> Result<(), Status> {
    validate_relative_path(relative_path)?;
    let parts: Vec<&str> = relative_path.split('/').collect();
    if parts.len() != 2 {
        return Err(Status::invalid_argument(
            "session upload relative_path must be <upload_id>/<file_name>",
        ));
    }
    validate_segment(parts[0])?;
    validate_segment(parts[1])?;
    Ok(())
}

fn resolve_scope_root(
    os_user: &str,
    tddy_data_dir: &Path,
    scope: HostDocumentScope,
    session_id: &str,
    project_id: &str,
) -> Result<PathBuf, Status> {
    let sessions_base = sessions_base_for_user(os_user, Some(tddy_data_dir))
        .ok_or_else(|| Status::internal("could not resolve sessions path"))?;

    match scope {
        HostDocumentScope::SessionArtifact => {
            validate_session_id_segment(session_id)
                .map_err(|e| Status::invalid_argument(e.message()))?;
            Ok(unified_session_dir_path(&sessions_base, session_id).join("artifacts"))
        }
        HostDocumentScope::SessionUpload => {
            validate_session_id_segment(session_id)
                .map_err(|e| Status::invalid_argument(e.message()))?;
            Ok(unified_session_dir_path(&sessions_base, session_id).join("uploads"))
        }
        HostDocumentScope::SessionWorktree => {
            validate_session_id_segment(session_id)
                .map_err(|e| Status::invalid_argument(e.message()))?;
            let session_dir = unified_session_dir_path(&sessions_base, session_id);
            let meta = read_session_metadata(&session_dir).map_err(|e| {
                log::warn!("read_host_document_bytes: session metadata missing: {e}");
                Status::not_found("session metadata not found")
            })?;
            let repo_path = meta
                .repo_path
                .ok_or_else(|| Status::failed_precondition("session has no worktree repo_path"))?;
            Ok(PathBuf::from(repo_path))
        }
        HostDocumentScope::ProjectRepo => {
            let project_id = project_id.trim();
            if project_id.is_empty() {
                return Err(Status::invalid_argument(
                    "project_id is required for project repo scope",
                ));
            }
            let projects_dir = tddy_data_dir.join("projects");
            let project = project_storage::find_project(&projects_dir, project_id)
                .map_err(|e| Status::internal(e.to_string()))?
                .ok_or_else(|| Status::not_found("project not found"))?;
            Ok(PathBuf::from(project.main_repo_path))
        }
        HostDocumentScope::Unspecified => Err(Status::invalid_argument(
            "host document scope must be specified",
        )),
    }
}

/// Resolves a `HostDocumentRef` against the caller's `os_user` data root and reads the bytes.
/// `relative_path` is POSIX-separated, no `.`/`..`, not absolute, and canonicalize-and-contained
/// under the resolved scope root. The owning daemon performs the read under its own `os_user`
/// mapping; the referencing client's host grants no access.
pub fn read_host_document_bytes(
    os_user: &str,
    tddy_data_dir: &Path,
    scope: HostDocumentScope,
    session_id: &str,
    project_id: &str,
    relative_path: &str,
) -> Result<HostDocumentBytes, Status> {
    let scope_root = resolve_scope_root(os_user, tddy_data_dir, scope, session_id, project_id)?;

    if scope == HostDocumentScope::SessionUpload {
        validate_session_upload_relative_path(relative_path)?;
    } else if scope == HostDocumentScope::SessionWorktree {
        validate_relative_path(relative_path)?;
        let rel_slashed = relative_path.replace('\\', "/");
        let files = git_listed_files(&scope_root)?;
        if !files.iter().any(|f| f == &rel_slashed) {
            log::warn!(
                "read_host_document_bytes: rejected path not surfaced by listing: {:?}",
                relative_path
            );
            return Err(Status::permission_denied(
                "file is not a listed worktree file",
            ));
        }
    } else {
        validate_relative_path(relative_path)?;
        let basename = Path::new(relative_path)
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| Status::invalid_argument("relative_path must be a basename"))?;
        validate_segment(basename)?;
    }

    if !scope_root.exists() {
        return Err(Status::not_found("host document not found"));
    }

    let joined = scope_root.join(relative_path.replace('\\', "/"));
    let canonical_root = scope_root.canonicalize().map_err(|e| {
        log::error!(
            "read_host_document_bytes: canonicalize scope root {:?} failed: {e}",
            scope_root
        );
        Status::internal(format!("failed to resolve scope root: {e}"))
    })?;
    let parent = joined
        .parent()
        .ok_or_else(|| Status::invalid_argument("relative_path must name a file"))?;
    std::fs::create_dir_all(parent).ok();
    let canonical_parent = if parent.exists() {
        contained_canonical_dir(&scope_root, parent)?
    } else {
        return Err(Status::not_found("host document not found"));
    };
    if !canonical_parent.starts_with(&canonical_root) {
        return Err(Status::invalid_argument("relative_path escapes scope root"));
    }

    let file_name = joined
        .file_name()
        .ok_or_else(|| Status::invalid_argument("relative_path must name a file"))?;
    let target = canonical_parent.join(file_name);
    if !target.is_file() {
        return Err(Status::not_found("host document not found"));
    }
    // Canonicalize the full file path so a symlinked file inside the scope root cannot
    // escape: `std::fs::read` follows symlinks, so a lexical containment check on the
    // (already-canonical) parent is not enough — the file name itself may be a link.
    let canonical_file = target.canonicalize().map_err(|e| {
        log::error!(
            "read_host_document_bytes: canonicalize {:?} failed: {e}",
            target
        );
        Status::internal(format!("failed to resolve host document: {e}"))
    })?;
    if !canonical_file.starts_with(&canonical_root) {
        return Err(Status::invalid_argument("relative_path escapes scope root"));
    }

    // Check the size on disk before reading, so an oversized file is refused without
    // loading its full contents into memory.
    let file_size = std::fs::metadata(&canonical_file)
        .map_err(|e| {
            log::error!(
                "read_host_document_bytes: metadata {:?} failed: {e}",
                canonical_file
            );
            Status::internal(format!("failed to read host document metadata: {e}"))
        })?
        .len();
    if file_size > MAX_HOST_DOCUMENT_BYTES as u64 {
        return Err(Status::invalid_argument(format!(
            "host document exceeds maximum size of {MAX_HOST_DOCUMENT_BYTES} bytes"
        )));
    }

    let data = std::fs::read(&canonical_file).map_err(|e| {
        log::error!(
            "read_host_document_bytes: read {:?} failed: {e}",
            canonical_file
        );
        Status::internal(format!("failed to read host document: {e}"))
    })?;
    // Defense in depth: the on-disk size was checked above, but re-check the in-memory
    // length in case the file changed between the metadata read and the content read.
    if data.len() > MAX_HOST_DOCUMENT_BYTES {
        return Err(Status::invalid_argument(format!(
            "host document exceeds maximum size of {MAX_HOST_DOCUMENT_BYTES} bytes"
        )));
    }
    let byte_size = data.len() as u64;
    Ok(HostDocumentBytes { data, byte_size })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use tddy_core::session_lifecycle::unified_session_dir_path;
    use tddy_core::session_metadata::SessionMetadata;
    use tddy_rpc::Code;

    const SESSION_ID: &str = "aaaaaaaa-aaaa-7aaa-8aaa-aaaaaaaaaaaa";
    const PROJECT_ID: &str = "host-doc-proj";

    fn caller() -> String {
        std::env::var("USER").expect("USER")
    }

    /// A data root + the owning session's directory, with `artifacts/` and `uploads/` ready.
    fn a_session_with_artifacts(
        artifact: Option<(&str, &[u8])>,
        upload: Option<(&str, &str, &[u8])>,
    ) -> (tempfile::TempDir, std::path::PathBuf) {
        let data = tempfile::tempdir().unwrap();
        let session_dir = unified_session_dir_path(data.path(), SESSION_ID);
        let artifacts = session_dir.join("artifacts");
        std::fs::create_dir_all(&artifacts).unwrap();
        if let Some((name, bytes)) = artifact {
            std::fs::write(artifacts.join(name), bytes).unwrap();
        }
        if let Some((upload_id, file_name, bytes)) = upload {
            let dir = session_dir.join("uploads").join(upload_id);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join(file_name), bytes).unwrap();
        }
        (data, session_dir)
    }

    fn write_session_metadata(session_dir: &std::path::Path, repo_path: Option<&str>) {
        let meta = SessionMetadata {
            session_id: SESSION_ID.to_string(),
            project_id: PROJECT_ID.to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            status: "running".to_string(),
            repo_path: repo_path.map(str::to_string),
            pid: None,
            tool: None,
            livekit_room: None,
            pending_elicitation: false,
            previous_session_id: None,
            session_type: None,
            model: None,
            activity_status: None,
            hook_token: None,
            sandbox: None,
            agent: None,
            recipe: None,
            specialized_agents: Vec::new(),
            cursor_chat_id: None,
        };
        tddy_core::write_session_metadata(session_dir, &meta).unwrap();
    }

    /// AC(hd-1) — SESSION_ARTIFACT resolves under the caller's os_user data root.
    #[test]
    fn read_host_document_resolves_session_artifact_scope_under_the_callers_os_user() {
        // Given — a session holding artifacts/PRD.md
        let (data, _session_dir) = a_session_with_artifacts(Some(("PRD.md", b"plan")), None);

        // When
        let doc = read_host_document_bytes(
            &caller(),
            data.path(),
            HostDocumentScope::SessionArtifact,
            SESSION_ID,
            "",
            "PRD.md",
        )
        .unwrap();

        // Then
        assert_eq!(doc.data, b"plan");
        assert_eq!(doc.byte_size, 4);
    }

    /// AC(hd-2) — SESSION_UPLOAD resolves with "<upload_id>/<file_name>".
    #[test]
    fn read_host_document_resolves_session_upload_scope_with_upload_id_slash_file_name() {
        // Given — a session holding uploads/u1/notes.txt
        let (data, _session_dir) =
            a_session_with_artifacts(None, Some(("u1", "notes.txt", b"notes")));

        // When
        let doc = read_host_document_bytes(
            &caller(),
            data.path(),
            HostDocumentScope::SessionUpload,
            SESSION_ID,
            "",
            "u1/notes.txt",
        )
        .unwrap();

        // Then
        assert_eq!(doc.data, b"notes");
    }

    /// AC(hd-3) — PROJECT_REPO resolves under the project's main_repo_path.
    #[test]
    fn read_host_document_resolves_project_repo_scope_under_the_projects_main_repo_path() {
        // Given — a project whose main_repo_path holds README.md
        let data = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        std::fs::write(repo.path().join("README.md"), b"repo doc").unwrap();
        crate::project_storage::write_projects(
            &data.path().join("projects"),
            &[crate::project_storage::ProjectData {
                project_id: PROJECT_ID.to_string(),
                name: "host-doc-proj".to_string(),
                git_url: String::new(),
                main_repo_path: repo.path().to_string_lossy().into_owned(),
                main_branch_ref: None,
                remote_name: None,
                host_repo_paths: std::collections::HashMap::new(),
            }],
        )
        .unwrap();

        // When
        let doc = read_host_document_bytes(
            &caller(),
            data.path(),
            HostDocumentScope::ProjectRepo,
            "",
            PROJECT_ID,
            "README.md",
        )
        .unwrap();

        // Then
        assert_eq!(doc.data, b"repo doc");
    }

    /// AC(hd-4) — a `relative_path` with `..` segments is refused.
    #[test]
    fn read_host_document_refuses_a_relative_path_with_dotdot_segments() {
        // Given — a session with an artifact
        let (data, _session_dir) = a_session_with_artifacts(Some(("PRD.md", b"x")), None);

        // When / Then
        let err = read_host_document_bytes(
            &caller(),
            data.path(),
            HostDocumentScope::SessionArtifact,
            SESSION_ID,
            "",
            "../outside.txt",
        )
        .unwrap_err();
        assert_eq!(err.code, Code::InvalidArgument);
    }

    /// AC(hd-5) — an absolute `relative_path` is refused.
    #[test]
    fn read_host_document_refuses_an_absolute_relative_path() {
        // Given
        let (data, _session_dir) = a_session_with_artifacts(Some(("PRD.md", b"x")), None);

        // When / Then
        let err = read_host_document_bytes(
            &caller(),
            data.path(),
            HostDocumentScope::SessionArtifact,
            SESSION_ID,
            "",
            "/etc/passwd",
        )
        .unwrap_err();
        assert_eq!(err.code, Code::InvalidArgument);
    }

    /// AC(hd-6) — a file over the cap is refused (not truncated).
    #[test]
    fn read_host_document_refuses_a_file_over_the_cap_without_truncating() {
        // Given — an artifact larger than the cap
        let (data, _session_dir) = a_session_with_artifacts(
            Some(("big.bin", &vec![b'x'; MAX_HOST_DOCUMENT_BYTES + 1])),
            None,
        );

        // When / Then
        let err = read_host_document_bytes(
            &caller(),
            data.path(),
            HostDocumentScope::SessionArtifact,
            SESSION_ID,
            "",
            "big.bin",
        )
        .unwrap_err();
        assert_eq!(err.code, Code::InvalidArgument);
    }

    /// AC(hd-7) — non-UTF-8 bytes survive verbatim.
    #[test]
    fn read_host_document_returns_bytes_verbatim_for_a_binary_file() {
        // Given — a binary (non-UTF-8) artifact
        let bin: Vec<u8> = vec![0xff, 0xfe, 0x00, 0x01, 0x80, 0x7f, 0xc3];
        let (data, _session_dir) = a_session_with_artifacts(Some(("img.bin", &bin)), None);

        // When
        let doc = read_host_document_bytes(
            &caller(),
            data.path(),
            HostDocumentScope::SessionArtifact,
            SESSION_ID,
            "",
            "img.bin",
        )
        .unwrap();

        // Then
        assert_eq!(doc.data, bin);
        assert_eq!(doc.byte_size, bin.len() as u64);
    }

    /// AC(hd-8) — a SESSION_WORKTREE path not surfaced by the git listing is refused.
    #[test]
    fn read_host_document_refuses_a_worktree_relative_path_not_surfaced_by_the_listing() {
        // Given — a git repo with a tracked file and a gitignored file
        let repo = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| {
            std::process::Command::new("git")
                .args(args)
                .current_dir(repo.path())
                .env("GIT_AUTHOR_NAME", "T")
                .env("GIT_AUTHOR_EMAIL", "t@t.com")
                .env("GIT_COMMITTER_NAME", "T")
                .env("GIT_COMMITTER_EMAIL", "t@t.com")
                .status()
                .expect("git");
        };
        run(&["init", "-q"]);
        run(&["config", "user.email", "t@t.com"]);
        run(&["config", "user.name", "T"]);
        std::fs::write(repo.path().join("listed.txt"), b"ok").unwrap();
        std::fs::write(repo.path().join(".gitignore"), "secret.txt\n").unwrap();
        std::fs::write(repo.path().join("secret.txt"), b"hidden").unwrap();
        run(&["add", "listed.txt", ".gitignore"]);
        run(&["commit", "-q", "-m", "init"]);

        // ... and a session whose repo_path is that worktree
        let (data, session_dir) = a_session_with_artifacts(None, None);
        write_session_metadata(&session_dir, Some(repo.path().to_str().unwrap()));

        // When / Then — the gitignored file is not surfaced by the listing, so it is refused
        let err = read_host_document_bytes(
            &caller(),
            data.path(),
            HostDocumentScope::SessionWorktree,
            SESSION_ID,
            "",
            "secret.txt",
        )
        .unwrap_err();
        assert_eq!(err.code, Code::PermissionDenied);
    }

    /// Regression — a symlinked file inside the artifacts scope root that points outside the
    /// root is refused. `std::fs::read` follows symlinks, so the full file path is canonicalized
    /// and re-checked against the canonical scope root (a lexical check on the parent is not
    /// enough — the file name itself may be a link).
    #[test]
    fn read_host_document_refuses_a_symlinked_file_escaping_the_scope_root() {
        // Given — a session and an outside file the symlink will target
        let (data, session_dir) = a_session_with_artifacts(None, None);
        let outside = tempfile::tempdir().unwrap();
        let outside_file = outside.path().join("secret.txt");
        std::fs::write(&outside_file, b"outside bytes").unwrap();
        let artifacts = session_dir.join("artifacts");
        std::os::unix::fs::symlink(&outside_file, artifacts.join("escape.md")).unwrap();

        // When / Then — the symlink resolves outside the artifacts root, so it is refused
        let err = read_host_document_bytes(
            &caller(),
            data.path(),
            HostDocumentScope::SessionArtifact,
            SESSION_ID,
            "",
            "escape.md",
        )
        .unwrap_err();
        assert_eq!(err.code, Code::InvalidArgument);
    }
}
