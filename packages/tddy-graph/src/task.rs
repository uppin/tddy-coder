//! Task trait and primitives — graph-flow-compatible API.
//!
//! Mirrors [graph-flow task.rs](https://github.com/a-agmon/rs-graph-llm/blob/main/graph-flow/src/task.rs).
//!
//! Note: `BackendInvokeTask` depends on `tddy-core` types and stays in `tddy-core` as
//! `workflow::backend_invoke_task::BackendInvokeTask` (implements this `Task` trait).

use crate::context::Context;
use async_trait::async_trait;

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

/// Task that returns `WaitForInput`. Used for testing the on_exit_task early-return path.
#[derive(Clone)]
pub struct WaitingTask {
    id: String,
}

impl WaitingTask {
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }
}

#[async_trait]
impl Task for WaitingTask {
    fn id(&self) -> &str {
        &self.id
    }

    async fn run(
        &self,
        _context: Context,
    ) -> Result<TaskResult, Box<dyn std::error::Error + Send + Sync>> {
        Ok(TaskResult {
            response: String::new(),
            next_action: NextAction::WaitForInput,
            task_id: self.id.clone(),
            status_message: Some("Waiting.".to_string()),
        })
    }
}
