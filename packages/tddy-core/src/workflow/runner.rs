//! FlowRunner — load session, execute one step, save session.
//!
//! Mirrors [graph-flow runner.rs](https://github.com/a-agmon/rs-graph-llm/blob/main/graph-flow/src/runner.rs).

use crate::workflow::graph::{ExecutionResult, ExecutionStatus, Graph};
use crate::workflow::session::SessionStorage;
use crate::workflow::task::NextAction;
use std::sync::Arc;

/// Orchestrates load → execute → save cycle for workflow sessions.
pub struct FlowRunner {
    graph: Arc<Graph>,
    storage: Arc<dyn SessionStorage>,
}

impl FlowRunner {
    pub fn new(graph: Arc<Graph>, storage: Arc<dyn SessionStorage>) -> Self {
        Self { graph, storage }
    }

    pub async fn run(
        &self,
        session_id: &str,
    ) -> Result<ExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
        let session = self
            .storage
            .get(session_id)
            .await?
            .ok_or("Session not found")?;

        let task = self
            .graph
            .get_task(&session.current_task_id)
            .ok_or("Task not found")?;

        let ctx = session.context.clone();
        let result = task.run(ctx.clone()).await?;

        let next_task_id = match &result.next_action {
            NextAction::End => {
                let mut session = session;
                session.status_message = result.status_message.clone();
                self.storage.save(&session).await?;
                return Ok(ExecutionResult {
                    status: ExecutionStatus::Completed,
                    session_id: session_id.to_string(),
                    current_task_id: None,
                });
            }
            NextAction::WaitForInput => {
                let mut session = session;
                session.status_message = result.status_message.clone();
                self.storage.save(&session).await?;
                return Ok(ExecutionResult {
                    status: ExecutionStatus::WaitingForInput {
                        message: result.status_message,
                    },
                    session_id: session_id.to_string(),
                    current_task_id: Some(session.current_task_id),
                });
            }
            NextAction::Continue | NextAction::ContinueAndExecute => self
                .graph
                .next_task_id(&session.current_task_id, &ctx)
                .ok_or("No next task")?,
            NextAction::GoTo(id) => id.clone(),
            NextAction::GoBack => return Err("GoBack not implemented".into()),
        };

        let mut session = session;
        session.current_task_id = next_task_id.clone();
        session.status_message = result.status_message;
        self.storage.save(&session).await?;

        Ok(ExecutionResult {
            status: ExecutionStatus::Paused {
                message: Some("Step complete".to_string()),
            },
            session_id: session_id.to_string(),
            current_task_id: Some(next_task_id),
        })
    }
}
