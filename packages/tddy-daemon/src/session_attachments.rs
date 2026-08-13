//! Storage for a session's **attachments** — user-attached documents kept under
//! `session_dir/artifacts/attachments/`, alongside (never inside) the recipe-owned planning
//! artifacts. Attachments are addressed by basename in a single flat level, so no client-supplied
//! value ever becomes more than one path segment.
//!
//! Writes go through [`copy_attachment_into_session`] (from a local file) or
//! [`write_attachment_bytes`] (from bytes already in memory). Both validate the basename with the
//! same `validate_segment` guard the uploads path uses, confirm the attachments directory resolves
//! inside the canonical `artifacts/` root, and create the target exclusively so an existing
//! attachment is never truncated. Product contract: `docs/ft/coder/session-attachments.md`.

use std::fs::{File, OpenOptions};
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};

use tddy_rpc::Status;
use tddy_workflow::{session_artifacts_root, session_attachments_root};

use crate::session_file_upload::{contained_canonical_dir, validate_segment};

/// One attachment file on disk under `artifacts/attachments/`: its `basename`, absolute `path`, and
/// size in bytes as reported by the filesystem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionAttachmentFile {
    pub basename: String,
    pub path: PathBuf,
    pub size_bytes: u64,
}

/// Lists the session's attachments, sorted by basename so a listing is deterministic across
/// filesystems. Regular files only — subdirectories and other non-regular entries are skipped. A
/// session with no attachments directory (the common case) yields an empty list, not an error.
pub fn list_session_attachments(session_dir: &Path) -> Vec<SessionAttachmentFile> {
    let attachments_dir = session_attachments_root(session_dir);
    let Ok(entries) = std::fs::read_dir(&attachments_dir) else {
        log::debug!(
            "list_session_attachments: no attachments directory at {} — empty listing",
            attachments_dir.display()
        );
        return Vec::new();
    };

    let mut files: Vec<SessionAttachmentFile> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = entry.metadata() else {
            log::warn!(
                "list_session_attachments: metadata unavailable for {} — skipping",
                path.display()
            );
            continue;
        };
        if !metadata.is_file() {
            log::debug!(
                "list_session_attachments: skipping non-regular entry {}",
                path.display()
            );
            continue;
        }
        let Some(basename) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
            continue;
        };
        files.push(SessionAttachmentFile {
            basename,
            path,
            size_bytes: metadata.len(),
        });
    }
    files.sort_by(|a, b| a.basename.cmp(&b.basename));

    log::debug!(
        "list_session_attachments: {} attachment(s) under {}",
        files.len(),
        attachments_dir.display()
    );
    files
}

/// Validates an attachment basename as a single safe path segment.
///
/// The rule is the uploads path's `validate_segment` — reused, not reimplemented — but its refusal
/// message is written for that surface's `upload_id` / `file_name` fields, which are not concepts in
/// the attachment API. A caller who sends a bad `SessionAttachment.basename` gets told about the
/// field it actually sent.
pub(crate) fn validate_attachment_basename(basename: &str) -> Result<&str, Status> {
    validate_segment(basename)
        .map_err(|_| Status::invalid_argument("attachment basename must be a single path segment"))
}

/// Confirms `source` is an existing regular file.
fn validate_attachment_source(source: &Path) -> Result<(), Status> {
    let source_metadata = std::fs::metadata(source).map_err(|e| {
        log::warn!(
            "copy_attachment_into_session: source {} is not accessible: {e}",
            source.display()
        );
        Status::invalid_argument("attachment source file does not exist")
    })?;
    if !source_metadata.is_file() {
        log::warn!(
            "copy_attachment_into_session: source {} is not a regular file",
            source.display()
        );
        return Err(Status::invalid_argument(
            "attachment source must be a regular file",
        ));
    }
    Ok(())
}

/// Creates the attachments directory on demand and confirms it resolves inside the canonical
/// `artifacts/` root.
fn ensure_contained_attachments_dir(session_dir: &Path) -> Result<PathBuf, Status> {
    let attachments_dir = session_attachments_root(session_dir);
    std::fs::create_dir_all(&attachments_dir).map_err(|e| {
        log::error!(
            "copy_attachment_into_session: create_dir_all {} failed: {e}",
            attachments_dir.display()
        );
        Status::internal(format!("failed to create attachments dir: {e}"))
    })?;

    // The artifacts root holds no untrusted component, so it is the trusted base the attachments
    // directory must stay within — this catches an `attachments` symlink pointing outside the
    // session tree.
    let artifacts_root = session_artifacts_root(session_dir);
    let canonical_attachments_dir = contained_canonical_dir(&artifacts_root, &attachments_dir)?;
    log::debug!(
        "copy_attachment_into_session: attachments dir {} is contained in {}",
        canonical_attachments_dir.display(),
        artifacts_root.display()
    );
    Ok(canonical_attachments_dir)
}

