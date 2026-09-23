//! The changeset model and the session metadata stored beside it: `changeset.yaml`, the PR stack
//! an orchestrator session carries, the unified session directory, and each session's agents,
//! activity and labels.
//!
//! Extracted from `tddy-core`, which re-exports every module and root item at its old path.

pub mod agent_activity;
pub mod branch_worktree_intent;
pub mod changeset;
pub mod elapsed_format;
pub mod session_activity;
pub mod session_agent;
pub mod session_context;
pub mod session_label;
pub mod session_lifecycle;
pub mod session_metadata;
pub mod session_participant_metadata;
pub mod source_path;

// The storage layer these modules are built on, named at the `crate::` paths they used inside
// `tddy-core`.
use tddy_session_store::error::WorkflowError;
use tddy_session_store::{atomic_file, error, output};

pub use changeset::{
    append_session_and_update_state, get_session_for_tag, merge_persisted_workflow_into_context,
    read_changeset, resolve_agent_from_changeset, resolve_model, update_state, write_changeset,
    write_changeset_atomic, BranchWorktreeIntent, Changeset, ChangesetState, ChangesetWorkflow,
    ClarificationQa, ClarificationQuestionForQa, DiscoveryData, GithubPrStatus,
    QuestionOptionForQa, SessionEntry, StateTransition,
};
pub use changeset::{
    link_stack_node_to_child_session, sync_stack_node_from_child, update_stack_atomic,
    PrInternalStatus, Stack, StackNode,
};
pub use elapsed_format::format_elapsed_compact;
pub use session_activity::{
    activity_status_from_hook, parse_hook_event, HookEvent, SessionActivityStatus,
};
pub use session_agent::{AgentId, AgentIdError, SessionAgentRecord};
pub use session_lifecycle::{
    materialize_unified_session_directory, resolve_effective_session_id, unified_session_dir_path,
    validate_session_id_segment, SessionIdValidationError, SessionLifecycleBootstrap,
    UnifiedSessionTreeBootstrap,
};
pub use session_metadata::{
    paired_agent, read_session_metadata, repo_root_for_session, update_activity_status,
    write_initial_tool_session_metadata, write_session_metadata, InitialToolSessionMetadataOpts,
    SessionMetadata, SESSION_METADATA_FILENAME,
};
pub use source_path::{classify_rust_source_path, RustSourcePathKind};
