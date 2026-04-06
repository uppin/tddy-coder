//! Workflow hooks for **merge-pr** (`sync-main` → `finalize` → `end`).

use std::error::Error;
use std::sync::mpsc;

use tddy_core::presenter::WorkflowEvent;
use tddy_core::workflow::context::Context;
use tddy_core::workflow::hooks::RunnerHooks;
use tddy_core::workflow::task::TaskResult;

use super::{TASK_FINALIZE, TASK_SYNC_MAIN};

/// Hooks for merge-pr: git context injection (Green) mirrors `ReviewWorkflowHooks` patterns.
#[derive(Debug)]
pub struct MergePrWorkflowHooks {
    event_tx: Option<mpsc::Sender<WorkflowEvent>>,
}

impl MergePrWorkflowHooks {
    pub fn new(event_tx: Option<mpsc::Sender<WorkflowEvent>>) -> Self {
        Self { event_tx }
    }
}

impl RunnerHooks for MergePrWorkflowHooks {
    fn before_task(
        &self,
        task_id: &str,
        _context: &Context,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        eprintln!(
            "{{\"tddy\":{{\"marker_id\":\"M002\",\"scope\":\"merge_pr::hooks::MergePrWorkflowHooks::before_task\",\"data\":{{\"task_id\":\"{task_id}\"}}}}}}"
        );
        if matches!(task_id, TASK_SYNC_MAIN | TASK_FINALIZE) {
            log::debug!("[merge-pr hooks] before_task task_id={task_id}");
        }
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

    fn on_error(&self, task_id: &str, _context: &Context, error: &(dyn Error + Send + Sync)) {
        log::warn!("MergePrWorkflowHooks::on_error task={task_id} err={error}");
    }
}
