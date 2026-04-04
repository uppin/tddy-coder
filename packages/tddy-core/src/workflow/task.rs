//! Task trait and related types — graph-flow-compatible API.
//!
//! Mirrors [graph-flow task.rs](https://github.com/a-agmon/rs-graph-llm/blob/main/graph-flow/src/task.rs).

use crate::backend::{
    CodingBackend, GoalHints, GoalId, InvokeRequest, SessionMode, WorkflowRecipe,
};
use crate::toolcall::take_submit_result_for_goal;
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
    goal_id: GoalId,
    submit_key: GoalId,
    hints: GoalHints,
    backend: Arc<dyn CodingBackend>,
    requires_tddy_tools_submit: bool,
}

impl BackendInvokeTask {
    pub fn new(
        id: impl Into<String>,
        goal_id: GoalId,
        submit_key: GoalId,
        hints: GoalHints,
        backend: Arc<dyn CodingBackend>,
        requires_tddy_tools_submit: bool,
    ) -> Self {
        Self {
            id: id.into(),
            goal_id,
            submit_key,
            hints,
            backend,
            requires_tddy_tools_submit,
        }
    }

    /// Resolve hints and submit key from a [`WorkflowRecipe`].
    pub fn from_recipe(
        id: impl Into<String>,
        goal_id: GoalId,
        recipe: &dyn WorkflowRecipe,
        backend: Arc<dyn CodingBackend>,
    ) -> Self {
        let hints = recipe.goal_hints(&goal_id).unwrap_or_else(|| {
            panic!(
                "WorkflowRecipe {}: missing hints for goal {}",
                recipe.name(),
                goal_id
            )
        });
        let submit_key = recipe.submit_key(&goal_id);
        let requires_tddy_tools_submit = recipe.goal_requires_tddy_tools_submit(&goal_id);
        Self::new(
            id,
            goal_id,
            submit_key,
            hints,
            backend,
            requires_tddy_tools_submit,
        )
    }
}

/// How many backend invokes to allow for a goal when each turn finishes without a relayed
/// `tddy-tools submit` before failing the workflow. Keeps remediation bounded.
const BACKEND_INVOKE_MAX_ATTEMPTS_WITHOUT_SUBMIT: usize = 8;

fn missing_submit_remediation_line(submit_key: &str) -> String {
    format!(
        "Agent finished without calling tddy-tools submit for goal '{}'. Ensure tddy-tools is on PATH and the agent follows the system prompt.",
        submit_key
    )
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
        // Prefer prompt (set by before_* hooks, e.g. followup with answers) over feature_input.
        // Hooks like before_acceptance_tests set prompt when resuming from clarification.
        let prompt: String = context
            .get_sync("prompt")
            .or_else(|| context.get_sync("feature_input"))
            .unwrap_or_else(|| "Add a feature".to_string());

        let session_dir: Option<PathBuf> = context.get_sync("session_dir");
        let working_dir = context
            .get_sync::<PathBuf>("worktree_dir")
            .or_else(|| context.get_sync::<PathBuf>("output_dir"))
            .or_else(|| session_dir.clone());
        // Use Resume when is_resume is true, or when session_id exists but is_resume was cleared
        // (e.g. Evaluate after Green — after_task clears is_resume). Use Fresh only when
        // explicitly creating a new session (before_red, before_acceptance_tests set is_resume=false).
        let is_resume = context.get_sync::<bool>("is_resume").unwrap_or(true);
        let session = context.get_sync::<String>("session_id").map(|id| {
            if is_resume {
                SessionMode::Resume(id)
            } else {
                SessionMode::Fresh(id)
            }
        });

        let key = self.submit_key.as_str();
        let mut prompt_for_invoke = prompt;

        for attempt in 0..BACKEND_INVOKE_MAX_ATTEMPTS_WITHOUT_SUBMIT {
            let request = InvokeRequest {
                prompt: prompt_for_invoke.clone(),
                system_prompt: context.get_sync("system_prompt"),
                system_prompt_path: None,
                goal_id: self.goal_id.clone(),
                submit_key: self.submit_key.clone(),
                hints: self.hints.clone(),
                model: context.get_sync("model"),
                session: session.clone(),
                working_dir: working_dir.clone(),
                debug: context.get_sync::<bool>("debug").unwrap_or(false),
                agent_output: context.get_sync::<bool>("agent_output").unwrap_or(false),
                agent_output_sink: crate::workflow::agent_output::get_agent_sink(),
                progress_sink: crate::workflow::agent_output::get_progress_sink(),
                conversation_output_path: context.get_sync("conversation_output_path"),
                inherit_stdin: context.get_sync::<bool>("inherit_stdin").unwrap_or(false),
                extra_allowed_tools: context.get_sync("allowed_tools"),
                socket_path: context.get_sync("socket_path"),
                session_dir: session_dir.clone(),
            };

            let response = self
                .backend
                .invoke(request)
                .await
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;

            let submit_output = self
                .backend
                .submit_channel()
                .and_then(|ch| ch.take_for_goal(key))
                .or_else(|| take_submit_result_for_goal(key));

            if let Some(output) = submit_output {
                context.set_sync("output", output.clone());
                let prior = context.get_sync::<String>("session_id");
                if let Some(eff) = crate::session_lifecycle::resolve_effective_session_id(
                    prior.as_deref(),
                    response.session_id.as_deref(),
                ) {
                    log::debug!(
                        "BackendInvokeTask {}: session_id {:?} -> {} (backend reported {:?})",
                        self.id,
                        prior,
                        eff,
                        response.session_id
                    );
                    context.set_sync("session_id", eff);
                }
                return Ok(TaskResult {
                    response: output,
                    next_action: NextAction::Continue,
                    task_id: self.id.clone(),
                    status_message: Some(format!("{} step complete", self.id)),
                });
            }

            if !response.questions.is_empty() {
                context.set_sync("pending_questions", response.questions.clone());
                return Ok(TaskResult {
                    response: response.output,
                    next_action: NextAction::WaitForInput,
                    task_id: self.id.clone(),
                    status_message: Some("Clarification needed".to_string()),
                });
            }

            if !self.requires_tddy_tools_submit {
                let prior = context.get_sync::<String>("session_id");
                if let Some(eff) = crate::session_lifecycle::resolve_effective_session_id(
                    prior.as_deref(),
                    response.session_id.as_deref(),
                ) {
                    log::debug!(
                        "BackendInvokeTask {}: no-submit goal session_id {:?} -> {}",
                        self.id,
                        prior,
                        eff
                    );
                    context.set_sync("session_id", eff);
                }
                context.set_sync("output", response.output.clone());
                return Ok(TaskResult {
                    response: response.output,
                    next_action: NextAction::Continue,
                    task_id: self.id.clone(),
                    status_message: Some(format!("{} step complete", self.id)),
                });
            }

            if attempt + 1 >= BACKEND_INVOKE_MAX_ATTEMPTS_WITHOUT_SUBMIT {
                return Err(Box::new(crate::WorkflowError::ParseError(
                    crate::ParseError::Malformed(missing_submit_remediation_line(key)),
                )));
            }

            prompt_for_invoke.push_str("\n\n---\n");
            prompt_for_invoke.push_str(&missing_submit_remediation_line(key));
            prompt_for_invoke.push('\n');
        }

        Err(Box::new(crate::WorkflowError::ParseError(
            crate::ParseError::Malformed(missing_submit_remediation_line(key)),
        )))
    }
}
