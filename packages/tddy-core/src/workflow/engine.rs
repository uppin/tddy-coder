//! WorkflowEngine — create-once infrastructure for graph-flow execution.
//!
//! Holds graph, runner, storage, backend. Provides run_goal() and run_full_workflow().

use crate::backend::{CodingBackend, GoalId, WorkflowRecipe};
use crate::workflow::context::Context;
use crate::workflow::graph::{ExecutionResult, ExecutionStatus, Graph};
use crate::workflow::hooks::RunnerHooks;
use crate::workflow::runner::FlowRunner;
use crate::workflow::session::{Session, SessionStorage};
use crate::SharedBackend;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// Workflow runner session id: use `session_id` from context when set (CLI/daemon/tests), else generate.
fn workflow_session_id_from_context(ctx: &mut Context) -> String {
    ctx.get_sync::<String>("session_id")
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| {
            let id = uuid::Uuid::new_v4().to_string();
            ctx.set_sync("session_id", id.clone());
            id
        })
}

/// Central struct owning create-once infrastructure for workflow execution.
pub struct WorkflowEngine {
    pub recipe: Arc<dyn WorkflowRecipe>,
    pub graph: Arc<Graph>,
    pub storage: Arc<dyn SessionStorage>,
    pub runner: FlowRunner,
    pub backend: SharedBackend,
}

impl WorkflowEngine {
    /// Create a WorkflowEngine for the given recipe (graph + hooks from the recipe).
    ///
    /// Uses `storage_dir` for [`crate::workflow::session::FileSessionStorage`] (typically
    /// [`crate::workflow::session::workflow_engine_storage_dir`] under the artifact session dir).
    /// When `hooks` is `None`, uses `recipe.create_hooks(None)`.
    pub fn new(
        recipe: Arc<dyn WorkflowRecipe>,
        backend: SharedBackend,
        storage_dir: PathBuf,
        hooks: Option<Arc<dyn RunnerHooks>>,
    ) -> Self {
        let backend_arc = backend.as_arc();
        let graph = Arc::new(recipe.build_graph(backend_arc));
        let storage = Arc::new(crate::workflow::session::FileSessionStorage::new(
            storage_dir,
        ));
        let hooks = hooks.unwrap_or_else(|| recipe.create_hooks(None));
        let runner = FlowRunner::new_with_hooks(graph.clone(), storage.clone(), Some(hooks));

        Self {
            recipe,
            graph: graph.clone(),
            storage,
            runner,
            backend,
        }
    }

    /// Run a single goal: create session at target task, populate context, run once.
    pub async fn run_goal(
        &self,
        goal: &GoalId,
        mut context_values: HashMap<String, serde_json::Value>,
    ) -> Result<ExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
        if !context_values.contains_key("backend_name") {
            context_values.insert(
                "backend_name".to_string(),
                serde_json::json!(self.backend.name()),
            );
        }
        let mut ctx = Context::new();
        for (k, v) in context_values {
            ctx.set_sync(&k, v);
        }
        let session_id = workflow_session_id_from_context(&mut ctx);

        let session = Session {
            id: session_id.clone(),
            graph_id: self.graph.id.clone(),
            current_task_id: goal.to_string(),
            status_message: None,
            context: ctx,
        };

        self.storage.save(&session).await?;
        self.runner.run(&session_id).await
    }

    /// Run the full workflow: create session at "plan", loop until Completed, Error, or WaitingForInput.
    pub async fn run_full_workflow(
        &self,
        mut context_values: HashMap<String, serde_json::Value>,
    ) -> Result<ExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
        if !context_values.contains_key("backend_name") {
            context_values.insert(
                "backend_name".to_string(),
                serde_json::json!(self.backend.name()),
            );
        }
        let mut ctx = Context::new();
        for (k, v) in context_values {
            ctx.set_sync(&k, v);
        }
        let session_id = workflow_session_id_from_context(&mut ctx);

        let session = Session {
            id: session_id.clone(),
            graph_id: self.graph.id.clone(),
            current_task_id: self.recipe.start_goal().to_string(),
            status_message: None,
            context: ctx,
        };

        self.storage.save(&session).await?;

        let mut result = self.runner.run(&session_id).await?;
        loop {
            match &result.status {
                ExecutionStatus::Completed | ExecutionStatus::Error(_) => return Ok(result),
                ExecutionStatus::WaitingForInput { .. } => return Ok(result),
                ExecutionStatus::ElicitationNeeded { .. } => return Ok(result),
                ExecutionStatus::Paused { .. } => {
                    result = self.runner.run(&session_id).await?;
                }
            }
        }
    }

    /// Run workflow from a given goal: create session at target task, loop until Completed, Error, or WaitingForInput.
    /// Use when resuming (e.g. from acceptance-tests after plan is done).
    pub async fn run_workflow_from(
        &self,
        goal: &GoalId,
        mut context_values: HashMap<String, serde_json::Value>,
    ) -> Result<ExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
        if !context_values.contains_key("backend_name") {
            context_values.insert(
                "backend_name".to_string(),
                serde_json::json!(self.backend.name()),
            );
        }
        let mut ctx = Context::new();
        for (k, v) in context_values {
            ctx.set_sync(&k, v);
        }
        let session_id = workflow_session_id_from_context(&mut ctx);

        let session = Session {
            id: session_id.clone(),
            graph_id: self.graph.id.clone(),
            current_task_id: goal.to_string(),
            status_message: None,
            context: ctx,
        };

        self.storage.save(&session).await?;

        let mut result = self.runner.run(&session_id).await?;
        loop {
            match &result.status {
                ExecutionStatus::Completed | ExecutionStatus::Error(_) => return Ok(result),
                ExecutionStatus::WaitingForInput { .. } => return Ok(result),
                ExecutionStatus::ElicitationNeeded { .. } => return Ok(result),
                ExecutionStatus::Paused { .. } => {
                    result = self.runner.run(&session_id).await?;
                }
            }
        }
    }

    /// Run one step for an existing session. Used to continue after WaitingForInput.
    pub async fn run_session(
        &self,
        session_id: &str,
    ) -> Result<ExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
        self.runner.run(session_id).await
    }

    /// Update session context with additional values, then save. Use before run_session when resuming after clarification.
    pub async fn update_session_context(
        &self,
        session_id: &str,
        updates: HashMap<String, serde_json::Value>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let session = self
            .storage
            .get(session_id)
            .await?
            .ok_or("Session not found")?;

        for (k, v) in updates {
            session.context.set_sync(&k, v);
        }

        self.storage.save(&session).await?;
        Ok(())
    }

    /// Get a session by id. Used to retrieve context (e.g. session_dir) after a run.
    pub async fn get_session(
        &self,
        session_id: &str,
    ) -> Result<Option<crate::workflow::session::Session>, Box<dyn std::error::Error + Send + Sync>>
    {
        self.storage.get(session_id).await
    }
}
