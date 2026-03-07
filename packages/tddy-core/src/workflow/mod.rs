//! Workflow state machine for tddy-coder.

mod planning;

use crate::backend::CodingBackend;
use crate::error::WorkflowError;
use crate::output::{
    parse_planning_response, slugify_directory_name, write_artifacts, PlanningResponse,
};
use std::path::{Path, PathBuf};

/// Workflow state.
#[derive(Debug, Clone)]
pub enum WorkflowState {
    Init,
    Planning,
    Planned { output_dir: PathBuf },
    Failed { error: String },
}

/// Workflow orchestrator with a coding backend.
#[derive(Debug)]
pub struct Workflow<B: CodingBackend> {
    state: WorkflowState,
    backend: B,
}

impl<B: CodingBackend> Workflow<B> {
    /// Create a new workflow in Init state.
    pub fn new(backend: B) -> Self {
        Self {
            state: WorkflowState::Init,
            backend,
        }
    }

    /// Current state.
    pub fn state(&self) -> &WorkflowState {
        &self.state
    }

    /// Run the planning step: read feature description, invoke backend, write artifacts.
    /// When `answers` is `None`, performs first invoke; when backend returns questions,
    /// returns `ClarificationNeeded`. Call again with `Some(answers)` to continue.
    pub fn plan(
        &mut self,
        input: &str,
        output_dir: &Path,
        answers: Option<&str>,
        model: Option<String>,
    ) -> Result<PathBuf, WorkflowError> {
        let can_start = matches!(self.state, WorkflowState::Init);
        let can_continue = matches!(self.state, WorkflowState::Planning) && answers.is_some();

        if !can_start && !can_continue {
            return Err(WorkflowError::InvalidTransition(format!(
                "cannot plan from {:?}",
                self.state
            )));
        }

        let input = input.trim();
        if input.is_empty() {
            return Err(WorkflowError::InvalidTransition(
                "empty feature description".into(),
            ));
        }

        if can_start {
            self.state = WorkflowState::Planning;
        }

        let system_prompt = planning::system_prompt();
        let prompt = match answers {
            None => planning::build_prompt(input),
            Some(a) => planning::build_followup_prompt(input, a),
        };

        let request = crate::backend::InvokeRequest {
            prompt,
            system_prompt: Some(system_prompt),
            permission_mode: crate::backend::PermissionMode::Plan,
            model,
        };

        let response = match self.backend.invoke(request) {
            Ok(r) => r,
            Err(e) => {
                self.state = WorkflowState::Failed {
                    error: e.to_string(),
                };
                return Err(WorkflowError::Backend(e));
            }
        };

        let planning_response = match parse_planning_response(&response.output) {
            Ok(r) => r,
            Err(e) => {
                self.state = WorkflowState::Failed {
                    error: e.to_string(),
                };
                return Err(WorkflowError::ParseError(e));
            }
        };

        match planning_response {
            PlanningResponse::Questions { questions } => {
                Err(WorkflowError::ClarificationNeeded { questions })
            }
            PlanningResponse::PlanningOutput { prd, todo } => {
                let planning = crate::output::PlanningOutput { prd, todo };
                let dir_name = slugify_directory_name(input);
                let output_path = output_dir.join(&dir_name);

                write_artifacts(&output_path, &planning)?;

                self.state = WorkflowState::Planned {
                    output_dir: output_path.clone(),
                };

                Ok(output_path)
            }
        }
    }
}
