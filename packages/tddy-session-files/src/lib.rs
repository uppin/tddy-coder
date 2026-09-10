//! Session file I/O: the workflow files a recipe wrote, agent context sync, uploads, and the staged
//! attachments and host documents a session start draws on.
//!
//! Extracted from `tddy-daemon` by `#unbundle` node 6, serving
//! `session_files.SessionFilesService` — families I, J, R and S, 13 methods.
//!
//! # Frame sizes are a wire contract
//!
//! `StreamReadHostDocument` frames at [`tddy_daemon_kernel::HOST_DOCUMENT_FRAME_BYTES`], and so do
//! the context-file reads, so a consumer that switches between them sees the same boundaries. That
//! constant is node 1's, published in the kernel, and it is the only thing this subsystem reached
//! the 23,099-line god module for (`context_files.rs:41`).
//!
//! # Truncation is refused, never returned
//!
//! A staged file whose upload never completed has no final chunk, and a fetch of it is **refused**
//! with `FAILED_PRECONDITION` (`types.proto` § `HOST_DOCUMENT_SCOPE_STAGED_ATTACHMENT`) rather than
//! answered with the bytes that did arrive. Handing back a prefix is worse than an error: the caller
//! cannot tell a short file from an interrupted one, and a session started from it materialises a
//! partial attachment that looks deliberate.
//!
//! # Errors are `Status`, not a crate error type
//!
//! Every method here answers an RPC, and the modules moving in speak [`tddy_rpc::Status`] already.
//! A crate-local error enum would buy one conversion at the boundary and one chance for a
//! documented code — `FAILED_PRECONDITION` for the incomplete upload above — to drift into
//! something else on the way out.

use std::sync::Arc;

use tddy_daemon_kernel::SessionsBaseResolver;
use tddy_rpc::Status;
use tddy_sandbox::ContextManifest;
use tddy_worktree_service::worktree_files::validate_rel_path_shape;

/// Where a document being read lives — the generated `types.HostDocumentScope`, not a Rust mirror
/// of it.
///
/// The enum is in `types.proto` rather than `session_files.proto` because
/// `connection.ConnectionService`'s `StartSession` needs it too: a session start names the staged
/// attachments to materialise, and each carries a scope. Re-exported rather than re-declared so a
/// caller never has to convert, and so [`HostDocumentScope::Unspecified`] — the proto3 zero value a
/// hand-written mirror silently drops — stays representable and therefore refusable.
pub use tddy_service::proto::types::HostDocumentScope;

/// Where the agent's guidance is read from, for one session.
///
/// A trait rather than two syncers because the *decision* — what to fetch, what to delete, what to
/// leave alone — must be identical on both halves, and the only thing that differs is where the
/// bytes come from: the split path talks to the daemon holding the codebase over LiveKit, the
/// co-located path reads the worktree sitting beside it.
///
/// Unframed on purpose. Framing is the streaming *transport's* concern; a source hands back one
/// path's bytes, and the caller that caps them is the caller that frames them.
pub trait ContextSource: Send + Sync {
    /// Every allow-listed path the repository currently serves, with its hash.
    fn manifest(&self) -> Result<ContextManifest, Status>;

    /// The raw bytes of one allow-listed path.
    fn read(&self, rel_path: &str) -> Result<Vec<u8>, Status>;
}

/// The `session_files.SessionFilesService` entry the daemon's wiring layer registers.
pub fn build_session_files_entry(
    _sessions_base: SessionsBaseResolver,
    _source: Arc<dyn ContextSource>,
) -> tddy_rpc::ServiceEntry {
    // TODO(session-io-services): implement once the 13 handlers move in — the entry has no service
    // to point at until `context_files`, `session_file_upload`, `session_attachment_staging` and
    // `host_documents` are in this crate.
    unimplemented!("build_session_files_entry")
}

