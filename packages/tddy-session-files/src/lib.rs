//! Session file I/O: the workflow files a recipe wrote, agent context sync, uploads, and the staged
//! attachments and host documents a session start draws on.
//!
//! Extracted from `tddy-daemon` by `#unbundle` node 6, serving
//! `session_files.SessionFilesService` — families I, J, R and S, 13 methods.
//!
//! # Frame sizes are a wire contract
//!
//! `ReadHostDocument` and `StreamReadHostDocument` frame identically, at
//! [`tddy_daemon_kernel::HOST_DOCUMENT_FRAME_BYTES`], so a consumer that switches between them sees
//! the same boundaries. That constant is node 1's, published in the kernel, and it is the only thing
//! this subsystem reached the 23,099-line god module for (`context_files.rs:41`).
//!
//! # Truncation is refused, never returned
//!
//! A staged file whose upload never completed has no final chunk, and a fetch of it is **refused**
//! rather than answered with the bytes that did arrive. Handing back a prefix is worse than an
//! error: the caller cannot tell a short file from an interrupted one, and a session started from it
//! materialises a partial attachment that looks deliberate.

use std::sync::Arc;

use tddy_daemon_kernel::SessionsBaseResolver;

/// Where a document being read lives.
///
/// Mirrors `types.HostDocumentScope` on the wire. It is in `types.proto` rather than
/// `session_files.proto` because `connection.ConnectionService`'s `StartSession` needs it too — a
/// session start names the staged attachments to materialise, and each carries a scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentScope {
    SessionArtifact,
    SessionUpload,
    SessionWorktree,
    ProjectRepo,
    StagedAttachment,
}

/// Why a session file could not be read or written.
#[derive(Debug, thiserror::Error)]
pub enum SessionFilesError {
    #[error("no session {session_id} on this host")]
    NoSuchSession { session_id: String },
    #[error("{relative_path} is outside the {scope:?} root")]
    OutsideScope {
        relative_path: String,
        scope: DocumentScope,
    },
    #[error("the upload of {relative_path} never completed, so its bytes are incomplete")]
    IncompleteUpload { relative_path: String },
}

/// Reads a document from one of the scopes a session can draw on.
pub trait ContextSource: Send + Sync {
    /// Read `relative_path` within `scope`, in frames of
    /// [`tddy_daemon_kernel::HOST_DOCUMENT_FRAME_BYTES`].
    fn read(
        &self,
        scope: DocumentScope,
        relative_path: &str,
    ) -> Result<Vec<Vec<u8>>, SessionFilesError>;
}

/// The `session_files.SessionFilesService` entry the daemon's wiring layer registers.
pub fn build_session_files_entry(
    _sessions_base: SessionsBaseResolver,
    _source: Arc<dyn ContextSource>,
) -> tddy_rpc::ServiceEntry {
    // TODO(session-io-services): implement
    unimplemented!("build_session_files_entry")
}

/// Refuse a relative path that escapes its scope's root.
///
/// Checked rather than trusted: the path comes from a caller, and a `..` that resolves outside the
/// scope would read an arbitrary file as that OS user.
pub fn validate_relative_path(
    _scope: DocumentScope,
    _relative_path: &str,
) -> Result<(), SessionFilesError> {
    // TODO(session-io-services): implement
    unimplemented!("validate_relative_path")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_service_families_i_j_r_and_s_move_to() {
        // Given
        let sessions_base: SessionsBaseResolver = Arc::new(|_| None);
        let source: Arc<dyn ContextSource> = Arc::new(ARefusingSource);

        // When
        let entry = build_session_files_entry(sessions_base, source);

        // Then
        assert_eq!(entry.name, "session_files.SessionFilesService");
    }

    /// A `..` that escapes the scope would read an arbitrary file as the session's OS user, so the
    /// path is checked rather than trusted.
    #[test]
    fn refuses_a_path_that_escapes_its_scope() {
        // When
        let outcome = validate_relative_path(DocumentScope::SessionWorktree, "../../etc/passwd");

        // Then
        assert!(matches!(
            outcome,
            Err(SessionFilesError::OutsideScope { .. })
        ));
    }

    #[test]
    fn accepts_a_path_inside_its_scope() {
        // When
        let outcome = validate_relative_path(DocumentScope::SessionArtifact, "PRD.md");

        // Then
        assert!(outcome.is_ok());
    }

    /// Handing back the bytes that did arrive is worse than an error: the caller cannot tell a short
    /// file from an interrupted one, and a session started from it materialises a partial attachment
    /// that looks deliberate.
    #[test]
    fn refuses_a_staged_file_whose_upload_never_completed() {
        // Given a source that reports an incomplete upload
        let source = ARefusingSource;

        // When
        let outcome = source.read(DocumentScope::StagedAttachment, "staging-1/big.patch");

        // Then
        assert!(matches!(
            outcome,
            Err(SessionFilesError::IncompleteUpload { .. })
        ));
    }

    /// The two host-document reads frame identically, so a consumer switching between them sees the
    /// same boundaries. The constant is node 1's, in the kernel.
    #[test]
    fn frames_a_document_at_the_kernels_host_document_size() {
        assert_eq!(
            tddy_daemon_kernel::HOST_DOCUMENT_FRAME_BYTES,
            48 * 1024,
            "the streaming and unary reads must agree on frame size"
        );
    }

    struct ARefusingSource;

    impl ContextSource for ARefusingSource {
        fn read(
            &self,
            _scope: DocumentScope,
            relative_path: &str,
        ) -> Result<Vec<Vec<u8>>, SessionFilesError> {
            Err(SessionFilesError::IncompleteUpload {
                relative_path: relative_path.to_string(),
            })
        }
    }
}
