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

pub mod attachment_progress;
pub mod attachment_prompt;
pub mod context_files;
pub mod context_sync;
pub mod host_documents;
pub mod service;
pub mod session_attachment_materialization;
pub mod session_attachment_staging;
pub mod session_attachments;
pub mod session_context_docs;
pub mod session_file_upload;
pub mod session_files;
pub mod session_uploads;
pub mod session_workflow_files;
pub mod stack_doc_attachments;
pub mod svc_materialize_staged_attachment;

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
/// `the pre-unbundle monolithic RPC coordinate`'s `StartSession` needs it too: a session start names the staged
/// attachments to materialise, and each carries a scope. Re-exported rather than re-declared so a
/// caller never has to convert, and so [`HostDocumentScope::Unspecified`] — the proto3 zero value a
/// hand-written mirror silently drops — stays representable and therefore refusable.
pub use tddy_service::proto::types::HostDocumentScope;

pub use service::{SessionFilesPorts, SessionFilesServiceImpl};

/// The two halves of the path gate every host-document read passes through.
///
/// Re-exported from [`host_documents`] rather than declared here, because this crate had both for
/// one release of this branch and the two disagreed: the exported pair enforced neither
/// `SESSION_WORKTREE`'s git listing nor the basename rule the served path applies, so a caller
/// reaching for the crate's advertised gate got the weaker check. Two spellings of "is this inside
/// the root" is how one of them ends up being the lenient one — so there is one spelling, it lives
/// beside the resolver that runs it, and [`host_documents::resolve_host_document`] calls both of
/// these and nothing else.
pub use host_documents::{contained_in_scope_root, validate_relative_path};

#[cfg(all(test, unix))]
mod service_tests;
