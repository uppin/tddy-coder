//! The workflow engine: runs a recipe's goal graph against a coding backend, drives agent-led
//! transitions through the controller, caches actions, and chooses the goal a continuing session
//! resumes at.
//!
//! Extracted from `tddy-core`, which re-exports the module and its root items at their old paths.

pub mod workflow;

// The backend, changeset, storage layer and tool-call channels the engine runs over, named at the
// `crate::` paths these modules used inside `tddy-core`.
use tddy_agent_backend::{backend, SharedBackend};
use tddy_changeset::{changeset, session_lifecycle};
use tddy_session_store::error::{ParseError, WorkflowError};
use tddy_session_store::{atomic_file, error};
use tddy_toolcall::toolcall;

pub use workflow::{
    engine::WorkflowEngine,
    find_git_root,
    graph::{ElicitationEvent, ExecutionResult, ExecutionStatus},
    ids::{GoalId, WorkflowState},
    recipe::{GoalHints, PermissionHint, WorkflowRecipe},
    session::{workflow_engine_storage_dir, WORKFLOW_ENGINE_STORAGE_SUBDIR},
    start_goal_for_session_continue, GoalOptions,
};
