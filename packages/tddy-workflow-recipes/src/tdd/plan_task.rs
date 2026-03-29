//! Plan step task: invokes backend, parses response, writes PRD.md (with TODO section).

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use tddy_core::backend::{CodingBackend, GoalId, InvokeRequest, WorkflowRecipe};
use tddy_core::error::{BackendError, ParseError, WorkflowError};
use tddy_core::toolcall::take_submit_result_for_goal;
use tddy_core::workflow::context::Context;
use tddy_core::workflow::task::{NextAction, Task, TaskResult};

use super::session_dir_resolve::resolve_existing_session_dir_for_plan;
use crate::parser::parse_planning_response_with_base;
use crate::tdd::planning;
use tddy_core::session_lifecycle::resolve_effective_session_id;

/// Plan step Task: invokes backend, parses response, writes PRD.md (with TODO section).
pub struct PlanTask {
    backend: Arc<dyn CodingBackend>,
    recipe: Arc<dyn WorkflowRecipe>,
}

impl PlanTask {
    pub fn new(backend: Arc<dyn CodingBackend>, recipe: Arc<dyn WorkflowRecipe>) -> Self {
        Self { backend, recipe }
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

        let session_dir: PathBuf = resolve_existing_session_dir_for_plan(&context).map_err(
            |msg| -> Box<dyn std::error::Error + Send + Sync> {
                Box::new(WorkflowError::WriteFailed(msg))
            },
        )?;
        context.set_sync("session_dir", session_dir.clone());

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
                tddy_core::backend::SessionMode::Resume(id)
            } else {
                tddy_core::backend::SessionMode::Fresh(id)
            }
        });

        let bound_process_session_id: Option<String> = context.get_sync("session_id");

        let gid = GoalId::new("plan");
        let hints = self
            .recipe
            .goal_hints(&gid)
            .expect("plan goal must have hints");
        let submit_key = self.recipe.submit_key(&gid);
        let request = InvokeRequest {
            prompt,
            system_prompt: Some(system_prompt),
            system_prompt_path: None,
            goal_id: gid.clone(),
            submit_key,
            hints,
            model: context.get_sync("model"),
            session,
            working_dir: Some(output_dir.clone()),
            debug: context.get_sync::<bool>("debug").unwrap_or(false),
            agent_output: context.get_sync::<bool>("agent_output").unwrap_or(false),
            agent_output_sink: tddy_core::workflow::get_agent_sink(),
            progress_sink: tddy_core::workflow::get_progress_sink(),
            conversation_output_path: context.get_sync("conversation_output_path"),
            inherit_stdin: context.get_sync::<bool>("inherit_stdin").unwrap_or(false),
            extra_allowed_tools: context.get_sync("allowed_tools"),
            socket_path: context.get_sync("socket_path"),
            session_dir: context.get_sync("session_dir"),
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
            let planning = parse_planning_response_with_base(&output, &session_dir).map_err(
                |e: ParseError| {
                    Box::new(WorkflowError::ParseError(e))
                        as Box<dyn std::error::Error + Send + Sync>
                },
            )?;

            context.set_sync("parsed_planning", planning);
            context.set_sync("session_dir", session_dir.clone());
            if let Some(eff) = resolve_effective_session_id(
                bound_process_session_id.as_deref(),
                response.session_id.as_deref(),
            ) {
                log::info!(
                    "PlanTask: engine session_id set to {} (backend reported {:?})",
                    eff,
                    response.session_id
                );
                context.set_sync("session_id", eff);
            }

            return Ok(TaskResult {
                response: format!("Plan complete for {}", session_dir.display()),
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
