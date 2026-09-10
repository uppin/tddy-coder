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

pub mod context_files;
pub mod context_sync;
pub mod host_documents;
pub mod service;
pub mod session_attachment_staging;
pub mod session_attachments;
pub mod session_context_docs;
pub mod session_file_upload;
pub mod session_uploads;
pub mod session_workflow_files;
pub mod stack_doc_attachments;

use tddy_rpc::Status;
use tddy_worktree_service::worktree_files::validate_rel_path_shape;

/// Where the agent's guidance is read from, for one session — the agent-context-sync trait, which
/// arrived with the module that owns it.
///
/// Re-exported from [`context_sync`] rather than declared here: `split_session` (which stays in
/// `tddy-daemon`) implements the decision procedure against it, and two declarations of one trait is
/// how the split half and the co-located half would stop being obliged to sync identically.
pub use context_sync::{ContextSource, ContextSyncer, LocalWorktreeSource, PrefetchedContext};

/// Where a document being read lives — the generated `types.HostDocumentScope`, not a Rust mirror
/// of it.
///
/// The enum is in `types.proto` rather than `session_files.proto` because
/// `connection.ConnectionService`'s `StartSession` needs it too: a session start names the staged
/// attachments to materialise, and each carries a scope. Re-exported rather than re-declared so a
/// caller never has to convert, and so [`HostDocumentScope::Unspecified`] — the proto3 zero value a
/// hand-written mirror silently drops — stays representable and therefore refusable.
pub use tddy_service::proto::types::HostDocumentScope;

pub use service::{build_session_files_entry, SessionFilesPorts, SessionFilesServiceImpl};

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
///
/// `SESSION_UPLOAD` and `STAGED_ATTACHMENT` address a file as exactly `<id>/<file_name>`, both
/// segments pure basenames — the rule [`session_file_upload::validate_segment`] states, applied
/// here through [`host_documents::validate_two_segment_relative_path`] rather than restated, because
/// both segments are untrusted client input that become path components.
pub fn validate_relative_path(scope: HostDocumentScope, relative_path: &str) -> Result<(), Status> {
    if scope == HostDocumentScope::Unspecified {
        return Err(Status::invalid_argument(
            "host document scope must be specified",
        ));
    }
    if relative_path.is_empty() {
        return Err(Status::invalid_argument("relative_path must not be empty"));
    }
    validate_rel_path_shape(relative_path)?;
    match scope {
        HostDocumentScope::SessionUpload => host_documents::validate_two_segment_relative_path(
            relative_path,
            "session upload",
            "upload_id",
        ),
        HostDocumentScope::StagedAttachment => host_documents::validate_two_segment_relative_path(
            relative_path,
            "staged attachment",
            "staging_id",
        ),
        _ => Ok(()),
    }
}

