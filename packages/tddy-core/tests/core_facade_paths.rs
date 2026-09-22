//! The public paths consumers name through `tddy-core` still resolve.
//!
//! `tddy-core` is becoming a wiring point: every module's body moves to a crate of its own and
//! `tddy-core` keeps a `pub use` facade in its place, so the several hundred consumer sites that name
//! `tddy_core::<module>::…` compile unedited. A move that forgets one facade breaks a consumer this
//! crate's own tests never compile, so this names a representative path from every group — module
//! paths and root re-exports both — and holds before the move and after it.

use std::any::type_name;

use tddy_core::agent_skills::DiscoveredSkill;
use tddy_core::backend::CodingBackend;
use tddy_core::changeset::{read_changeset, Changeset, Stack};
use tddy_core::log_backend::{init_tddy_logger, LogConfig};
use tddy_core::presenter::{Presenter, PresenterEvent};
use tddy_core::session_action_jobs::{invoke_session_action, SessionActionInvokeOptions};
use tddy_core::session_action_pipeline::SessionActionPipelineError;
use tddy_core::session_metadata::SessionMetadata;
use tddy_core::toolcall::{take_submit_result_for_goal, ToolCallRequest};
use tddy_core::workflow::recipe::{GoalHints, PermissionHint, WorkflowRecipe};
use tddy_core::worktree::setup_worktree_for_session;

/// A function item, named only so the compiler has to resolve its path.
fn resolves<F>(_item: F) {}

/// A type's path, which is all a resolution guard needs to observe.
fn named<T: ?Sized>() -> &'static str {
    type_name::<T>()
}

#[test]
fn the_public_paths_consumers_use_still_resolve() {
    // Given a path from each group consumers reach through `tddy-core`
    let types = [
        named::<DiscoveredSkill>(),
        named::<dyn CodingBackend>(),
        named::<Changeset>(),
        named::<Stack>(),
        named::<LogConfig>(),
        named::<Presenter>(),
        named::<PresenterEvent>(),
        named::<SessionActionInvokeOptions>(),
        named::<SessionActionPipelineError>(),
        named::<SessionMetadata>(),
        named::<ToolCallRequest>(),
        named::<GoalHints>(),
        named::<PermissionHint>(),
        named::<dyn WorkflowRecipe>(),
        named::<tddy_core::ProgressEvent>(),
        named::<tddy_core::WorkflowEngine>(),
        named::<tddy_core::StackParentRoute>(),
        named::<tddy_core::SessionActivityStatus>(),
    ];
    resolves(read_changeset);
    resolves(init_tddy_logger);
    resolves(invoke_session_action);
    resolves(take_submit_result_for_goal);
    resolves(setup_worktree_for_session);
    resolves(tddy_core::start_goal_for_session_continue);

    // When they are resolved — which the compiler already did to build this test

    // Then every one names a real item
    assert!(types.iter().all(|path| !path.is_empty()));
}