/// Best-effort removal of a partial attachment file after a failed exclusive create or write.
fn remove_partial_attachment(target: &Path) {
    if let Err(e) = std::fs::remove_file(target) {
        log::warn!(
            "session_attachments: failed to remove partial attachment {}: {e}",
            target.display()
        );
    }
}

/// Exclusively creates `canonical_dir/<safe_basename>`, returning the open file and its path.
///
/// `create_new` makes the existence check and the creation a single atomic step, so a concurrent
/// writer cannot slip in between them. An existing target — a regular file or a symlink planted in
/// the attachments directory — is refused with [`Status::failed_precondition`] rather than followed
/// or truncated. Shared by both write entry points so neither is a weaker gate than the other.
fn create_attachment_file_exclusively(
    canonical_dir: &Path,
    safe_basename: &str,
) -> Result<(File, PathBuf), Status> {
    let target = canonical_dir.join(safe_basename);

    let dest = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)
        .map_err(|e| {
            if e.kind() == io::ErrorKind::AlreadyExists {
                log::warn!(
                    "session_attachments: refusing to overwrite existing attachment {}",
                    target.display()
                );
                Status::failed_precondition("an attachment with this name already exists")
            } else {
                log::error!(
                    "session_attachments: create {} failed: {e}",
                    target.display()
                );
                Status::internal(format!("failed to store attachment: {e}"))
            }
        })?;

    Ok((dest, target))
}

/// Atomically creates `canonical_dir/<safe_basename>` and streams `source` into it.
///
/// On failure opening the source or during `io::copy`, the partial target is removed so a retry is
/// not blocked by an empty file left behind by `create_new`.
fn store_attachment_bytes_exclusively(
    canonical_dir: &Path,
    safe_basename: &str,
    source: &Path,
) -> Result<PathBuf, Status> {
    let (mut dest, target) = create_attachment_file_exclusively(canonical_dir, safe_basename)?;

    let mut source_file = match File::open(source) {
        Ok(file) => file,
        Err(e) => {
            log::error!(
                "copy_attachment_into_session: open source {} failed: {e}",
                source.display()
            );
            remove_partial_attachment(&target);
            return Err(Status::internal(format!("failed to store attachment: {e}")));
        }
    };

    let bytes_copied = match io::copy(&mut source_file, &mut dest) {
        Ok(bytes) => bytes,
        Err(e) => {
            log::error!(
                "copy_attachment_into_session: copy {} -> {} failed: {e}",
                source.display(),
                target.display()
            );
            remove_partial_attachment(&target);
            return Err(Status::internal(format!("failed to store attachment: {e}")));
        }
    };

    log::info!(
        "copy_attachment_into_session: stored {} ({bytes_copied} byte(s))",
        target.display()
    );
    Ok(target)
}

/// Copies `source` into `artifacts/attachments/<basename>`, returning the written path.
///
/// `basename` must be a single safe path segment (the uploads path's `validate_segment` guard) and
/// `source` must be an existing regular file; either violation is refused with
/// [`Status::invalid_argument`] and writes nothing. The attachments directory is created on demand
/// and then confirmed to resolve inside the canonical `artifacts/` root, so a symlinked
/// `attachments` pointing out of the session tree is refused rather than followed. An existing
/// target is never overwritten: a second copy under the same basename is refused with
/// [`Status::failed_precondition`], leaving the stored bytes untouched.
pub fn copy_attachment_into_session(
    session_dir: &Path,
    source: &Path,
    basename: &str,
) -> Result<PathBuf, Status> {
    log::debug!(
        "copy_attachment_into_session: session_dir={} source={} basename={basename:?}",
        session_dir.display(),
        source.display()
    );

    // Untrusted basename: it must not introduce a separator or `..` that would climb out of the
    // attachments directory.
    let safe_basename = validate_attachment_basename(basename)?;

    validate_attachment_source(source)?;
    let canonical_attachments_dir = ensure_contained_attachments_dir(session_dir)?;
    store_attachment_bytes_exclusively(&canonical_attachments_dir, safe_basename, source)
}

