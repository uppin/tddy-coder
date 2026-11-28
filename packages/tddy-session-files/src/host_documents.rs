//! `ConnectionService.ReadHostDocument` resolver — fetch the bytes of a document that already
//! exists on a connected host, for materializing a `HostDocumentRef` during `StartSession`.
//!
//! The owning daemon resolves the scope root under **its own** `os_user` mapping (the referencing
//! client's host grants no access) and refuses a `relative_path` that escapes. Binary (attachments
//! may be images/PDFs), so it does not reuse the UTF-8 readers (`ReadSessionWorkflowFile`,
//! `ReadWorktreeFile`). A file over [`MAX_HOST_DOCUMENT_BYTES`] is refused by the unary read with
//! `INVALID_ARGUMENT` rather than truncated — a truncated attachment is useless. Anything larger
//! goes through `StreamReadHostDocument`, which shares [`resolve_host_document`] (and therefore
//! every guard) and applies the host's configured attachment cap instead.
//!
//! Product contract: `docs/ft/coder/session-attachments.md` + the amendment
//! `docs/ft/coder/session-attachments.md` § Start-session materialization.

use std::path::{Component, Path, PathBuf};

use tddy_core::read_session_metadata;
use tddy_core::session_lifecycle::{unified_session_dir_path, validate_session_id_segment};
use tddy_daemon_kernel::HOST_DOCUMENT_FRAME_BYTES;
use tddy_rpc::Status;
use tddy_service::proto::types::HostDocumentScope;

use crate::session_file_upload::{contained_canonical_dir_on, validate_segment};
use tddy_daemon_kernel::user_paths::sessions_base_for_user;
use tddy_worktree_service::project_storage;
use tddy_worktree_service::worktree_files::git_listed_files;

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

/// The one refusal for the proto3 zero value, shared by the path gate and the root resolver so
/// there is a single sentence for it.
const UNSPECIFIED_SCOPE_ERR: &str = "host document scope must be specified";

