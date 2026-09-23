//! The public paths consumers name through `tddy-core` still resolve.
//!
//! `tddy-core` is becoming a wiring point: every module's body moves to a crate of its own and
//! `tddy-core` keeps a `pub use` facade in its place, so the several hundred consumer sites that name
//! `tddy_core::<module>::…` compile unedited. A move that forgets one facade breaks a consumer this
//! crate's own tests never compile, so this names a representative path from every group — module
//! paths and root re-exports both — and holds before the move and after it.

use tddy_core::agent_skills::DiscoveredSkill;
use tddy_core::backend::CodingBackend;
use tddy_core::changeset::{read_changeset, Changeset, Stack};
use tddy_core::log_backend::{init_tddy_logger, LogConfig};
use tddy_core::presenter::{Presenter, PresenterEvent};
use tddy_core::session_action_jobs::{invoke_session_action, SessionActionInvokeOptions};
use tddy_core::session_action_pipeline::SessionActionPipelineError;
use tddy_core::session_metadata::SessionMetadata;
use tddy_core::toolcall::{take_submit_result_for_goal, ToolCallRequest};
use tddy_core::workflow::context::Context;
use tddy_core::workflow::graph::{ExecutionStatus, Graph};
use tddy_core::workflow::hooks::RunnerHooks;
use tddy_core::workflow::recipe::{GoalHints, PermissionHint, WorkflowRecipe};
use tddy_core::workflow::runner::FlowRunner;
use tddy_core::workflow::session::workflow_engine_storage_dir;
use tddy_core::workflow::task::{BackendInvokeTask, TaskResult};
use tddy_core::worktree::setup_worktree_for_session;

/// A function item, named only so the compiler has to resolve its path.
fn resolves<F>(_item: F) {}

/// A type, named only so the compiler has to resolve its path.
fn names_type<T: ?Sized>() {}

#[test]
fn the_public_paths_consumers_use_still_resolve() {
    // Given a path from each group consumers reach through `tddy-core`
    names_type::<DiscoveredSkill>();
    names_type::<dyn CodingBackend>();
    names_type::<Changeset>();
    names_type::<Stack>();
    names_type::<LogConfig>();
    names_type::<Presenter>();
    names_type::<PresenterEvent>();
    names_type::<SessionActionInvokeOptions>();
    names_type::<SessionActionPipelineError>();
    names_type::<SessionMetadata>();
    names_type::<ToolCallRequest>();
    names_type::<Context>();
    names_type::<ExecutionStatus>();
    names_type::<Graph>();
    names_type::<dyn RunnerHooks>();
    names_type::<GoalHints>();
    names_type::<PermissionHint>();
    names_type::<dyn WorkflowRecipe>();
    names_type::<FlowRunner>();
    names_type::<BackendInvokeTask>();
    names_type::<TaskResult>();
    names_type::<tddy_core::ProgressEvent>();
    names_type::<tddy_core::WorkflowEngine>();
    names_type::<tddy_core::StackParentRoute>();
    names_type::<tddy_core::SessionActivityStatus>();
    resolves(read_changeset);
    resolves(init_tddy_logger);
    resolves(invoke_session_action);
    resolves(take_submit_result_for_goal);
    resolves(setup_worktree_for_session);
    resolves(workflow_engine_storage_dir);
    resolves(tddy_core::start_goal_for_session_continue);

    // When they are resolved — which the compiler does to build this test

    // Then every one names a real item: the compile is the test, so there is nothing left to
    // assert at run time — a missing facade fails this file before it ever runs.
}
