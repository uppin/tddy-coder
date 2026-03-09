//! RunnerHooks — callbacks for FlowRunner around task execution.
//!
//! Hooks handle file I/O and event emission. Called before and after each task run.

use crate::workflow::context::Context;
use crate::workflow::task::TaskResult;
use std::error::Error;

/// Hooks invoked by FlowRunner around task execution.
/// Implementations handle file I/O (read artifacts into context, write outputs) and event emission.
pub trait RunnerHooks: Send + Sync {
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

    /// Called when a task fails. Use for logging or error reporting.
    fn on_error(&self, task_id: &str, error: &(dyn Error + Send + Sync));
}
