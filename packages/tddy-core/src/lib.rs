//! Core library for tddy-coder.

pub mod backend;
pub mod error;
pub mod output;
pub mod stream;
pub mod workflow;

pub use backend::{
    build_claude_args, ClarificationQuestion, ClaudeCodeBackend, CodingBackend, InvokeRequest,
    InvokeResponse, MockBackend, PermissionMode, QuestionOption,
};
pub use error::{BackendError, ParseError, WorkflowError};
pub use output::{
    parse_acceptance_tests_response, parse_planning_output, read_session_file, write_artifacts,
    write_session_file, AcceptanceTestInfo, AcceptanceTestsOutput, PlanningOutput,
};
pub use stream::ProgressEvent;
pub use workflow::{Workflow, WorkflowState};
