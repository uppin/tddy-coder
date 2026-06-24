//! RunnerHooks — callbacks for FlowRunner around task execution.
//!
//! Hooks handle file I/O and event emission. Called before and after each task run.

use crate::context::Context;
use crate::graph::ElicitationEvent;
use crate::task::TaskResult;
use std::error::Error;

/// Hooks invoked by FlowRunner around task execution.
/// Implementations handle file I/O (read artifacts into context, write outputs) and event emission.
pub trait RunnerHooks: Send + Sync {
    /// Called immediately before `task.run`. Default impl is a no-op.
    /// Override to set up thread-local sinks (e.g. `set_sinks(...)`) for the task.
    fn on_enter_task(&self, _task_id: &str, _context: &Context) {}

    /// Called unconditionally after `task.run` returns (on success AND error, before any
    /// early-return match). Default impl is a no-op.
    /// Override to tear down thread-local sinks (e.g. `clear_sinks()`).
    fn on_exit_task(&self, _task_id: &str, _context: &Context) {}

    /// Called before a task runs. Use to read files into context, emit GoalStarted, etc.
    fn before_task(
        &self,
        task_id: &str,
        context: &Context,
    ) -> Result<(), Box<dyn Error + Send + Sync>>;

    /// Called after a task runs successfully. Use to write artifacts from context, emit StateChange, etc.
    fn after_task(
        &self,
        task_id: &str,
        context: &Context,
        result: &TaskResult,
    ) -> Result<(), Box<dyn Error + Send + Sync>>;

    /// Signal that the workflow should pause for user elicitation after a task.
    fn elicitation_after_task(
        &self,
        _task_id: &str,
        _context: &Context,
        _result: &TaskResult,
    ) -> Option<ElicitationEvent> {
        None
    }

    /// Called when a task fails.
    fn on_error(&self, task_id: &str, context: &Context, error: &(dyn Error + Send + Sync));
}
