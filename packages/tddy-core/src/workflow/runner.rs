//! FlowRunner — load session, execute one step, save session.
//!
//! Mirrors [graph-flow runner.rs](https://github.com/a-agmon/rs-graph-llm/blob/main/graph-flow/src/runner.rs).

use crate::workflow::graph::{ExecutionResult, ExecutionStatus, Graph};
use crate::workflow::hooks::RunnerHooks;
use crate::workflow::session::SessionStorage;
use crate::workflow::task::NextAction;
use std::sync::Arc;

/// Orchestrates load → execute → save cycle for workflow sessions.
pub struct FlowRunner {
    graph: Arc<Graph>,
    storage: Arc<dyn SessionStorage>,
    hooks: Option<Arc<dyn RunnerHooks>>,
}

impl FlowRunner {
    /// Create a FlowRunner without hooks.
    pub fn new(graph: Arc<Graph>, storage: Arc<dyn SessionStorage>) -> Self {
        Self {
            graph,
            storage,
            hooks: None,
        }
    }

    /// Create a FlowRunner with optional hooks for file I/O and event emission.
    pub fn new_with_hooks(
        graph: Arc<Graph>,
        storage: Arc<dyn SessionStorage>,
        hooks: Option<Arc<dyn RunnerHooks>>,
    ) -> Self {
        Self {
            graph,
            storage,
            hooks,
        }
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

        if let Some(ref hooks) = self.hooks {
            hooks.before_task(&session.current_task_id, &ctx)?;
            crate::workflow::agent_output::set_sinks(
                hooks.agent_output_sink(),
                hooks.progress_sink(),
            );
        }

        let result = match task.run(ctx.clone()).await {
            Ok(r) => r,
            Err(e) => {
                crate::workflow::agent_output::clear_sinks();
                if let Some(ref hooks) = self.hooks {
                    hooks.on_error(&session.current_task_id, e.as_ref());
                }
                return Err(e);
            }
        };

        crate::workflow::agent_output::clear_sinks();

        let next_task_id = match &result.next_action {
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
            NextAction::End => {
                if let Some(ref hooks) = self.hooks {
                    hooks.after_task(&session.current_task_id, &ctx, &result)?;
                }
                let mut session = session;
                session.status_message = result.status_message.clone();
                self.storage.save(&session).await?;
                return Ok(ExecutionResult {
                    status: ExecutionStatus::Completed,
                    session_id: session_id.to_string(),
                    current_task_id: None,
                });
            }
            NextAction::Continue | NextAction::ContinueAndExecute => self
                .graph
                .next_task_id(&session.current_task_id, &ctx)
                .ok_or("No next task")?,
            NextAction::GoTo(id) => id.clone(),
            NextAction::GoBack => return Err("GoBack not implemented".into()),
        };

        if let Some(ref hooks) = self.hooks {
            hooks.after_task(&session.current_task_id, &ctx, &result)?;
        }

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