/// Refuse a path that leaves `scope_root` once every symlink on it has been followed.
///
/// The filesystem-dependent half of the gate [`validate_relative_path`] opens, kept separate for
/// the reason that function gives and applied *after* it. It reuses
/// [`session_file_upload::contained_canonical_dir`] — the guard the upload writer, the upload
/// delete, the staging writer and the staged delete all share — rather than spelling containment a
/// fifth time: `.claude/creds -> ../../.env` canonicalizes to a path outside the root while passing
/// every syntactic check there is, and a second spelling of this check is how one of them ends up
/// being the lenient one.
pub fn contained_in_scope_root(
    scope_root: &std::path::Path,
    relative_path: &str,
) -> Result<std::path::PathBuf, Status> {
    let joined = scope_root.join(relative_path.replace('\\', "/"));
    let parent = joined
        .parent()
        .ok_or_else(|| Status::invalid_argument("relative_path must name a file"))?;
    let canonical_parent = session_file_upload::contained_canonical_dir(scope_root, parent)?;
    let file_name = joined
        .file_name()
        .ok_or_else(|| Status::invalid_argument("relative_path must name a file"))?;
    Ok(canonical_parent.join(file_name))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::Arc;

    use prost::Message as _;
    use tddy_rpc::{Code, RpcMessage, RpcResult};
    use tddy_service::proto::session_files::{
        ListSessionWorkflowFilesRequest, ListSessionWorkflowFilesResponse,
    };

    use crate::service::{SessionContextScope, SessionContextScopes, SessionFilesPorts};

    const OS_USER: &str = "tddy-test-user";
    const SESSION_ID: &str = "aaaaaaaa-aaaa-7aaa-8aaa-aaaaaaaaaaaa";
    const TOKEN: &str = "a-valid-session-token";
    const SERVICE: &str = "session_files.SessionFilesService";

    /// A host with a data dir and a staging area on disk, and the entry that serves it.
    struct AHost {
        _root: tempfile::TempDir,
        tddy_data_dir: PathBuf,
        staging_base_dir: PathBuf,
    }

    fn a_host() -> AHost {
        let root = tempfile::tempdir().expect("a temp dir");
        let tddy_data_dir = root.path().join("data");
        let staging_base_dir = root.path().join("staging");
        std::fs::create_dir_all(&tddy_data_dir).expect("the data dir");
        std::fs::create_dir_all(&staging_base_dir).expect("the staging base");
        AHost {
            _root: root,
            tddy_data_dir,
            staging_base_dir,
        }
    }

    impl AHost {
        fn session_dir(&self) -> PathBuf {
            tddy_core::session_lifecycle::unified_session_dir_path(&self.tddy_data_dir, SESSION_ID)
        }

        /// A workflow file the allow-list names, in the session directory the service resolves to.
        fn with_workflow_file(&self, basename: &str, contents: &str) -> &Self {
            let dir = self.session_dir();
            std::fs::create_dir_all(&dir).expect("the session dir");
            std::fs::write(dir.join(basename), contents).expect("the workflow file");
            self
        }

        /// A staged file whose uploader never wrote the final chunk: bytes on disk, no
        /// `.staged-complete` marker beside them.
        fn with_an_interrupted_staged_upload(&self, staging_id: &str, file_name: &str) -> &Self {
            let batch =
                session_attachment_staging::staging_root_for(OS_USER, &self.staging_base_dir)
                    .join(staging_id);
            std::fs::create_dir_all(&batch).expect("the staging batch dir");
            std::fs::write(batch.join(file_name), b"the bytes that did arrive")
                .expect("the partial file");
            self
        }

        fn ports(&self) -> SessionFilesPorts {
            SessionFilesPorts {
                os_users: Arc::new(|token: &str| {
                    if token == TOKEN {
                        Ok(OS_USER.to_string())
                    } else {
                        Err(Status::unauthenticated("invalid or expired session"))
                    }
                }),
                tddy_data_dir: self.tddy_data_dir.clone(),
                staging_base_dir: self.staging_base_dir.clone(),
                max_attachment_bytes: 4 * 1024 * 1024,
                daemon_instance_id: "the-serving-host".to_string(),
                context_scopes: Arc::new(NoContextScope),
            }
        }

        fn entry(&self) -> tddy_rpc::ServiceEntry {
            build_session_files_entry(self.ports())
        }
    }

    /// A scope resolver no test here reaches: the context family is proved by
    /// `tests/context_files_acceptance.rs`, against the reader rather than the transport.
    struct NoContextScope;

    impl SessionContextScopes for NoContextScope {
        fn scope_for(&self, _: &str, _: &str, _: &str) -> Result<SessionContextScope, Status> {
            Err(Status::failed_precondition(
                "session not found or .session.yaml missing",
            ))
        }
    }

    /// The unary answer to one method of the entry, decoded.
    async fn unary_answer(entry: &tddy_rpc::ServiceEntry, method: &str, request: &[u8]) -> Vec<u8> {
        let message = RpcMessage::new(request.to_vec(), Default::default());
        match entry.service.handle_rpc(SERVICE, method, &message).await {
            RpcResult::Unary(Ok(bytes)) => bytes,
            RpcResult::Unary(Err(status)) => panic!("{method} was refused: {status:?}"),
            RpcResult::ServerStream(_) => panic!("{method} answered with a stream"),
        }
    }

    #[test]
    fn names_the_service_families_i_j_r_and_s_move_to() {
        // Given
        let host = a_host();

        // When
        let entry = host.entry();

        // Then
        assert_eq!(entry.name, "session_files.SessionFilesService");
    }

    /// The name alone would be satisfied by an entry with nothing behind it, so this dispatches a
    /// real method at the registered service and reads the answer back off the wire.
    #[tokio::test]
    async fn answers_a_method_dispatched_at_the_registered_service() {
        // Given
        let host = a_host();
        host.with_workflow_file("PRD.md", "# the plan")
            .with_workflow_file("TODO.md", "- [ ] the work");
        let entry = host.entry();
        let request = ListSessionWorkflowFilesRequest {
            session_token: TOKEN.to_string(),
            session_id: SESSION_ID.to_string(),
        };

        // When
        let answer =
            unary_answer(&entry, "ListSessionWorkflowFiles", &request.encode_to_vec()).await;

        // Then
        let response =
            ListSessionWorkflowFilesResponse::decode(&answer[..]).expect("a decodable response");
        assert_eq!(
            response
                .files
                .iter()
                .map(|f| f.basename.as_str())
                .collect::<Vec<_>>(),
            vec!["PRD.md", "TODO.md"]
        );
    }

    /// A token the host does not know must not reach the filesystem at all, so the refusal is the
    /// service's rather than a read's.
    #[tokio::test]
    async fn refuses_a_method_whose_session_token_is_not_this_hosts() {
        // Given
        let host = a_host();
        host.with_workflow_file("PRD.md", "# the plan");
        let entry = host.entry();
        let request = ListSessionWorkflowFilesRequest {
            session_token: "a-token-from-another-host".to_string(),
            session_id: SESSION_ID.to_string(),
        };
        let message = RpcMessage::new(request.encode_to_vec(), Default::default());

        // When
        let outcome = entry
            .service
            .handle_rpc(SERVICE, "ListSessionWorkflowFiles", &message)
            .await;

        // Then
        let RpcResult::Unary(Err(status)) = outcome else {
            panic!("an unknown token must be refused");
        };
        assert_eq!(status.code(), Code::Unauthenticated);
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

    /// `SESSION_UPLOAD` addresses a file as exactly `<upload_id>/<file_name>`, so a bare basename
    /// names no drop to read it out of.
    #[test]
    fn refuses_a_session_upload_path_that_names_no_drop() {
        // When
        let outcome = validate_relative_path(HostDocumentScope::SessionUpload, "spec.md");

        // Then
        assert_eq!(
            outcome
                .expect_err("a one-segment upload path must be refused")
                .message,
            "session upload relative_path must be <upload_id>/<file_name>"
        );
    }

    /// The second segment is a file name, not a directory to descend into: a nested path would let
    /// a caller read out of a drop's subtree.
    #[test]
    fn refuses_a_staged_attachment_path_with_a_third_segment() {
        // When
        let outcome = validate_relative_path(
            HostDocumentScope::StagedAttachment,
            "aaaaaaaa-aaaa-7aaa-8aaa-aaaaaaaaaaaa/nested/spec.md",
        );

        // Then
        assert_eq!(
            outcome
                .expect_err("a three-segment staged path must be refused")
                .message,
            "staged attachment relative_path must be <staging_id>/<file_name>"
        );
    }

    #[test]
    fn accepts_a_staged_attachment_addressed_as_a_batch_and_a_file_name() {
        // When
        let outcome = validate_relative_path(
            HostDocumentScope::StagedAttachment,
            "aaaaaaaa-aaaa-7aaa-8aaa-aaaaaaaaaaaa/spec.md",
        );

        // Then
        assert!(outcome.is_ok(), "got {outcome:?}");
    }

    /// The syntactic gate passes a path made entirely of ordinary segments, so containment is
    /// checked against the *resolved* root: a symlinked directory inside the scope points wherever
    /// its target does, and `std::fs::read` follows it.
    #[test]
    fn refuses_a_path_whose_symlinked_directory_leaves_the_scope_root() {
        // Given a scope root holding `escape -> ../outside`
        let root = tempfile::tempdir().expect("a temp dir");
        let scope_root = root.path().join("artifacts");
        let outside = root.path().join("outside");
        std::fs::create_dir_all(&scope_root).expect("the scope root");
        std::fs::create_dir_all(&outside).expect("the directory outside it");
        std::fs::write(outside.join("id_rsa"), b"a private key").expect("the file to reach for");
        std::os::unix::fs::symlink(&outside, scope_root.join("escape")).expect("the symlink");

        // When
        let outcome = contained_in_scope_root(&scope_root, "escape/id_rsa");

        // Then
        assert_eq!(
            outcome
                .expect_err("a symlinked escape must be refused")
                .code(),
            Code::InvalidArgument
        );
    }

    #[test]
    fn resolves_a_contained_path_to_its_canonical_file() {
        // Given
        let root = tempfile::tempdir().expect("a temp dir");
        let scope_root = root.path().join("artifacts");
        std::fs::create_dir_all(scope_root.join("docs")).expect("the scope root");
        std::fs::write(scope_root.join("docs/PRD.md"), b"# the plan").expect("the document");

        // When
        let resolved = contained_in_scope_root(&scope_root, "docs/PRD.md")
            .expect("a contained path must resolve");

        // Then
        assert_eq!(
            resolved,
            scope_root
                .canonicalize()
                .expect("the canonical scope root")
                .join("docs/PRD.md")
        );
    }

    /// Handing back the bytes that did arrive is worse than an error: the caller cannot tell a short
    /// file from an interrupted one, and a session started from it materialises a partial attachment
    /// that looks deliberate. The refusal is the `.staged-complete` marker gate — the marker its
    /// uploader drops beside the file on the final chunk, and only then.
    #[test]
    fn refuses_a_staged_file_whose_upload_never_completed() {
        // Given a staged batch whose uploader never wrote the final chunk
        let host = a_host();
        host.with_an_interrupted_staged_upload("staging-1", "big.patch");

        // When
        let outcome = host_documents::resolve_host_document(
            OS_USER,
            &host.tddy_data_dir,
            &host.staging_base_dir,
            HostDocumentScope::StagedAttachment,
            "",
            "",
            "staging-1/big.patch",
        );

        // Then
        let refusal = outcome.expect_err("an incomplete upload must be refused");
        assert_eq!(refusal.code(), Code::FailedPrecondition);
        assert_eq!(refusal.message, "staged attachment upload is not complete");
    }

    /// The same file once its uploader dropped the marker: the gate is the marker, not the bytes.
    #[test]
    fn serves_a_staged_file_once_its_upload_completed() {
        // Given
        let host = a_host();
        host.with_an_interrupted_staged_upload("staging-1", "big.patch");
        let batch = session_attachment_staging::staging_root_for(OS_USER, &host.staging_base_dir)
            .join("staging-1");
        std::fs::write(
            session_attachment_staging::staged_complete_marker(&batch, "big.patch"),
            b"",
        )
        .expect("the completeness marker");

        // When
        let resolved = host_documents::resolve_host_document(
            OS_USER,
            &host.tddy_data_dir,
            &host.staging_base_dir,
            HostDocumentScope::StagedAttachment,
            "",
            "",
            "staging-1/big.patch",
        )
        .expect("a completed upload must be served");

        // Then
        assert_eq!(resolved.byte_size, "the bytes that did arrive".len() as u64);
    }

    /// The two host-document reads frame identically, so a consumer switching between them sees the
    /// same boundaries. The constant is node 1's, in the kernel — and this asserts the frames a
    /// document longer than one frame actually produces, not the constant against itself.
    #[tokio::test]
    async fn frames_a_document_at_the_kernels_host_document_size() {
        // Given a document two and a half frames long
        let frame_bytes = tddy_daemon_kernel::HOST_DOCUMENT_FRAME_BYTES;
        let total = frame_bytes * 2 + frame_bytes / 2;
        let root = tempfile::tempdir().expect("a temp dir");
        let path = root.path().join("attachment.bin");
        std::fs::write(&path, vec![b'x'; total]).expect("the document");
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Result<(Vec<u8>, u64), Status>>();

        // When
        host_documents::stream_document_frames(&path, total as u64, |data, size| (data, size), &tx);
        drop(tx);

        // Then
        let mut frames = Vec::new();
        while let Some(frame) = rx.recv().await {
            frames.push(frame.expect("a frame, not a mid-stream failure"));
        }
        assert_eq!(
            frames
                .iter()
                .map(|(data, _)| data.len())
                .collect::<Vec<_>>(),
            vec![frame_bytes, frame_bytes, frame_bytes / 2],
            "the streaming and unary reads must agree on frame size"
        );
        assert!(
            frames.iter().all(|(_, size)| *size == total as u64),
            "every frame carries the whole document's size, so a consumer sizes a progress bar \
             from the first one"
        );
    }

    /// A zero-byte document still yields exactly one frame, so "the document is empty" never has to
    /// be told apart from "the stream produced nothing".
    #[tokio::test]
    async fn frames_an_empty_document_as_one_empty_frame() {
        // Given
        let root = tempfile::tempdir().expect("a temp dir");
        let path = root.path().join("empty.bin");
        std::fs::write(&path, b"").expect("the empty document");
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Result<(Vec<u8>, u64), Status>>();

        // When
        host_documents::stream_document_frames(&path, 0, |data, size| (data, size), &tx);
        drop(tx);

        // Then
        let mut frames = Vec::new();
        while let Some(frame) = rx.recv().await {
            frames.push(frame.expect("a frame, not a mid-stream failure"));
        }
        assert_eq!(frames, vec![(Vec::new(), 0)]);
    }
}
