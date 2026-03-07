//! Core library for tddy-coder.

pub mod backend;
pub mod changeset;
pub mod error;
pub mod output;
pub mod permission;
pub mod stream;
pub mod workflow;

pub use backend::{
    build_claude_args, AnyBackend, ClarificationQuestion, ClaudeCodeBackend, ClaudeInvokeConfig,
    CodingBackend, CursorBackend, Goal, InvokeRequest, InvokeResponse, MockBackend, PermissionMode,
    QuestionOption,
};
pub use changeset::{
    append_session_and_update_state, get_session_for_tag, next_goal_for_state, read_changeset,
    resolve_model, update_state, write_changeset, Changeset, ChangesetState, ClarificationQa,
    ClarificationQuestionForQa, DiscoveryData, QuestionOptionForQa, SessionEntry, StateTransition,
};
pub use error::{BackendError, ParseError, WorkflowError};
pub use output::{
    parse_acceptance_tests_response, parse_green_response, parse_planning_output,
    parse_red_response, parse_validate_response, read_session_file, write_acceptance_tests_file,
    write_artifacts, write_session_file, write_validation_report, AcceptanceTestInfo,
    AcceptanceTestsOutput, GreenOutput, GreenTestResult, ImplementationInfo, PlanningOutput,
    RedOutput, RedTestInfo, SkeletonInfo, ValidateBuildResult, ValidateChangesetSync,
    ValidateFileAnalyzed, ValidateIssue, ValidateOutput, ValidateTestImpact,
};
pub use permission::{
    acceptance_tests_allowlist, green_allowlist, plan_allowlist, red_allowlist, validate_allowlist,
};
pub use stream::ProgressEvent;
pub use workflow::{
    AcceptanceTestsOptions, GreenOptions, PlanOptions, RedOptions, ValidateOptions, Workflow,
    WorkflowState,
};
