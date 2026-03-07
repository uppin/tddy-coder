//! Workflow state machine for tddy-coder.

mod acceptance_tests;
mod planning;

use crate::backend::CodingBackend;
use crate::error::WorkflowError;
use crate::output::{
    parse_acceptance_tests_response, parse_planning_response, read_session_file,
    slugify_directory_name, write_artifacts, write_session_file,
};
use std::path::{Path, PathBuf};

/// Workflow state.
#[derive(Debug, Clone)]
pub enum WorkflowState {
    Init,
    Planning,
    Planned {
        output_dir: PathBuf,
    },
    AcceptanceTesting,
    AcceptanceTestsReady {
        output: crate::output::AcceptanceTestsOutput,
    },
    Failed {
        error: String,
    },
}

/// Workflow orchestrator with a coding backend.
#[derive(Debug)]
pub struct Workflow<B: CodingBackend> {
    state: WorkflowState,
    backend: B,
    session_id: Option<String>,
}

impl<B: CodingBackend> Workflow<B> {
    /// Create a new workflow in Init state.
    pub fn new(backend: B) -> Self {
        Self {
            state: WorkflowState::Init,
            backend,
            session_id: None,
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
        agent_output: bool,
        inherit_stdin: bool,
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

        let (session_id, is_resume) = match &self.session_id {
            None => {
                let sid = uuid::Uuid::new_v4().to_string();
                self.session_id = Some(sid.clone());
                (Some(sid), false)
            }
            Some(sid) => (Some(sid.clone()), answers.is_some()),
        };

        let request = crate::backend::InvokeRequest {
            prompt,
            system_prompt: Some(system_prompt),
            permission_mode: crate::backend::PermissionMode::Plan,
            model,
            session_id,
            is_resume,
            agent_output,
            inherit_stdin,
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

        if !response.questions.is_empty() {
            if !response.session_id.is_empty() {
                self.session_id = Some(response.session_id.clone());
            }
            return Err(WorkflowError::ClarificationNeeded {
                questions: response.questions,
                session_id: response.session_id,
            });
        }

        let planning = match parse_planning_response(&response.output) {
            Ok(out) => out,
            Err(e) => {
                eprintln!(
                    "--- Failed parse input (length {} bytes) ---",
                    response.output.len()
                );
                eprintln!("{}", response.output);
                eprintln!("--- End failed parse input ---");
                self.state = WorkflowState::Failed {
                    error: e.to_string(),
                };
                return Err(WorkflowError::ParseError(e));
            }
        };

        let dir_name = slugify_directory_name(input);
        let output_path = output_dir.join(&dir_name);

        write_artifacts(&output_path, &planning)?;

        if let Some(ref sid) = self.session_id {
            write_session_file(&output_path, sid)?;
        }

        self.state = WorkflowState::Planned {
            output_dir: output_path.clone(),
        };

        Ok(output_path)
    }

    /// Run the acceptance-tests step: read plan from plan_dir, resume session, create failing tests.
    /// When `answers` is `None`, performs first invoke; when backend returns questions,
    /// returns `ClarificationNeeded`. Call again with `Some(answers)` to continue.
    pub fn acceptance_tests(
        &mut self,
        plan_dir: &Path,
        model: Option<String>,
        agent_output: bool,
        inherit_stdin: bool,
        answers: Option<&str>,
    ) -> Result<crate::output::AcceptanceTestsOutput, WorkflowError> {
        let can_start = matches!(self.state, WorkflowState::Init)
            || matches!(self.state, WorkflowState::Planned { .. });
        let can_continue =
            matches!(self.state, WorkflowState::AcceptanceTesting) && answers.is_some();

        if !can_start && !can_continue {
            return Err(WorkflowError::InvalidTransition(format!(
                "cannot run acceptance_tests from {:?}",
                self.state
            )));
        }

        let prd_path = plan_dir.join("PRD.md");
        if !prd_path.exists() {
            return Err(WorkflowError::PlanDirInvalid(
                "PRD.md not found in plan directory".into(),
            ));
        }

        let mut session_id = read_session_file(plan_dir)?;
        let prd_content = std::fs::read_to_string(&prd_path)
            .map_err(|e| WorkflowError::PlanDirInvalid(e.to_string()))?;

        if can_start {
            self.state = WorkflowState::AcceptanceTesting;
        }

        let system_prompt = acceptance_tests::system_prompt();
        let prompt = match answers {
            None => acceptance_tests::build_prompt(&prd_content),
            Some(a) => acceptance_tests::build_followup_prompt(&prd_content, a),
        };

        let request = crate::backend::InvokeRequest {
            prompt,
            system_prompt: Some(system_prompt),
            permission_mode: crate::backend::PermissionMode::AcceptEdits,
            model,
            session_id: Some(session_id.clone()),
            is_resume: true,
            agent_output,
            inherit_stdin,
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

        if !response.questions.is_empty() {
            if !response.session_id.is_empty() {
                session_id = response.session_id.clone();
                let _ = write_session_file(plan_dir, &session_id);
            }
            return Err(WorkflowError::ClarificationNeeded {
                questions: response.questions,
                session_id: response.session_id,
            });
        }

        let output = match parse_acceptance_tests_response(&response.output) {
            Ok(out) => out,
            Err(e) => {
                eprintln!(
                    "--- Failed parse acceptance tests output (length {} bytes) ---",
                    response.output.len()
                );
                eprintln!("{}", response.output);
                eprintln!("--- End failed parse ---");
                self.state = WorkflowState::Failed {
                    error: e.to_string(),
                };
                return Err(WorkflowError::ParseError(e));
            }
        };

        self.state = WorkflowState::AcceptanceTestsReady {
            output: output.clone(),
        };

        Ok(output)
    }
}