/// Writes `data` into `artifacts/attachments/<basename>`, returning the written path.
///
/// The in-memory counterpart of [`copy_attachment_into_session`], for bytes that have no local file
/// to copy from — a `HostDocumentRef` fetched from a peer daemon arrives as a byte buffer. Every
/// guarantee of the copy path holds here: the basename must be a single safe segment, the
/// attachments directory must resolve inside the canonical `artifacts/` root, the target is created
/// exclusively so an existing attachment is refused with [`Status::failed_precondition`] rather than
/// truncated, and a failed write removes the partial file so a retry is not blocked.
pub(crate) fn write_attachment_bytes(
    session_dir: &Path,
    basename: &str,
    data: &[u8],
) -> Result<PathBuf, Status> {
    log::debug!(
        "write_attachment_bytes: session_dir={} basename={basename:?} bytes={}",
        session_dir.display(),
        data.len()
    );

    // Untrusted basename: it must not introduce a separator or `..` that would climb out of the
    // attachments directory.
    let safe_basename = validate_attachment_basename(basename)?;

    let canonical_attachments_dir = ensure_contained_attachments_dir(session_dir)?;
    let (mut dest, target) =
        create_attachment_file_exclusively(&canonical_attachments_dir, safe_basename)?;

    if let Err(e) = dest.write_all(data) {
        log::error!(
            "write_attachment_bytes: write {} failed: {e}",
            target.display()
        );
        remove_partial_attachment(&target);
        return Err(Status::internal(format!("failed to store attachment: {e}")));
    }

    log::info!(
        "write_attachment_bytes: stored {} ({} byte(s))",
        target.display(),
        data.len()
    );
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::{copy_attachment_into_session, list_session_attachments, SessionAttachmentFile};

    use std::fs;
    use std::path::{Path, PathBuf};

    use tddy_rpc::{Code, Status};
    use tddy_workflow::session_attachments_root;
    use tempfile::TempDir;

    // ---- fluent helpers -------------------------------------------------------------------

    /// A session directory plus a separate directory holding the files an attachment is copied from,
    /// so a source never has to live inside the session tree it is attached to.
    struct SessionUnderTest {
        /// Held for its drop guard only — every path the fixture hands out comes from
        /// `session_path`.
        _session: TempDir,
        /// The session directory as the writers under test report it. Both writers resolve their
        /// target through `contained_canonical_dir`, so the path they return has every symlink
        /// resolved. On macOS `TempDir` hands back `/tmp/...`, and `/tmp` is itself a symlink to
        /// `/private/tmp` — resolving it once here is what lets the tests compare like with like
        /// instead of asserting on which of the two spellings the fixture happened to produce.
        session_path: PathBuf,
        sources: TempDir,
    }

    /// A session that already has its `artifacts/` root.
    fn a_session() -> SessionUnderTest {
        let session = a_session_without_an_artifacts_dir();
        fs::create_dir_all(session.path().join("artifacts")).expect("create artifacts dir");
        session
    }

    /// A brand-new session directory with nothing in it yet.
    fn a_session_without_an_artifacts_dir() -> SessionUnderTest {
        let session = tempfile::tempdir().expect("create session dir");
        let session_path = fs::canonicalize(session.path()).expect("canonicalize session dir");
        SessionUnderTest {
            _session: session,
            session_path,
            sources: tempfile::tempdir().expect("create sources dir"),
        }
    }

    impl SessionUnderTest {
        fn path(&self) -> &Path {
            &self.session_path
        }

        fn attachments_dir(&self) -> PathBuf {
            session_attachments_root(self.path())
        }

        /// Writes a file to attach and returns its path.
        fn a_source_file(&self, name: &str, contents: &[u8]) -> PathBuf {
            let path = self.sources.path().join(name);
            fs::write(&path, contents).expect("write source file");
            path
        }

        /// A path in the sources directory that was never created.
        fn a_missing_source_path(&self, name: &str) -> PathBuf {
            self.sources.path().join(name)
        }

        /// A directory in the sources directory, for the "source is not a file" case.
        fn a_source_directory(&self, name: &str) -> PathBuf {
            let path = self.sources.path().join(name);
            fs::create_dir_all(&path).expect("create source directory");
            path
        }

        fn attach(&self, source: &Path, basename: &str) -> Result<PathBuf, Status> {
            copy_attachment_into_session(self.path(), source, basename)
        }

        /// Attaches a source written on the spot under the same basename.
        fn attached(&self, basename: &str, contents: &[u8]) -> PathBuf {
            let source = self.a_source_file(basename, contents);
            self.attach(&source, basename)
                .expect("attaching a valid source must succeed")
        }

        fn attachments(&self) -> Vec<SessionAttachmentFile> {
            list_session_attachments(self.path())
        }
    }

    /// `(basename, size_bytes)` per listed attachment, in listing order.
    fn basenames_and_sizes(files: &[SessionAttachmentFile]) -> Vec<(String, u64)> {
        files
            .iter()
            .map(|f| (f.basename.clone(), f.size_bytes))
            .collect()
    }

    fn assert_refused_with<T>(result: Result<T, Status>, expected: Code) {
        let status = result
            .err()
            .unwrap_or_else(|| panic!("expected {expected:?}, but the operation succeeded"));
        assert_eq!(
            status.code, expected,
            "expected {expected:?}, got {:?} ({})",
            status.code, status.message
        );
    }

    fn assert_stored_bytes(path: &Path, expected: &[u8]) {
        let got = fs::read(path).unwrap_or_else(|e| panic!("reading {path:?} failed: {e}"));
        assert_eq!(got, expected, "stored bytes at {path:?}");
    }

    fn assert_no_attachments_written(session: &SessionUnderTest) {
        assert_eq!(
            basenames_and_sizes(&session.attachments()),
            Vec::<(String, u64)>::new(),
            "a rejected copy must not write under attachments"
        );
    }

    // ---- copying in -----------------------------------------------------------------------

    #[test]
    fn copying_a_source_file_stores_it_under_artifacts_attachments_with_its_bytes() {
        // Given — a session and a document to attach
        let session = a_session();
        let source = session.a_source_file("spec.md", b"# Spec\n");

        // When
        let stored = session
            .attach(&source, "spec.md")
            .expect("attaching a regular file must succeed");

        // Then — the file lands at artifacts/attachments/<basename> with the source's bytes
        assert_eq!(stored, session.attachments_dir().join("spec.md"));
        assert_stored_bytes(&stored, b"# Spec\n");
    }

    #[test]
    fn copying_a_binary_source_stores_the_bytes_verbatim() {
        // Given — a PNG header, which is not valid UTF-8
        let session = a_session();
        let png = &[0x89u8, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0xff, 0x00];
        let source = session.a_source_file("screenshot.png", png);

        // When
        let stored = session
            .attach(&source, "screenshot.png")
            .expect("attaching an image must succeed");

        // Then — every byte survives the copy
        assert_stored_bytes(&stored, png);
    }

    #[test]
    fn copying_creates_the_attachments_directory_when_the_session_has_none() {
        // Given — a session directory with no artifacts/ subtree at all
        let session = a_session_without_an_artifacts_dir();
        let source = session.a_source_file("notes.md", b"note\n");

        // When
        let stored = session
            .attach(&source, "notes.md")
            .expect("the store must create the directories it needs");

        // Then — artifacts/attachments/ was created and holds the file
        assert_eq!(stored, session.attachments_dir().join("notes.md"));
        assert_stored_bytes(&stored, b"note\n");
    }

    #[test]
    fn copying_a_second_source_under_an_existing_basename_is_refused_and_keeps_the_stored_bytes() {
        // Given — a session that already has spec.md attached
        let session = a_session();
        let stored = session.attached("spec.md", b"first\n");
        let replacement = session.a_source_file("replacement.md", b"second\n");

        // When — a second document claims the same basename
        let result = session.attach(&replacement, "spec.md");

        // Then — refused, and the originally stored bytes are untouched
        assert_refused_with(result, Code::FailedPrecondition);
        assert_stored_bytes(&stored, b"first\n");
    }

    #[test]
    fn copying_with_a_traversal_basename_is_rejected_as_invalid_argument() {
        // Given — a session and a valid source
        let session = a_session();
        let source = session.a_source_file("payload.md", b"payload\n");

        // When — the target basename tries to climb out of attachments/
        let result = session.attach(&source, "../escaped.md");

        // Then — refused, and nothing was written where the traversal pointed
        assert_refused_with(result, Code::InvalidArgument);
        assert!(
            !session.path().join("artifacts").join("escaped.md").exists(),
            "a rejected traversal must not write outside the attachments directory"
        );
    }

    #[test]
    fn copying_with_an_empty_basename_is_rejected_as_invalid_argument() {
        // Given — a session and a valid source
        let session = a_session();
        let source = session.a_source_file("payload.md", b"payload\n");

        // When — the target basename is empty
        let result = session.attach(&source, "");

        // Then
        assert_refused_with(result, Code::InvalidArgument);
        assert_no_attachments_written(&session);
    }

    #[test]
    fn copying_with_a_dot_basename_is_rejected_as_invalid_argument() {
        // Given — a session and a valid source
        let session = a_session();
        let source = session.a_source_file("payload.md", b"payload\n");

        // When — the target basename is a single dot
        let result = session.attach(&source, ".");

        // Then
        assert_refused_with(result, Code::InvalidArgument);
        assert_no_attachments_written(&session);
    }

    #[test]
    fn copying_with_a_dotdot_basename_is_rejected_as_invalid_argument() {
        // Given — a session and a valid source
        let session = a_session();
        let source = session.a_source_file("payload.md", b"payload\n");

        // When — the target basename is parent-directory traversal
        let result = session.attach(&source, "..");

        // Then
        assert_refused_with(result, Code::InvalidArgument);
        assert_no_attachments_written(&session);
    }

    #[test]
    fn copying_a_missing_source_is_rejected_as_invalid_argument() {
        // Given — a path that was never written
        let session = a_session();
        let missing = session.a_missing_source_path("ghost.md");

        // When
        let result = session.attach(&missing, "ghost.md");

        // Then
        assert_refused_with(result, Code::InvalidArgument);
    }

    #[test]
    fn copying_a_directory_as_the_source_is_rejected_as_invalid_argument() {
        // Given — a directory offered as the source
        let session = a_session();
        let directory = session.a_source_directory("a-folder");

        // When
        let result = session.attach(&directory, "a-folder");

        // Then
        assert_refused_with(result, Code::InvalidArgument);
    }

    #[cfg(unix)]
    #[test]
    fn copying_into_an_attachments_directory_symlinked_outside_the_artifacts_root_is_refused() {
        // Given — artifacts/attachments is a symlink pointing out of the session tree
        let session = a_session();
        let outside = tempfile::tempdir().expect("create outside dir");
        std::os::unix::fs::symlink(outside.path(), session.attachments_dir())
            .expect("create attachments symlink");
        let source = session.a_source_file("payload.md", b"payload\n");

        // When
        let result = session.attach(&source, "payload.md");

        // Then — the escape is refused and the outside directory stays empty
        assert_refused_with(result, Code::InvalidArgument);
        assert_eq!(
            fs::read_dir(outside.path())
                .expect("read outside dir")
                .count(),
            0,
            "a symlinked attachments directory must not be written through"
        );
    }

    // ---- listing --------------------------------------------------------------------------

    #[test]
    fn listing_attachments_returns_them_sorted_by_basename_with_their_sizes() {
        // Given — three attachments added out of alphabetical order
        let session = a_session();
        session.attached("zeta.md", b"zeta\n");
        session.attached("alpha.md", b"abc");
        session.attached("mid.png", &[1u8, 2, 3, 4]);

        // When
        let listed = session.attachments();

        // Then — basename order, with each file's byte size
        assert_eq!(
            basenames_and_sizes(&listed),
            vec![
                ("alpha.md".to_string(), 3),
                ("mid.png".to_string(), 4),
                ("zeta.md".to_string(), 5),
            ]
        );
    }

    #[test]
    fn listing_a_session_without_an_attachments_directory_returns_an_empty_list() {
        // Given — a session nobody has attached anything to
        let session = a_session();

        // When
        let listed = session.attachments();

        // Then — an empty list, not an error
        assert_eq!(basenames_and_sizes(&listed), Vec::new());
    }

    #[test]
    fn listing_skips_a_subdirectory_inside_the_attachments_directory() {
        // Given — one attachment and a stray subdirectory beside it
        let session = a_session();
        session.attached("keep.md", b"keep\n");
        fs::create_dir_all(session.attachments_dir().join("stray-dir"))
            .expect("create stray subdirectory");

        // When
        let listed = session.attachments();

        // Then — only the regular file is listed
        assert_eq!(
            basenames_and_sizes(&listed),
            vec![("keep.md".to_string(), 5)]
        );
    }

    #[test]
    fn a_listed_attachment_carries_its_absolute_path_under_the_attachments_root() {
        // Given — one attachment
        let session = a_session();
        session.attached("spec.md", b"# Spec\n");

        // When
        let listed = session.attachments();

        // Then — the row's path points at the stored file
        assert_eq!(
            listed.iter().map(|f| f.path.clone()).collect::<Vec<_>>(),
            vec![session.attachments_dir().join("spec.md")]
        );
    }
}