/// The shape rules every scope shares: a non-empty, relative path with no `.` or `..` segment.
///
/// Private because a shape check alone is not the gate — [`validate_relative_path`] is, and it adds
/// the per-scope rules. Exporting this half on its own is how a caller ends up holding the lenient
/// spelling.
fn validate_path_shape(relative_path: &str) -> Result<(), Status> {
    if relative_path.is_empty() {
        return Err(Status::invalid_argument("relative_path must not be empty"));
    }
    if relative_path.starts_with('/') || relative_path.starts_with('\\') {
        return Err(Status::invalid_argument("relative_path must be relative"));
    }
    // The walk reads the path with backslashes already taken as separators, the way every lookup
    // below reads it (`relative_path.replace('\\', "/")`). On Unix `..\secret` is one legal
    // filename rather than a traversal, so checking the raw form here and the slashed form at the
    // join is how a request gets refused for one reason while being looked up as something else
    // entirely — the rule `validate_rel_path_shape` states for the worktree reads.
    let slashed = relative_path.replace('\\', "/");
    for comp in Path::new(&slashed).components() {
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

/// Refuse a relative path that could escape its scope's root, and a scope that names no root.
///
/// This is the **filesystem-independent** half of the check — the half [`resolve_host_document`]
/// applies before anything is read — and it is deliberately separate from containment: a refusal
/// must not depend on whether a file happens to exist, so the syntactic guard runs on its own and
/// canonicalize-and-contain ([`contained_in_scope_root`]) runs afterwards, against the resolved
/// scope root. Two spellings of "is this inside the root" is how one of them ends up being the
/// lenient one, and this particular check decides whether a caller can read an arbitrary file as
/// the session's OS user — so this is the only spelling, and the served path calls it.
///
/// An unspecified scope is refused rather than defaulted. It is proto3's zero value, so an
/// uninitialised or forward-incompatible request arrives carrying it, and every scope resolves to a
/// *different* root — picking one would read a file from the wrong place and answer as if that were
/// what was asked for.
///
/// The per-scope rules, each stated once:
///
/// * `SESSION_UPLOAD` and `STAGED_ATTACHMENT` address a file as exactly `<id>/<file_name>`, both
///   segments pure basenames — [`validate_two_segment_relative_path`], because both segments are
///   untrusted client input that become path components.
/// * `SESSION_ARTIFACT` and `PROJECT_REPO` may name a nested path, but its last segment is a file
///   name rather than something [`validate_segment`] would refuse.
/// * `SESSION_WORKTREE` has one more gate — the path must be surfaced by the repo's git listing —
///   and it needs the resolved root, so it belongs to the filesystem-dependent half and lives in
///   [`resolve_host_document`] beside the root it reads.
pub fn validate_relative_path(scope: HostDocumentScope, relative_path: &str) -> Result<(), Status> {
    if scope == HostDocumentScope::Unspecified {
        return Err(Status::invalid_argument(UNSPECIFIED_SCOPE_ERR));
    }
    validate_path_shape(relative_path)?;
    match scope {
        HostDocumentScope::SessionUpload => {
            validate_two_segment_relative_path(relative_path, "session upload", "upload_id")
        }
        HostDocumentScope::StagedAttachment => {
            validate_staged_attachment_relative_path(relative_path)
        }
        HostDocumentScope::WorkQueueFile => {
            validate_two_segment_relative_path(relative_path, "work queue file", "item_id")
        }
        HostDocumentScope::SessionWorktree => Ok(()),
        HostDocumentScope::SessionArtifact | HostDocumentScope::ProjectRepo => {
            let basename = Path::new(relative_path)
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| Status::invalid_argument("relative_path must be a basename"))?;
            validate_segment(basename)?;
            Ok(())
        }
        // Refused above; listed rather than folded into a catch-all so a scope added to the proto
        // has to name its own rule here instead of inheriting one.
        HostDocumentScope::Unspecified => Err(Status::invalid_argument(UNSPECIFIED_SCOPE_ERR)),
    }
}

/// Refuse a path that leaves `scope_root` once every symlink on it has been followed, and resolve
/// it to the canonical file the read will open.
///
/// The filesystem-dependent half of the gate [`validate_relative_path`] opens, applied *after* it
/// and only by way of it. It reuses [`contained_canonical_dir_on`](crate::session_file_upload::contained_canonical_dir_on) — the guard the upload writer, the
/// upload delete, the staging writer and the staged delete all share — rather than spelling
/// containment a fifth time: `.claude/creds -> ../../.env` canonicalizes to a path outside the root
/// while passing every syntactic check there is.
///
/// The *file name* is canonicalized too, not just its parent: [`std::fs::read`] follows symlinks,
/// so a lexical containment check on the (already-canonical) parent is not enough — the last
/// segment may itself be a link out of the root.
///
/// Nothing is created or written here; a path whose parent or file is absent is `NOT_FOUND`.
pub fn contained_in_scope_root(scope_root: &Path, relative_path: &str) -> Result<PathBuf, Status> {
    contained_in_scope_root_on(&tddy_fs::DiskFs, scope_root, relative_path)
}

/// [`contained_in_scope_root`] through the filesystem port.
pub fn contained_in_scope_root_on(
    fs: &dyn tddy_fs::Fs,
    scope_root: &Path,
    relative_path: &str,
) -> Result<PathBuf, Status> {
    if !fs.exists(scope_root) {
        return Err(Status::not_found("host document not found"));
    }
    let joined = scope_root.join(relative_path.replace('\\', "/"));
    let canonical_root = fs.canonicalize(scope_root).map_err(|e| {
        log::error!(
            "contained_in_scope_root: canonicalize scope root {:?} failed: {e}",
            scope_root
        );
        Status::internal(format!("failed to resolve scope root: {e}"))
    })?;
    let parent = joined
        .parent()
        .ok_or_else(|| Status::invalid_argument("relative_path must name a file"))?;
    if !fs.exists(parent) {
        return Err(Status::not_found("host document not found"));
    }
    let canonical_parent = contained_canonical_dir_on(fs, scope_root, parent)?;
    if !canonical_parent.starts_with(&canonical_root) {
        return Err(Status::invalid_argument("relative_path escapes scope root"));
    }

    let file_name = joined
        .file_name()
        .ok_or_else(|| Status::invalid_argument("relative_path must name a file"))?;
    let target = canonical_parent.join(file_name);
    // `metadata_follow`, as before: `is_file` follows a link.
    if !matches!(
        fs.metadata_follow(&target),
        Ok(m) if m.kind == tddy_fs::FsKind::File
    ) {
        return Err(Status::not_found("host document not found"));
    }
    let canonical_file = fs.canonicalize(&target).map_err(|e| {
        log::error!(
            "contained_in_scope_root: canonicalize {:?} failed: {e}",
            target
        );
        Status::internal(format!("failed to resolve host document: {e}"))
    })?;
    if !canonical_file.starts_with(&canonical_root) {
        return Err(Status::invalid_argument("relative_path escapes scope root"));
    }
    Ok(canonical_file)
}

/// The two-basename address shape both `HOST_DOCUMENT_SCOPE_SESSION_UPLOAD` and
/// `HOST_DOCUMENT_SCOPE_STAGED_ATTACHMENT` use: exactly `<id>/<file_name>`.
///
/// Both segments are untrusted client input that become path components, so each must be a pure
/// basename — the batch or drop is not a directory the caller may descend into or climb out of. The
/// basename rule is [`validate_segment`]'s, reused rather than restated, so a host-document fetch
/// cannot be a weaker gate than the upload that wrote the file.
///
/// `addressed` and `id_field` spell the scope into the refusal — "session upload" /
/// `<upload_id>`, "staged attachment" / `<staging_id>` — because the two scopes resolve to
/// different roots, and a caller that got the shape wrong needs to know which one it was
/// addressing and what the first segment should have named.
pub fn validate_two_segment_relative_path(
    relative_path: &str,
    addressed: &str,
    id_field: &str,
) -> Result<(), Status> {
    validate_path_shape(relative_path)?;
    let parts: Vec<&str> = relative_path.split('/').collect();
    if parts.len() != 2 {
        return Err(Status::invalid_argument(format!(
            "{addressed} relative_path must be <{id_field}>/<file_name>"
        )));
    }
    validate_segment(parts[0])?;
    validate_segment(parts[1])?;
    Ok(())
}

/// The `HOST_DOCUMENT_SCOPE_STAGED_ATTACHMENT` address shape: exactly `<staging_id>/<file_name>`,
/// the same two-segment form `SESSION_UPLOAD` uses.
pub fn validate_staged_attachment_relative_path(relative_path: &str) -> Result<(), Status> {
    validate_two_segment_relative_path(relative_path, "staged attachment", "staging_id")
}

fn resolve_scope_root(
    os_user: &str,
    tddy_data_dir: &Path,
    staging_base_dir: &Path,
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
        HostDocumentScope::StagedAttachment => Ok(
            crate::session_attachment_staging::staging_root_for(os_user, staging_base_dir),
        ),
        HostDocumentScope::WorkQueueFile => {
            validate_session_id_segment(session_id)
                .map_err(|e| Status::invalid_argument(e.message()))?;
            Ok(tddy_workflow::work_queue_files_root(
                &unified_session_dir_path(&sessions_base, session_id),
            ))
        }
        HostDocumentScope::Unspecified => Err(Status::invalid_argument(UNSPECIFIED_SCOPE_ERR)),
    }
}

