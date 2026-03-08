//! Core library for tddy-coder.

pub mod backend;
mod quiet;
pub mod changeset;
pub mod error;
pub mod output;
pub mod permission;
pub mod schema;
pub mod stream;
pub mod workflow;

pub use backend::{
    build_claude_args, clear_child_pid, get_child_pid, kill_child_process, set_child_pid,
    AnyBackend, ClarificationQuestion, ClaudeCodeBackend, ClaudeInvokeConfig, CodingBackend,
    CursorBackend, Goal, InvokeRequest, InvokeResponse, MockBackend, PermissionMode,
    QuestionOption,
};
pub use changeset::{
    append_session_and_update_state, get_session_for_tag, next_goal_for_state, read_changeset,
    resolve_model, update_state, write_changeset, Changeset, ChangesetState, ClarificationQa,
    ClarificationQuestionForQa, DiscoveryData, QuestionOptionForQa, SessionEntry, StateTransition,
};
pub use error::{BackendError, ParseError, WorkflowError};
pub use output::{
    extract_last_structured_block, parse_acceptance_tests_response, parse_evaluate_response,
    parse_green_response, parse_planning_output, parse_red_response,
    parse_validate_refactor_response, parse_validate_response, read_session_file,
    write_acceptance_tests_file, write_artifacts, write_evaluation_report, write_session_file,
    write_validation_report, AcceptanceTestInfo, AcceptanceTestsOutput, EvaluateAffectedTest,
    EvaluateChangedFile, EvaluateOutput, GreenOutput, GreenTestResult, ImplementationInfo,
    PlanningOutput, RedOutput, RedTestInfo, SkeletonInfo, StructuredBlock, ValidateBuildResult,
    ValidateChangesetSync, ValidateFileAnalyzed, ValidateIssue, ValidateOutput,
    ValidateRefactorOutput, ValidateTestImpact,
};
pub use permission::{
    acceptance_tests_allowlist, evaluate_allowlist, green_allowlist, plan_allowlist, red_allowlist,
    validate_allowlist, validate_refactor_allowlist,
};
pub use schema::{
    format_validation_errors, get_schema, schema_file_path, validate_output,
    write_all_schemas_to_dir, write_schema_to_dir, SchemaError,
};
pub use stream::ProgressEvent;
pub use workflow::{
    AcceptanceTestsOptions, EvaluateOptions, GreenOptions, PlanOptions, RedOptions,
    ValidateOptions, ValidateRefactorOptions, Workflow, WorkflowState,
};