/// Refuse a relative path that could escape its scope's root, and a scope that names no root.
///
/// This is the **filesystem-independent** half of the check, and it is deliberately separate from
/// containment for the reason [`validate_rel_path_shape`] gives: a refusal must not depend on
/// whether a file happens to exist, so the syntactic guard runs on its own and canonicalize-and-
/// contain runs afterwards, against the resolved scope root.
///
/// It delegates the shape rules — absolute paths, leading separators, `..` traversal, and a
/// backslash read as a separator the same way the lookup will read it — to that one function rather
/// than restating them. Two spellings of "is this inside the root" is how one of them ends up being
/// the lenient one, and this particular check decides whether a caller can read an arbitrary file as
/// the session's OS user.
///
/// An unspecified scope is refused rather than defaulted. It is proto3's zero value, so an
/// uninitialised or forward-incompatible request arrives carrying it, and every scope resolves to a
/// *different* root — picking one would read a file from the wrong place and answer as if that were
/// what was asked for.
pub fn validate_relative_path(scope: HostDocumentScope, relative_path: &str) -> Result<(), Status> {
    if scope == HostDocumentScope::Unspecified {
        return Err(Status::invalid_argument(
            "host document scope must be specified",
        ));
    }
    if relative_path.is_empty() {
        return Err(Status::invalid_argument("relative_path must not be empty"));
    }
    // TODO(session-io-services): `SESSION_UPLOAD` and `STAGED_ATTACHMENT` address a file as exactly
    // `<id>/<file_name>`, both segments pure basenames. That rule needs `validate_segment`, which
    // is still `pub(crate)` in `tddy-daemon`'s `session_file_upload` and arrives with it — until
    // then `host_documents::resolve_host_document` applies it, and this function must not be taken
    // for the whole gate.
    validate_rel_path_shape(relative_path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tddy_rpc::Code;

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
        let outcome =
            validate_relative_path(HostDocumentScope::SessionWorktree, "../../etc/passwd");

        // Then
        assert_eq!(
            outcome.expect_err("a traversal must be refused").code(),
            Code::InvalidArgument
        );
    }

    #[test]
    fn accepts_a_path_inside_its_scope() {
        // When
        let outcome = validate_relative_path(HostDocumentScope::SessionArtifact, "PRD.md");

        // Then
        assert!(outcome.is_ok());
    }

    /// Every scope resolves to a different root, so the proto3 zero value names none of them. A
    /// scope silently mapped to a real one reads a file from the wrong place.
    #[test]
    fn refuses_a_read_whose_scope_was_never_specified() {
        // When
        let outcome = validate_relative_path(HostDocumentScope::Unspecified, "PRD.md");

        // Then
        assert_eq!(
            outcome
                .expect_err("an unspecified scope must be refused")
                .code(),
            Code::InvalidArgument
        );
    }

    /// Handing back the bytes that did arrive is worse than an error: the caller cannot tell a short
    /// file from an interrupted one, and a session started from it materialises a partial attachment
    /// that looks deliberate.
    ///
    /// TODO(session-io-services): this asserts on a stub, not on the crate. The real refusal is the
    /// `.staged-complete` marker gate in `resolve_host_document`; re-point this at it once
    /// `host_documents` and `session_attachment_staging` move in.
    #[test]
    fn refuses_a_staged_file_whose_upload_never_completed() {
        // Given a source that reports an incomplete upload
        let source = ARefusingSource;

        // When
        let outcome = source.read("staging-1/big.patch");

        // Then
        assert_eq!(
            outcome
                .expect_err("an incomplete upload must be refused")
                .code(),
            Code::FailedPrecondition
        );
    }

    /// The two host-document reads frame identically, so a consumer switching between them sees the
    /// same boundaries. The constant is node 1's, in the kernel.
    ///
    /// TODO(session-io-services): this pins the constant, not the framing. The framing lives in
    /// `stream_document_frames`, which arrives with `host_documents`; assert the frame *sizes* of a
    /// document longer than one frame once it is here.
    #[test]
    fn frames_a_document_at_the_kernels_host_document_size() {
        assert_eq!(
            tddy_daemon_kernel::HOST_DOCUMENT_FRAME_BYTES,
            48 * 1024,
            "the streaming and unary reads must agree on frame size"
        );
    }

    /// A source that refuses every read the way an incomplete upload is refused, for the callers
    /// that need a `ContextSource` but not its bytes.
    struct ARefusingSource;

    impl ContextSource for ARefusingSource {
        fn manifest(&self) -> Result<ContextManifest, Status> {
            Err(Status::failed_precondition(
                "the upload of staging-1/big.patch never completed",
            ))
        }

        fn read(&self, rel_path: &str) -> Result<Vec<u8>, Status> {
            Err(Status::failed_precondition(format!(
                "the upload of {rel_path} never completed"
            )))
        }
    }
}
