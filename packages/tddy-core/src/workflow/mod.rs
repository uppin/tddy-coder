//! Workflow state machine for tddy-coder.

mod acceptance_tests;
mod planning;
mod red;

use crate::backend::CodingBackend;
use crate::error::WorkflowError;
use crate::output::{
    parse_acceptance_tests_response, parse_planning_response, parse_red_response,
    read_session_file, slugify_directory_name, write_acceptance_tests_file, write_artifacts,
    write_progress_file, write_red_output_file, write_session_file,
};
use std::path::{Path, PathBuf};

/// Options for the plan step.
#[derive(Debug, Default)]
pub struct PlanOptions {
    pub model: Option<String>,
    pub agent_output: bool,
    pub inherit_stdin: bool,
    pub allowed_tools_extras: Option<Vec<String>>,
    pub debug: bool,
}

/// Options for the acceptance-tests step.
#[derive(Debug, Default)]
pub struct AcceptanceTestsOptions {
    pub model: Option<String>,
    pub agent_output: bool,
    pub inherit_stdin: bool,
    pub allowed_tools_extras: Option<Vec<String>>,
    pub debug: bool,
}

/// Options for the red step.
#[derive(Debug, Default)]
pub struct RedOptions {
    pub model: Option<String>,
    pub agent_output: bool,
    pub inherit_stdin: bool,
    pub allowed_tools_extras: Option<Vec<String>>,
    pub debug: bool,
}

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
    RedTesting,
    RedTestsReady {
        output: crate::output::RedOutput,
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

    /// Access the backend (e.g. for tests to inspect invocations).
    pub fn backend(&self) -> &B {
        &self.backend
    }

    /// Run the planning step: read feature description, invoke backend, write artifacts.
    /// When `answers` is `None`, performs first invoke; when backend returns questions,
    /// returns `ClarificationNeeded`. Call again with `Some(answers)` to continue.
    /// `options.allowed_tools_extras` is merged with the plan goal's built-in allowlist.
    pub fn plan(
        &mut self,
        input: &str,
        output_dir: &Path,
        answers: Option<&str>,
        options: &PlanOptions,
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

        let mut allowed_tools = crate::permission::plan_allowlist();
        if let Some(extras) = &options.allowed_tools_extras {
            allowed_tools.extend(extras.iter().cloned());
        }

        let request = crate::backend::InvokeRequest {
            prompt,
            system_prompt: Some(system_prompt),
            permission_mode: crate::backend::PermissionMode::Plan,
            model: options.model.clone(),
            session_id,
            is_resume,
            agent_output: options.agent_output,
            inherit_stdin: options.inherit_stdin,
            allowed_tools: Some(allowed_tools),
            permission_prompt_tool: None,
            mcp_config_path: None,
            working_dir: Some(output_dir.to_path_buf()),
            debug: options.debug,
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
    /// `options.allowed_tools_extras` is merged with the acceptance-tests goal's built-in allowlist.
    pub fn acceptance_tests(
        &mut self,
        plan_dir: &Path,
        answers: Option<&str>,
        options: &AcceptanceTestsOptions,
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

        let mut allowed_tools = crate::permission::acceptance_tests_allowlist();
        if let Some(extras) = &options.allowed_tools_extras {
            allowed_tools.extend(extras.iter().cloned());
        }

        let request = crate::backend::InvokeRequest {
            prompt,
            system_prompt: Some(system_prompt),
            permission_mode: crate::backend::PermissionMode::AcceptEdits,
            model: options.model.clone(),
            session_id: Some(session_id.clone()),
            is_resume: true,
            agent_output: options.agent_output,
            inherit_stdin: options.inherit_stdin,
            allowed_tools: Some(allowed_tools),
            permission_prompt_tool: None,
            mcp_config_path: None,
            working_dir: plan_dir.parent().map(std::path::Path::to_path_buf),
            debug: options.debug,
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
            Ok(out) => {
                write_acceptance_tests_file(plan_dir, &out)?;
                out
            }
            Err(e) => {
                eprintln!(
                    "--- Failed parse acceptance tests output (length {} bytes) ---",
                    response.output.len()
                );
                if response.output.is_empty() {
                    eprintln!(
                        "Hint: Empty output can mean Claude Code CLI produced no stream-json content, \
                         or the result event had an empty result field (known bug: anthropics/claude-code#7124). \
                         Ensure you have rebuilt with `cargo build -p tddy-coder` and that the plan directory \
                         has a valid .session file from a prior plan run."
                    );
                } else {
                    eprintln!("{}", response.output);
                }
                match &response.raw_stream {
                    Some(raw) if !raw.is_empty() => {
                        eprintln!(
                            "--- Raw stream from Claude CLI ({} lines) ---",
                            raw.lines().count()
                        );
                        eprintln!("{}", raw);
                        eprintln!("--- End raw stream ---");
                    }
                    _ => {
                        eprintln!(
                            "Raw stream: (empty - no NDJSON lines received from Claude CLI stdout)"
                        );
                    }
                }
                eprintln!("Claude CLI exit code: {}", response.exit_code);
                if let Some(ref stderr) = response.stderr {
                    eprintln!("--- Claude CLI stderr ---");
                    eprintln!("{}", stderr);
                    eprintln!("--- End stderr ---");
                }
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

    /// Run the red step: read PRD and acceptance-tests.md from plan_dir, create skeleton code and failing tests.
    /// When `answers` is `None`, performs first invoke; when backend returns questions,
    /// returns `ClarificationNeeded`. Call again with `Some(answers)` to continue.
    /// Starts a fresh session (does not resume). Uses AcceptEdits permission mode.
    pub fn red(
        &mut self,
        plan_dir: &Path,
        answers: Option<&str>,
        options: &RedOptions,
    ) -> Result<crate::output::RedOutput, WorkflowError> {
        let can_start = matches!(self.state, WorkflowState::Init)
            || matches!(self.state, WorkflowState::Planned { .. })
            || matches!(self.state, WorkflowState::AcceptanceTestsReady { .. });
        let can_continue = matches!(self.state, WorkflowState::RedTesting) && answers.is_some();

        if !can_start && !can_continue {
            return Err(WorkflowError::InvalidTransition(format!(
                "cannot run red from {:?}",
                self.state
            )));
        }

        let prd_path = plan_dir.join("PRD.md");
        if !prd_path.exists() {
            return Err(WorkflowError::PlanDirInvalid(
                "PRD.md not found in plan directory".into(),
            ));
        }

        let at_path = plan_dir.join("acceptance-tests.md");
        if !at_path.exists() {
            return Err(WorkflowError::PlanDirInvalid(
                "acceptance-tests.md not found in plan directory".into(),
            ));
        }

        let prd_content = std::fs::read_to_string(&prd_path)
            .map_err(|e| WorkflowError::PlanDirInvalid(e.to_string()))?;
        let acceptance_tests_content = std::fs::read_to_string(&at_path)
            .map_err(|e| WorkflowError::PlanDirInvalid(e.to_string()))?;

        if can_start {
            self.state = WorkflowState::RedTesting;
        }

        let system_prompt = red::system_prompt();
        let prompt = match answers {
            None => red::build_prompt(&prd_content, &acceptance_tests_content),
            Some(a) => red::build_followup_prompt(&prd_content, &acceptance_tests_content, a),
        };

        let session_id = uuid::Uuid::new_v4().to_string();

        let mut allowed_tools = crate::permission::red_allowlist();
        if let Some(extras) = &options.allowed_tools_extras {
            allowed_tools.extend(extras.iter().cloned());
        }

        let request = crate::backend::InvokeRequest {
            prompt,
            system_prompt: Some(system_prompt),
            permission_mode: crate::backend::PermissionMode::AcceptEdits,
            model: options.model.clone(),
            session_id: Some(session_id.clone()),
            is_resume: false,
            agent_output: options.agent_output,
            inherit_stdin: options.inherit_stdin,
            allowed_tools: Some(allowed_tools),
            permission_prompt_tool: None,
            mcp_config_path: None,
            working_dir: plan_dir.parent().map(std::path::Path::to_path_buf),
            debug: options.debug,
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
            return Err(WorkflowError::ClarificationNeeded {
                questions: response.questions,
                session_id: response.session_id,
            });
        }

        let output = match parse_red_response(&response.output) {
            Ok(out) => {
                let _ = write_red_output_file(plan_dir, &out);
                let _ = write_progress_file(plan_dir, &out);
                out
            }
            Err(e) => {
                self.state = WorkflowState::Failed {
                    error: e.to_string(),
                };
                return Err(WorkflowError::ParseError(e));
            }
        };

        self.state = WorkflowState::RedTestsReady {
            output: output.clone(),
        };

        Ok(output)
    }
}
