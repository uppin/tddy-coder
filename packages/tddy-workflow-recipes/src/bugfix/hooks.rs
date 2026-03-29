//! Minimal hooks for the bug-fix stub workflow (no file I/O).

use std::error::Error;
use std::sync::mpsc;

use tddy_core::presenter::WorkflowEvent;
use tddy_core::workflow::context::Context;
use tddy_core::workflow::hooks::RunnerHooks;
use tddy_core::workflow::task::TaskResult;

/// Hooks for [`super::BugfixRecipe`]. Emits [`WorkflowEvent::GoalStarted`] when `event_tx` is set
/// (TUI / presenter), matching TDD hook behavior for progress display.
#[derive(Debug)]
pub struct BugfixWorkflowHooks {
    event_tx: Option<mpsc::Sender<WorkflowEvent>>,
}

impl BugfixWorkflowHooks {
    pub fn new(event_tx: Option<mpsc::Sender<WorkflowEvent>>) -> Self {
        Self { event_tx }
    }
}

impl RunnerHooks for BugfixWorkflowHooks {
    fn before_task(
        &self,
        task_id: &str,
        _context: &Context,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        log::debug!("[bugfix hooks] before_task: {}", task_id);
        if let Some(ref tx) = self.event_tx {
            let _ = tx.send(WorkflowEvent::GoalStarted(task_id.to_string()));
        }
        Ok(())
    }

    fn after_task(
        &self,
        _task_id: &str,
        _context: &Context,
        _result: &TaskResult,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        Ok(())
    }

    fn on_error(&self, _task_id: &str, _context: &Context, _error: &(dyn Error + Send + Sync)) {}
}
