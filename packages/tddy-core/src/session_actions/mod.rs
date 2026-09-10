//! Declarative **session actions**: YAML manifests under `<session>/actions`, validation, invocation contracts.
//!
//! See **[session-actions.md](../../../../docs/ft/coder/session-actions.md)**. CLI wiring — argument parsing, stdout and exit codes — lives in **tddy-tools** (`session_actions_cli`).

mod arch;
mod authoring;
mod error;
mod invoke;
mod list;
mod manifest;
mod paths;
pub(crate) mod runtime;
mod session_dir;
mod summary;
mod tool_gate;
mod validate;

pub use arch::ensure_action_architecture;
pub use authoring::{
    author_prompt, extract_manifest_yaml, prevalidate_manifest_yaml, MAX_AUTHOR_ATTEMPTS,
    MAX_MANIFEST_BYTES,
};
pub use error::{classify_session_actions_exit_code, SessionActionsError};
pub use invoke::{finalize_invocation_record, invoke_action_core, run_manifest_command};
pub use list::{list_action_summaries, ActionListResult, ActionSummary, DiscoveryQuery};
pub use manifest::{parse_action_manifest_file, parse_action_manifest_yaml, ActionManifest};
pub use paths::{
    derive_repo_key, repo_actions_root, resolve_action_manifest_path, resolve_allowlisted_path,
};
pub use runtime::action_manifest_to_spec;
pub use runtime::run_manifest_blocking;
pub use session_dir::{
    invoke_action_in_session_dir, list_actions_in_session_dir, ListActionsResponse,
};
pub use summary::{
    invocation_record_summary_value, parse_test_summary_from_process_output, TestSummary,
};
pub use tool_gate::{session_action_tools_enabled, SESSION_ACTION_TOOLS_ENV};
pub use validate::{validate_action_arguments_json, validate_authored_manifest};
