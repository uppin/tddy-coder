//! Hooks for the bug-fix workflow (demo: reproduce with agent output + questions).

use std::error::Error;
use std::sync::mpsc;

use tddy_core::backend::AgentOutputSink;
use tddy_core::presenter::WorkflowEvent;
use tddy_core::workflow::context::Context;
use tddy_core::workflow::hooks::RunnerHooks;
use tddy_core::workflow::task::TaskResult;

/// Hooks for [`super::BugfixRecipe`]. Emits goal/state events and provides an agent output sink.
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
        context: &Context,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        log::debug!("[bugfix hooks] before_task: {}", task_id);
        if let Some(answers) = context.get_sync::<String>("answers") {
            if !answers.trim().is_empty() {
                log::debug!(
                    "[bugfix hooks] transferring answers to prompt (len={})",
                    answers.len()
                );
                let prompt_with_answers = format!(
                    "Here are the user's answers to clarification questions:\n{}",
                    answers
                );
                context.set_sync("prompt", &prompt_with_answers);
                context.remove_sync("answers");
            }
        }
        if let Some(ref tx) = self.event_tx {
            let _ = tx.send(WorkflowEvent::GoalStarted(task_id.to_string()));
            let _ = tx.send(WorkflowEvent::StateChange {
                from: String::new(),
                to: task_id.to_string(),
            });
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
