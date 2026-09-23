//! Core library for tddy-coder — a wiring point.
//!
//! Every group of code lives in a crate of its own; this crate re-exports each of them whole, so
//! every `tddy_core::<module>::…` path and every root-level item consumers name resolves exactly as
//! it did when the code lived here. New code should name the owning crate directly.

pub use tddy_agent_backend::*;
pub use tddy_agent_skills::*;
pub use tddy_changeset::*;
pub use tddy_log::*;
pub use tddy_presenter::*;
pub use tddy_session_actions::*;
pub use tddy_session_worktree::*;
pub use tddy_toolcall::*;
pub use tddy_workflow_engine::*;

pub mod atomic_file;
pub mod changeset;
pub mod error;
pub mod output;
pub mod ssh_exec;

pub use atomic_file::{write_atomic, write_atomic_labelled};
pub use error::{BackendError, ParseError, WorkflowError};
pub use ssh_exec::{
    contain_remote_path, default_remote_repo_root, run_ssh_batch, shell_single_quote,
};
pub use tddy_workflow::{
    canonical_artifact_write_path, canonical_attachment_write_path, read_session_artifact_utf8,
    read_session_artifact_utf8_or_placeholder, resolve_existing_session_artifact,
    session_artifacts_root, session_attachments_root, SESSION_ARTIFACT_READ_PLACEHOLDER,
    SESSION_ATTACHMENTS_SUBDIR,
};

#[cfg(test)]
mod workflow_decouple_acceptance {
    /// After decoupling, the legacy session primary-document path helper must not be re-exported from the crate root.
    #[test]
    fn core_src_free_of_prd_path_helper() {
        // Given
        let lib_rs = include_str!("lib.rs");
        let forbidden = [
            "pub ",
            "use session_plan_prd::",
            "plan_prd_path_for_session_dir",
        ]
        .concat();

        // Then
        assert!(
            !lib_rs.contains(&forbidden),
            "tddy-core lib.rs must not re-export the legacy session_plan_prd helper; use workflow manifest resolvers"
        );
    }
}
