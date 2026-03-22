//! Minimal hooks for the bug-fix stub workflow (no file I/O).

use std::error::Error;

use tddy_core::workflow::context::Context;
use tddy_core::workflow::hooks::RunnerHooks;
use tddy_core::workflow::task::TaskResult;

/// No-op hooks for [`super::BugfixRecipe`].
#[derive(Debug, Default)]
pub struct BugfixWorkflowHooks;

impl RunnerHooks for BugfixWorkflowHooks {
    fn before_task(
        &self,
        _task_id: &str,
        _context: &Context,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
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
