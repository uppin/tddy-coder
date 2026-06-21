//! RunnerHooks for orchestrate-pr-stack.

use std::error::Error;

use tddy_core::workflow::context::Context;
use tddy_core::workflow::hooks::RunnerHooks;
use tddy_core::workflow::recipe::WorkflowEventSender;
use tddy_core::workflow::task::TaskResult;

pub struct OrchestratePrStackHooks {
    _event_tx: Option<WorkflowEventSender>,
}

impl OrchestratePrStackHooks {
    pub fn new(event_tx: Option<WorkflowEventSender>) -> Self {
        Self { _event_tx: event_tx }
    }
}

impl RunnerHooks for OrchestratePrStackHooks {
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

    fn on_error(
        &self,
        _task_id: &str,
        _context: &Context,
        _error: &(dyn Error + Send + Sync),
    ) {}
}
