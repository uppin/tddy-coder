//! Task trait and related types — graph-flow-compatible API.
//!
//! Mirrors [graph-flow task.rs](https://github.com/a-agmon/rs-graph-llm/blob/main/graph-flow/src/task.rs).

use crate::backend::{CodingBackend, Goal, InvokeRequest};
use crate::workflow::context::Context;
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;

/// Next action after a task completes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NextAction {
    /// Advance one edge, return control to runner.
    Continue,
    /// Advance and keep running (execute next task immediately).
    ContinueAndExecute,
    /// Pause for user input (e.g. clarification answers).
    WaitForInput,
    /// Workflow complete.
    End,
    /// Jump to a specific task by id.
    GoTo(String),
    /// Return to previous task.
    GoBack,
}

/// Result of running a task.
#[derive(Debug, Clone)]
pub struct TaskResult {
    pub response: String,
    pub next_action: NextAction,
    pub task_id: String,
    pub status_message: Option<String>,
}

/// Task trait — async execution with context.
#[async_trait]
pub trait Task: Send + Sync {
    fn id(&self) -> &str;

    async fn run(
        &self,
        context: Context,
    ) -> Result<TaskResult, Box<dyn std::error::Error + Send + Sync>>;
}

/// Simple task for testing — echoes a value from context.
#[derive(Clone)]
pub struct EchoTask {
    id: String,
}

impl EchoTask {
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }
}

#[async_trait]
impl Task for EchoTask {
    fn id(&self) -> &str {
        &self.id
    }

    async fn run(
        &self,
        context: Context,
    ) -> Result<TaskResult, Box<dyn std::error::Error + Send + Sync>> {
        let input: Option<String> = context.get_sync("input");
        let response = input.unwrap_or_else(|| "no input".to_string());
        Ok(TaskResult {
            response: response.clone(),
            next_action: NextAction::Continue,
            task_id: self.id.clone(),
            status_message: Some(response),
        })
    }
}

/// Task that always fails. Used for testing error propagation and on_error hooks.
#[derive(Clone)]
pub struct FailingTask {
    id: String,
}

impl FailingTask {
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }
}

#[async_trait]
impl Task for FailingTask {
    fn id(&self) -> &str {
        &self.id
    }

    async fn run(
        &self,
        _context: Context,
    ) -> Result<TaskResult, Box<dyn std::error::Error + Send + Sync>> {
        Err("FailingTask always fails".into())
    }
}

/// Task that signals workflow completion.
#[derive(Clone)]
pub struct EndTask {
    id: String,
}

impl EndTask {
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }
}

#[async_trait]
impl Task for EndTask {
    fn id(&self) -> &str {
        &self.id
    }

    async fn run(
        &self,
        _context: Context,
    ) -> Result<TaskResult, Box<dyn std::error::Error + Send + Sync>> {
        Ok(TaskResult {
            response: "Workflow complete.".to_string(),
            next_action: NextAction::End,
            task_id: self.id.clone(),
            status_message: Some("Complete.".to_string()),
        })
    }
}

/// Task that invokes the backend for a given goal. Used by tddy-demo and workflow tests.
#[derive(Clone)]
pub struct BackendInvokeTask {
    id: String,
    goal: Goal,
    backend: Arc<dyn CodingBackend>,
}

impl BackendInvokeTask {
    pub fn new(id: impl Into<String>, goal: Goal, backend: Arc<dyn CodingBackend>) -> Self {
        Self {
            id: id.into(),
            goal,
            backend,
        }
    }
}

#[async_trait]
impl Task for BackendInvokeTask {
    fn id(&self) -> &str {
        &self.id
    }

    async fn run(
        &self,
        context: Context,
    ) -> Result<TaskResult, Box<dyn std::error::Error + Send + Sync>> {
        let prompt: String = context
            .get_sync("feature_input")
            .or_else(|| context.get_sync("prompt"))
            .unwrap_or_else(|| "Add a feature".to_string());

        let plan_dir: Option<PathBuf> = context.get_sync("plan_dir");
        let working_dir = plan_dir
            .or_else(|| context.get_sync::<PathBuf>("output_dir"))
            .clone();
        let is_resume = context.get_sync::<String>("answers").is_some();

        let request = InvokeRequest {
            prompt: prompt.clone(),
            system_prompt: context.get_sync("system_prompt"),
            system_prompt_path: None,
            goal: self.goal,
            model: context.get_sync("model"),
            session_id: context.get_sync("session_id"),
            is_resume,
            working_dir,
            debug: context.get_sync::<bool>("debug").unwrap_or(false),
            agent_output: context.get_sync::<bool>("agent_output").unwrap_or(false),
            agent_output_sink: None,
            conversation_output_path: context.get_sync("conversation_output_path"),
            inherit_stdin: context.get_sync::<bool>("inherit_stdin").unwrap_or(false),
            extra_allowed_tools: context.get_sync("allowed_tools"),
        };

        let response = self
            .backend
            .invoke(request)
            .await
            .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;

        context.set_sync("output", response.output.clone());
        if let Some(sid) = &response.session_id {
            context.set_sync("session_id", sid.clone());
        }

        let next_action = if response.questions.is_empty() {
            NextAction::Continue
        } else {
            context.set_sync("pending_questions", response.questions.clone());
            NextAction::WaitForInput
        };

        Ok(TaskResult {
            response: response.output,
            next_action,
            task_id: self.id.clone(),
            status_message: Some(format!("{} step complete", self.id)),
        })
    }
}
