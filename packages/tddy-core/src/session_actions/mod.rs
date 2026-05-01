//! Declarative **session actions**: YAML manifests under `<session>/actions`, validation, invocation contracts.
//!
//! See **[session-actions.md](../../../../docs/ft/coder/session-actions.md)**. CLI wiring lives in **tddy-tools** (`session_actions_cli`).

mod arch;
mod error;
mod invoke;
mod list;
mod manifest;
mod paths;
mod summary;
mod validate;

pub use arch::ensure_action_architecture;
pub use error::SessionActionsError;
pub use invoke::run_manifest_command;
pub use list::{list_action_summaries, ActionSummary};
pub use manifest::{parse_action_manifest_file, parse_action_manifest_yaml, ActionManifest};
pub use paths::resolve_allowlisted_path;
pub use summary::{invocation_record_summary_value, parse_test_summary_from_process_output, TestSummary};
pub use validate::validate_action_arguments_json;