/// A host document resolved to a real file on disk: its canonical path and on-disk size, with
/// every scope, containment and completeness guard already applied. Split out from the byte read
/// so the streaming reader applies its own (larger) cap without duplicating a single guard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedHostDocument {
    pub path: PathBuf,
    pub byte_size: u64,
}

/// Resolves a `HostDocumentRef` against the caller's `os_user` roots, without reading any bytes.
/// `relative_path` is POSIX-separated, no `.`/`..`, not absolute, and canonicalize-and-contained
/// under the resolved scope root. The owning daemon performs the resolution under its own
/// `os_user` mapping; the referencing client's host grants no access.
///
/// Both halves of the gate are the crate's exported ones — [`validate_relative_path`] and
/// [`contained_in_scope_root`] — rather than a second copy inlined here, so a caller reaching for
/// the crate's path gate gets the check this path runs. The one rule that cannot live in either is
/// `SESSION_WORKTREE`'s git listing, which needs the root the resolver just resolved.
pub fn resolve_host_document(
    os_user: &str,
    tddy_data_dir: &Path,
    staging_base_dir: &Path,
    scope: HostDocumentScope,
    session_id: &str,
    project_id: &str,
    relative_path: &str,
) -> Result<ResolvedHostDocument, Status> {
    resolve_host_document_on(
        &tddy_fs::DiskFs,
        os_user,
        tddy_data_dir,
        staging_base_dir,
        scope,
        session_id,
        project_id,
        relative_path,
    )
}

