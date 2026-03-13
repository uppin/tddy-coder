//! Full step Tasks — PlanTask, RedTask, GreenTask, etc.
//!
//! These replace BackendInvokeTask with Tasks that perform file I/O, parsing,
//! and changeset updates. Currently stubbed for TDD red phase.

use crate::backend::{CodingBackend, Goal, InvokeRequest};
use crate::error::{BackendError, ParseError, WorkflowError};
use crate::output::{
    create_session_dir_in, create_session_dir_with_id, parse_planning_response,
    slugify_directory_name,
};
use crate::toolcall::take_submit_result_for_goal;
use crate::workflow::context::Context;
use crate::workflow::planning;
use crate::workflow::task::{NextAction, Task, TaskResult};
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;

/// Plan step Task: invokes backend, parses response, writes PRD.md (with TODO section).
pub struct PlanTask {
    backend: Arc<dyn CodingBackend>,
}

impl PlanTask {
    pub fn new(backend: Arc<dyn CodingBackend>) -> Self {
        Self { backend }
    }
}

#[async_trait]
impl Task for PlanTask {
    fn id(&self) -> &str {
        "plan"
    }

    async fn run(
        &self,
        context: Context,
    ) -> Result<TaskResult, Box<dyn std::error::Error + Send + Sync>> {
        let feature_input: String = context
            .get_sync("feature_input")
            .or_else(|| context.get_sync("prompt"))
            .ok_or("PlanTask requires feature_input or prompt in context")?;

        let output_dir: PathBuf = context
            .get_sync("output_dir")
            .ok_or("PlanTask requires output_dir in context")?;

        let feature_input = feature_input.trim();
        if feature_input.is_empty() {
            return Err("empty feature description".into());
        }

        // plan_dir: from context, or session_base/sessions/{session_id}, or output_dir/slug
        let plan_dir: PathBuf = if let Some(p) = context.get_sync::<PathBuf>("plan_dir") {
            p
        } else if let (Some(base), Some(sid)) = (
            context.get_sync::<PathBuf>("session_base"),
            context.get_sync::<String>("session_id"),
        ) {
            create_session_dir_with_id(&base, &sid)
                .map_err(|e| WorkflowError::WriteFailed(e.to_string()))?
        } else if let Some(base) = context.get_sync::<PathBuf>("session_base") {
            create_session_dir_in(&base).map_err(|e| WorkflowError::WriteFailed(e.to_string()))?
        } else {
            output_dir.join(slugify_directory_name(feature_input))
        };
        std::fs::create_dir_all(&plan_dir)
            .map_err(|e| WorkflowError::WriteFailed(e.to_string()))?;
        context.set_sync("plan_dir", plan_dir.clone());

        let refinement_feedback: Option<String> = context.get_sync("refinement_feedback");
        let answers: Option<String> = context.get_sync("answers");
        let (is_resume, prompt) = match (&refinement_feedback, &answers) {
            (Some(fb), _) => (true, planning::build_refinement_prompt(feature_input, fb)),
            (_, Some(a)) => (true, planning::build_followup_prompt(feature_input, a)),
            (None, None) => (false, planning::build_prompt(feature_input)),
        };

        let system_prompt = planning::system_prompt();

        let session = context.get_sync::<String>("session_id").map(|id| {
            if is_resume {
                crate::backend::SessionMode::Resume(id)
            } else {
                crate::backend::SessionMode::Fresh(id)
            }
        });

        // Use output_dir (repo root) as working_dir so agent can discover Cargo.toml, packages/, etc.
        let request = InvokeRequest {
            prompt,
            system_prompt: Some(system_prompt),
            system_prompt_path: None,
            goal: Goal::Plan,
            model: context.get_sync("model"),
            session,
            working_dir: Some(output_dir.clone()),
            debug: context.get_sync::<bool>("debug").unwrap_or(false),
            agent_output: context.get_sync::<bool>("agent_output").unwrap_or(false),
            agent_output_sink: crate::workflow::agent_output::get_agent_sink(),
            progress_sink: crate::workflow::agent_output::get_progress_sink(),
            conversation_output_path: context.get_sync("conversation_output_path"),
            inherit_stdin: context.get_sync::<bool>("inherit_stdin").unwrap_or(false),
            extra_allowed_tools: context.get_sync("allowed_tools"),
            socket_path: context.get_sync("socket_path"),
            plan_dir: context.get_sync("plan_dir"),
        };

        let response = self.backend.invoke(request).await.map_err(
            |e: BackendError| -> Box<dyn std::error::Error + Send + Sync> {
                Box::new(WorkflowError::Backend(e))
            },
        )?;

        let output_to_store = self
            .backend
            .submit_channel()
            .and_then(|ch| ch.take_for_goal("plan"))
            .or_else(|| take_submit_result_for_goal("plan"));

        if let Some(output) = output_to_store {
            context.set_sync("output", output.clone());
            let planning = parse_planning_response(&output).map_err(|e: ParseError| {
                Box::new(WorkflowError::ParseError(e)) as Box<dyn std::error::Error + Send + Sync>
            })?;

            context.set_sync("parsed_planning", planning);
            context.set_sync("plan_dir", plan_dir.clone());
            if let Some(sid) = &response.session_id {
                context.set_sync("session_id", sid.clone());
            }

            return Ok(TaskResult {
                response: format!("Plan complete for {}", plan_dir.display()),
                next_action: NextAction::Continue,
                task_id: "plan".to_string(),
                status_message: Some("Plan complete".to_string()),
            });
        }

        if !response.questions.is_empty() {
            context.set_sync("pending_questions", response.questions.clone());
            return Ok(TaskResult {
                response: response.output,
                next_action: NextAction::WaitForInput,
                task_id: "plan".to_string(),
                status_message: Some("Clarification needed".to_string()),
            });
        }

        Err(Box::new(WorkflowError::ParseError(ParseError::Malformed(
            "Agent finished without calling tddy-tools submit. Ensure tddy-tools is on PATH and the agent follows the system prompt.".into(),
        ))) as Box<dyn std::error::Error + Send + Sync>)
    }
}

/// Red step Task: creates skeleton code and failing tests.
pub struct RedTask {
    _backend: Arc<dyn CodingBackend>,
}

impl RedTask {
    pub fn new(backend: Arc<dyn CodingBackend>) -> Self {
        Self { _backend: backend }
    }
}

#[async_trait]
impl Task for RedTask {
    fn id(&self) -> &str {
        "red"
    }

    async fn run(
        &self,
        _context: Context,
    ) -> Result<TaskResult, Box<dyn std::error::Error + Send + Sync>> {
        Err("RedTask not implemented".into())
    }
}

/// Green step Task: implements production code to make tests pass.
pub struct GreenTask {
    _backend: Arc<dyn CodingBackend>,
}

impl GreenTask {
    pub fn new(backend: Arc<dyn CodingBackend>) -> Self {
        Self { _backend: backend }
    }
}

#[async_trait]
impl Task for GreenTask {
    fn id(&self) -> &str {
        "green"
    }

    async fn run(
        &self,
        _context: Context,
    ) -> Result<TaskResult, Box<dyn std::error::Error + Send + Sync>> {
        Err("GreenTask not implemented".into())
    }
}
