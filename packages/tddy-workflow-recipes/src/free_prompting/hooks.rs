//! Hooks for [`super::FreePromptingRecipe`].

use std::error::Error;
use std::sync::mpsc;

use tddy_core::backend::AgentOutputSink;
use tddy_core::presenter::WorkflowEvent;
use tddy_core::workflow::context::Context;
use tddy_core::workflow::hooks::RunnerHooks;
use tddy_core::workflow::task::TaskResult;

/// Hooks for the free-prompting loop.
#[derive(Debug)]
pub struct FreePromptingWorkflowHooks {
    event_tx: Option<mpsc::Sender<WorkflowEvent>>,
}

impl FreePromptingWorkflowHooks {
    pub fn new(event_tx: Option<mpsc::Sender<WorkflowEvent>>) -> Self {
        log::debug!(
            "FreePromptingWorkflowHooks::new event_tx={}",
            event_tx.is_some()
        );
        Self { event_tx }
    }
}

impl RunnerHooks for FreePromptingWorkflowHooks {
    fn agent_output_sink(&self) -> Option<AgentOutputSink> {
        self.event_tx.as_ref().map(|tx| {
            let tx = tx.clone();
            AgentOutputSink::new(move |s: &str| {
                let _ = tx.send(WorkflowEvent::AgentOutput(s.to_string()));
            })
        })
    }

    fn before_task(
        &self,
        task_id: &str,
        _context: &Context,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        log::debug!("[free-prompting hooks] before_task: {}", task_id);
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