/// [`resolve_host_document`] through the filesystem port.
#[allow(clippy::too_many_arguments)]
pub fn resolve_host_document_on(
    fs: &dyn tddy_fs::Fs,
    os_user: &str,
    tddy_data_dir: &Path,
    staging_base_dir: &Path,
    scope: HostDocumentScope,
    session_id: &str,
    project_id: &str,
    relative_path: &str,
) -> Result<ResolvedHostDocument, Status> {
    let scope_root = resolve_scope_root(
        os_user,
        tddy_data_dir,
        staging_base_dir,
        scope,
        session_id,
        project_id,
    )?;

    validate_relative_path(scope, relative_path)?;

    if scope == HostDocumentScope::SessionWorktree {
        let rel_slashed = relative_path.replace('\\', "/");
        let files = git_listed_files(&scope_root)?;
        if !files.iter().any(|f| f == &rel_slashed) {
            log::warn!(
                "resolve_host_document: rejected path not surfaced by listing: {:?}",
                relative_path
            );
            return Err(Status::permission_denied(
                "file is not a listed worktree file",
            ));
        }
    }

    let canonical_file = contained_in_scope_root_on(fs, &scope_root, relative_path)?;

    // A staged file is only whole once its uploader wrote the final chunk and dropped the
    // completeness marker beside it. Refuse an in-progress or aborted upload here — the owning
    // host is the only party that can tell truncated bytes from a short document, so a fetch
    // must not be able to hand a caller half a file that reads as a whole one. The marker is
    // looked for beside the file the read will actually open, which is what the resolution above
    // returns.
    if scope == HostDocumentScope::StagedAttachment {
        let (dir, file_name) = canonical_file
            .parent()
            .zip(canonical_file.file_name())
            .ok_or_else(|| Status::invalid_argument("relative_path must name a file"))?;
        let marker = crate::session_attachment_staging::staged_complete_marker(
            dir,
            &file_name.to_string_lossy(),
        );
        if !fs.exists(&marker) {
            return Err(Status::failed_precondition(
                "staged attachment upload is not complete",
            ));
        }
    }

    // The size comes from the metadata, so an oversized file is refused by the caller's cap
    // without ever loading its contents into memory.
    let byte_size = fs
        .metadata_follow(&canonical_file)
        .map_err(|e| {
            log::error!(
                "resolve_host_document: metadata {:?} failed: {e}",
                canonical_file
            );
            Status::internal(format!("failed to read host document metadata: {e}"))
        })?
        .len;

    Ok(ResolvedHostDocument {
        path: canonical_file,
        byte_size,
    })
}

/// Resolves a `HostDocumentRef` and reads its bytes, capped at [`MAX_HOST_DOCUMENT_BYTES`] — the
/// unary transport's message-size budget. A file over the cap is refused rather than truncated;
/// `stream_read_host_document` is the path for anything larger.
pub fn read_host_document_bytes(
    os_user: &str,
    tddy_data_dir: &Path,
    staging_base_dir: &Path,
    scope: HostDocumentScope,
    session_id: &str,
    project_id: &str,
    relative_path: &str,
) -> Result<HostDocumentBytes, Status> {
    read_host_document_bytes_on(
        &tddy_fs::DiskFs,
        os_user,
        tddy_data_dir,
        staging_base_dir,
        scope,
        session_id,
        project_id,
        relative_path,
    )
}

