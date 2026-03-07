//! Core library for tddy-coder.

pub mod backend;
pub mod error;
pub mod output;
pub mod workflow;

pub use backend::{
    build_claude_args, ClaudeCodeBackend, CodingBackend, InvokeRequest, InvokeResponse, MockBackend,
    PermissionMode,
};
pub use error::{BackendError, ParseError, WorkflowError};
pub use output::{parse_planning_output, write_artifacts, PlanningOutput};
pub use workflow::{Workflow, WorkflowState};
