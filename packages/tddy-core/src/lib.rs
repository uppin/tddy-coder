//! Core library for tddy-coder.

pub mod backend;
pub mod error;
pub mod output;
pub mod permission;
pub mod stream;
pub mod workflow;

pub use backend::{
    build_claude_args, ClarificationQuestion, ClaudeCodeBackend, CodingBackend, InvokeRequest,
    InvokeResponse, MockBackend, PermissionMode, QuestionOption,
};
pub use error::{BackendError, ParseError, WorkflowError};
pub use output::{
    parse_acceptance_tests_response, parse_planning_output, parse_red_response, read_session_file,
    write_acceptance_tests_file, write_artifacts, write_session_file, AcceptanceTestInfo,
    AcceptanceTestsOutput, PlanningOutput, RedOutput, RedTestInfo, SkeletonInfo,
};
pub use permission::{acceptance_tests_allowlist, plan_allowlist, red_allowlist};
pub use stream::ProgressEvent;
pub use workflow::{AcceptanceTestsOptions, PlanOptions, RedOptions, Workflow, WorkflowState};
