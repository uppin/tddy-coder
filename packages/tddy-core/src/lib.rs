//! Core library for tddy-coder.

pub mod backend;
pub mod changeset;
pub mod error;
pub mod log_backend;
pub mod output;
pub mod permission;
pub mod presenter;
pub mod schema;
pub mod stream;
pub mod toolcall;
pub mod workflow;
pub mod worktree;

pub use backend::{
    build_claude_args, clear_child_pid, get_child_pid, kill_child_process, set_child_pid,
    AgentOutputSink, AnyBackend, ClarificationQuestion, ClaudeCodeBackend, ClaudeInvokeConfig,
    CodingBackend, CursorBackend, Goal, InMemoryToolExecutor, InvokeRequest, InvokeResponse,
    MockBackend, PermissionMode, ProcessToolExecutor, QuestionOption, SharedBackend, StubBackend,
    ToolExecutor,
};
pub use changeset::{
    append_session_and_update_state, get_session_for_tag, next_goal_for_state, read_changeset,
    resolve_model, update_state, write_changeset, Changeset, ChangesetState, ClarificationQa,
    ClarificationQuestionForQa, DiscoveryData, QuestionOptionForQa, SessionEntry, StateTransition,
};
pub use error::{BackendError, ParseError, WorkflowError};
pub use log_backend::{
    get_buffered_logs, init_tddy_logger, redirect_debug_output, resolve_log_defaults,
    take_buffered_logs,
};
pub use output::{
    parse_acceptance_tests_response, parse_demo_response, parse_evaluate_response,
    parse_green_response, parse_planning_response, parse_red_response, parse_refactor_response,
    parse_update_docs_response, parse_validate_subagents_response, read_session_file,
    write_acceptance_tests_file, write_artifacts, write_evaluation_report, write_session_file,
    AcceptanceTestInfo, AcceptanceTestsOutput, DemoOutput, EvaluateAffectedTest, EvaluateBuildResult,
    EvaluateChangedFile, EvaluateChangesetSync, EvaluateFileAnalyzed, EvaluateIssue,
    EvaluateOutput, EvaluateTestImpact, GreenOutput, GreenTestResult, ImplementationInfo,
    PlanningOutput, RedOutput, RedTestInfo, RefactorOutput, SkeletonInfo, UpdateDocsOutput,
    ValidateSubagentsOutput,
};
pub use permission::{
    acceptance_tests_allowlist, demo_allowlist, evaluate_allowlist, green_allowlist,
    plan_allowlist, red_allowlist, refactor_allowlist, update_docs_allowlist,
    validate_subagents_allowlist,
};
pub use presenter::{
    ActivityEntry, ActivityKind, AppMode, Presenter, PresenterEvent, PresenterHandle,
    PresenterState, PresenterView, UserIntent, WorkflowCompletePayload, WorkflowEvent,
};
pub use schema::{
    format_validation_errors, get_schema, schema_file_path, validate_output,
    write_all_schemas_to_dir, write_schema_to_dir, SchemaError,
};
pub use stream::ProgressEvent;
pub use workflow::{
    engine::WorkflowEngine,
    find_git_root,
    graph::{ElicitationEvent, ExecutionResult, ExecutionStatus},
    AcceptanceTestsOptions, DemoOptions, EvaluateOptions, GreenOptions, PlanOptions, RedOptions,
    RefactorOptions, UpdateDocsOptions, ValidateOptions,
};
pub use worktree::{create_worktree, list_worktrees, worktree_dir, WorktreeInfo};
