//! Core library for tddy-coder.

pub mod backend;
pub mod changeset;
pub mod elapsed_format;
pub mod error;
pub mod log_backend;
pub mod output;
pub mod presenter;
pub mod session_metadata;
pub mod session_plan_prd;
pub mod source_path;
pub mod stream;
pub mod toolcall;
pub mod workflow;
pub mod worktree;

pub use backend::{
    backend_from_label, backend_selection_question, build_claude_args, clear_child_pid,
    default_model_for_agent, get_child_pid, kill_child_process, preselected_index_for_agent,
    set_child_pid, AgentOutputSink, AnyBackend, ClarificationQuestion, ClaudeAcpBackend,
    ClaudeCodeBackend, ClaudeInvokeConfig, CodingBackend, CursorBackend, GoalHints, GoalId,
    InMemoryToolExecutor, InvokeRequest, InvokeResponse, MockBackend, PermissionHint,
    PermissionMode, ProcessToolExecutor, QuestionOption, SessionMode, SharedBackend, StubBackend,
    ToolExecutor, WorkflowRecipe,
};
pub use changeset::{
    append_session_and_update_state, get_session_for_tag, read_changeset,
    resolve_agent_from_changeset, resolve_model, update_state, write_changeset, Changeset,
    ChangesetState, ClarificationQa, ClarificationQuestionForQa, DiscoveryData,
    QuestionOptionForQa, SessionEntry, StateTransition,
};
pub use elapsed_format::format_elapsed_compact;
pub use error::{BackendError, ParseError, WorkflowError};
pub use log_backend::{
    config_has_file_output, default_log_config, find_matching_policy, get_buffered_logs,
    init_tddy_logger, init_tddy_logger_legacy, matches_selector, redirect_debug_output,
    resolve_log_defaults, resolve_logger, take_buffered_logs, DefaultLogPolicy, LogConfig,
    LogOutput, LogPolicy, LogRotation, LogSelector, LoggerDefinition, MatchedPolicy,
};
pub use presenter::{
    ActivityEntry, ActivityKind, AppMode, ExitAction, PendingWorkflowStart, Presenter,
    PresenterEvent, PresenterHandle, PresenterState, PresenterView, UserIntent, ViewConnection,
    WorkflowCompletePayload, WorkflowEvent,
};
pub use session_metadata::{
    read_session_metadata, write_session_metadata, SessionMetadata, SESSION_METADATA_FILENAME,
};
pub use session_plan_prd::plan_prd_path_for_session_dir;
pub use source_path::{classify_rust_source_path, RustSourcePathKind};
pub use stream::ProgressEvent;
pub use workflow::{
    engine::WorkflowEngine,
    find_git_root,
    graph::{ElicitationEvent, ExecutionResult, ExecutionStatus},
    ids::WorkflowState,
    session::{workflow_engine_storage_dir, WORKFLOW_ENGINE_STORAGE_SUBDIR},
    GoalOptions,
};
pub use worktree::{
    create_worktree, fetch_origin_master, list_worktrees, remove_worktree,
    setup_worktree_for_session, worktree_dir, WorktreeInfo,
};