/// [`read_host_document_bytes`] through the filesystem port.
#[allow(clippy::too_many_arguments)]
pub fn read_host_document_bytes_on(
    fs: &dyn tddy_fs::Fs,
    os_user: &str,
    tddy_data_dir: &Path,
    staging_base_dir: &Path,
    scope: HostDocumentScope,
    session_id: &str,
    project_id: &str,
    relative_path: &str,
) -> Result<HostDocumentBytes, Status> {
    let resolved = resolve_host_document_on(
        fs,
        os_user,
        tddy_data_dir,
        staging_base_dir,
        scope,
        session_id,
        project_id,
        relative_path,
    )?;
    if resolved.byte_size > MAX_HOST_DOCUMENT_BYTES as u64 {
        return Err(Status::invalid_argument(format!(
            "host document exceeds maximum size of {MAX_HOST_DOCUMENT_BYTES} bytes"
        )));
    }

    let data = fs.read(&resolved.path).map_err(|e| {
        log::error!(
            "read_host_document_bytes: read {:?} failed: {e}",
            resolved.path
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

/// Reads `path` in [`HOST_DOCUMENT_FRAME_BYTES`] slices into `tx`, stamping `total_byte_size` on
/// every frame. A zero-byte document still yields exactly one (empty) frame, so a consumer never
/// has to tell "empty document" from "stream produced nothing". A read error terminates the stream
/// with a status rather than closing it, so a partial document is never mistaken for a whole one.
///
/// `frame` builds the wire message from one slice and the total, so the *slicing* — the part a
/// consumer's boundaries depend on — has one implementation while `session_files.HostDocumentChunk`
/// and `connection.proto`'s surviving copy of it stay two types. Two loops, each free to pick its
/// own slice size, is how a consumer switching between the streaming and unary reads would start
/// seeing different boundaries.
pub fn stream_document_frames<T>(
    path: &Path,
    total_byte_size: u64,
    frame: impl Fn(Vec<u8>, u64) -> T,
    tx: &tokio::sync::mpsc::UnboundedSender<Result<T, Status>>,
) {
    stream_document_frames_on(&tddy_fs::DiskFs, path, total_byte_size, frame, tx)
}

/// [`stream_document_frames`] through the filesystem port.
///
/// The port has no streaming read, so the document is read whole and then sliced into
/// [`HOST_DOCUMENT_FRAME_BYTES`] frames — the slicing, which is what a consumer's boundaries
/// depend on, is unchanged, and a zero-byte document still yields exactly one (empty) frame.
/// The in-memory cost is bounded by the caller's cap: the unary path refuses over-cap documents
/// before calling, and the stream path resolves the size first.
pub fn stream_document_frames_on<T>(
    fs: &dyn tddy_fs::Fs,
    path: &Path,
    total_byte_size: u64,
    frame: impl Fn(Vec<u8>, u64) -> T,
    tx: &tokio::sync::mpsc::UnboundedSender<Result<T, Status>>,
) {
    let data = match fs.read(path) {
        Ok(d) => d,
        Err(e) => {
            log::error!("stream_read_host_document: read {path:?} failed: {e}");
            let _ = tx.send(Err(Status::internal(format!(
                "failed to read host document: {e}"
            ))));
            return;
        }
    };

    let mut chunks = data.chunks(HOST_DOCUMENT_FRAME_BYTES);
    match chunks.next() {
        None => {
            let _ = tx.send(Ok(frame(Vec::new(), total_byte_size)));
        }
        Some(first) => {
            if tx.send(Ok(frame(first.to_vec(), total_byte_size))).is_err() {
                return;
            }
            for chunk in chunks {
                if tx.send(Ok(frame(chunk.to_vec(), total_byte_size))).is_err() {
                    return;
                }
            }
        }
    }
}

#[cfg(all(test, unix))]
#[path = "host_document_tests.rs"]
mod host_document_tests;
