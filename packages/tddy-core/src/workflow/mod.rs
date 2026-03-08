//! Workflow state machine for tddy-coder.

mod acceptance_tests;
mod evaluate;
mod green;
mod planning;
mod red;
mod validate;
mod validate_refactor;

use crate::backend::{CodingBackend, InvokeRequest, InvokeResponse};
use crate::changeset::{
    append_session_and_update_state, clarification_qa_from_backend, get_session_for_tag,
    read_changeset, resolve_model, update_state, write_changeset, Changeset,
};
use crate::error::WorkflowError;
use crate::output::{
    extract_last_structured_block, parse_acceptance_tests_response, parse_evaluate_response,
    parse_green_response, parse_planning_response, parse_red_response,
    parse_validate_refactor_response, parse_validate_response, slugify_directory_name,
    update_acceptance_tests_file, update_progress_file, write_acceptance_tests_file,
    write_artifacts, write_demo_results_file, write_evaluation_report, write_progress_file,
    write_red_output_file, write_validation_report,
};
use crate::schema::{
    format_validation_errors, schema_file_path, validate_output, write_schema_to_dir,
};
use std::path::{Path, PathBuf};

/// Emit debug output when parsing agent response fails. Shows output, raw stream, exit code.
/// Suppressed when TDDY_QUIET is set (TUI mode) to avoid corrupting the terminal.
fn emit_parse_failure_debug(response: &InvokeResponse, goal: &str, empty_hint: Option<&str>) {
    if std::env::var("TDDY_QUIET").is_ok() {
        return;
    }
    eprintln!(
        "--- Failed parse {} output (length {} bytes) ---",
        goal,
        response.output.len()
    );
    if response.output.is_empty() {
        if let Some(hint) = empty_hint {
            eprintln!("Hint: {}", hint);
        }
    } else {
        eprintln!("{}", response.output);
    }
    match &response.raw_stream {
        Some(raw) if !raw.is_empty() => {
            eprintln!(
                "--- Raw stream from agent CLI ({} lines) ---",
                raw.lines().count()
            );
            eprintln!("{}", raw);
            eprintln!("--- End raw stream ---");
        }
        _ => {
            eprintln!("Raw stream: (empty - no NDJSON lines received from agent CLI stdout)");
        }
    }
    eprintln!("Agent CLI exit code: {}", response.exit_code);
    if let Some(ref stderr) = response.stderr {
        eprintln!("--- Agent CLI stderr ---");
        eprintln!("{}", stderr);
        eprintln!("--- End stderr ---");
    }
    eprintln!("--- End failed parse ---");
}

/// Options for the plan step.
#[derive(Debug, Default)]
pub struct PlanOptions {
    pub model: Option<String>,
    pub agent_output: bool,
    pub agent_output_sink: Option<crate::backend::AgentOutputSink>,
    pub conversation_output_path: Option<PathBuf>,
    pub inherit_stdin: bool,
    pub allowed_tools_extras: Option<Vec<String>>,
    pub debug: bool,
}

/// Options for the acceptance-tests step.
#[derive(Debug, Default)]
pub struct AcceptanceTestsOptions {
    pub model: Option<String>,
    pub agent_output: bool,
    pub agent_output_sink: Option<crate::backend::AgentOutputSink>,
    pub conversation_output_path: Option<PathBuf>,
    pub inherit_stdin: bool,
    pub allowed_tools_extras: Option<Vec<String>>,
    pub debug: bool,
}

/// Options for the red step.
#[derive(Debug, Default)]
pub struct RedOptions {
    pub model: Option<String>,
    pub agent_output: bool,
    pub agent_output_sink: Option<crate::backend::AgentOutputSink>,
    pub conversation_output_path: Option<PathBuf>,
    pub inherit_stdin: bool,
    pub allowed_tools_extras: Option<Vec<String>>,
    pub debug: bool,
}

/// Options for the green step.
#[derive(Debug)]
pub struct GreenOptions {
    pub model: Option<String>,
    pub agent_output: bool,
    pub agent_output_sink: Option<crate::backend::AgentOutputSink>,
    pub conversation_output_path: Option<PathBuf>,
    pub inherit_stdin: bool,
    pub allowed_tools_extras: Option<Vec<String>>,
    pub debug: bool,
}

impl Default for GreenOptions {
    fn default() -> Self {
        Self {
            model: None,
            agent_output: true,
            agent_output_sink: None,
            conversation_output_path: None,
            inherit_stdin: true,
            allowed_tools_extras: None,
            debug: false,
        }
    }
}

/// Options for the standalone demo step.
#[derive(Debug, Default)]
pub struct DemoOptions {
    pub model: Option<String>,
    pub agent_output: bool,
    pub agent_output_sink: Option<crate::backend::AgentOutputSink>,
    pub conversation_output_path: Option<PathBuf>,
    pub inherit_stdin: bool,
    pub allowed_tools_extras: Option<Vec<String>>,
    pub debug: bool,
}

/// Options for the validate-changes step.
#[derive(Debug, Default)]
pub struct ValidateOptions {
    pub model: Option<String>,
    pub agent_output: bool,
    pub agent_output_sink: Option<crate::backend::AgentOutputSink>,
    pub conversation_output_path: Option<PathBuf>,
    pub inherit_stdin: bool,
    pub allowed_tools_extras: Option<Vec<String>>,
    pub debug: bool,
}

/// Options for the evaluate-changes step (renamed from ValidateOptions).
#[derive(Debug, Default)]
pub struct EvaluateOptions {
    pub model: Option<String>,
    pub agent_output: bool,
    pub agent_output_sink: Option<crate::backend::AgentOutputSink>,
    pub conversation_output_path: Option<PathBuf>,
    pub inherit_stdin: bool,
    pub allowed_tools_extras: Option<Vec<String>>,
    pub debug: bool,
}

/// Options for the validate-refactor step.
#[derive(Debug, Default)]
pub struct ValidateRefactorOptions {
    pub model: Option<String>,
    pub agent_output: bool,
    pub agent_output_sink: Option<crate::backend::AgentOutputSink>,
    pub conversation_output_path: Option<PathBuf>,
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
    GreenImplementing,
    GreenComplete {
        output: crate::output::GreenOutput,
    },
    Validating,
    Validated {
        output: crate::output::ValidateOutput,
    },
    /// In-progress state for the demo step.
    DemoRunning,
    /// Terminal state after demo finishes successfully.
    DemoComplete {
        output: crate::output::DemoOutput,
    },
    /// When user chooses to skip demo.
    DemoSkipped,
    /// In-progress state for the evaluate-changes step.
    Evaluating,
    /// Terminal state after a successful evaluate-changes run.
    Evaluated {
        output: crate::output::EvaluateOutput,
    },
    /// Terminal state after a successful validate-refactor run.
    ValidateRefactorComplete {
        output: crate::output::ValidateRefactorOutput,
    },
    Failed {
        error: String,
    },
}

impl WorkflowState {
    /// Short name for display (e.g. "Init", "Planning", "Planned").
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Init => "Init",
            Self::Planning => "Planning",
            Self::Planned { .. } => "Planned",
            Self::AcceptanceTesting => "AcceptanceTesting",
            Self::AcceptanceTestsReady { .. } => "AcceptanceTestsReady",
            Self::RedTesting => "RedTesting",
            Self::RedTestsReady { .. } => "RedTestsReady",
            Self::GreenImplementing => "GreenImplementing",
            Self::GreenComplete { .. } => "GreenComplete",
            Self::Validating => "Validating",
            Self::Validated { .. } => "Validated",
            Self::DemoRunning => "DemoRunning",
            Self::DemoComplete { .. } => "DemoComplete",
            Self::DemoSkipped => "DemoSkipped",
            Self::Evaluating => "Evaluating",
            Self::Evaluated { .. } => "Evaluated",
            Self::ValidateRefactorComplete { .. } => "ValidateRefactorComplete",
            Self::Failed { .. } => "Failed",
        }
    }
}

/// Callback for state transitions: (from_state, to_state).
pub type OnStateChange = Box<dyn Fn(&str, &str) + Send>;

/// Workflow orchestrator with a coding backend.
pub struct Workflow<B: CodingBackend> {
    state: WorkflowState,
    backend: B,
    session_id: Option<String>,
    on_state_change: Option<OnStateChange>,
    /// Questions from ClarificationNeeded; used on follow-up success to build clarification_qa.
    pending_clarification_questions: Option<Vec<crate::backend::ClarificationQuestion>>,
}

impl<B: CodingBackend + std::fmt::Debug> std::fmt::Debug for Workflow<B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Workflow")
            .field("state", &self.state)
            .field("backend", &self.backend)
            .field("session_id", &self.session_id)
            .field("on_state_change", &self.on_state_change.is_some())
            .field(
                "pending_clarification_questions",
                &self
                    .pending_clarification_questions
                    .as_ref()
                    .map(|q| q.len()),
            )
            .finish()
    }
}

impl<B: CodingBackend> Workflow<B> {
    /// Create a new workflow in Init state.
    pub fn new(backend: B) -> Self {
        Self {
            state: WorkflowState::Init,
            backend,
            session_id: None,
            on_state_change: None,
            pending_clarification_questions: None,
        }
    }

    /// Set callback invoked on each state transition.
    #[must_use]
    pub fn with_on_state_change<F>(mut self, f: F) -> Self
    where
        F: Fn(&str, &str) + Send + 'static,
    {
        self.on_state_change = Some(Box::new(f));
        self
    }

    /// Current state.
    pub fn state(&self) -> &WorkflowState {
        &self.state
    }

    /// Restore state from persisted changeset (e.g. when resuming). Does not invoke on_state_change.
    pub fn restore_state(&mut self, state: WorkflowState) {
        self.state = state;
    }

    /// Access the backend (e.g. for tests to inspect invocations).
    pub fn backend(&self) -> &B {
        &self.backend
    }

    fn set_state(&mut self, new_state: WorkflowState) {
        let old = self.state.display_name();
        self.state = new_state;
        if let Some(ref cb) = self.on_state_change {
            cb(old, self.state.display_name());
        }
    }

    /// Extract JSON from response, validate against schema, retry once on validation failure.
    /// Returns the validated response (original or from retry).
    /// When allow_no_block is true (e.g. plan with delimited fallback), returns Ok(response) if no structured block found.
    fn validate_and_retry<F>(
        &mut self,
        goal_name: &str,
        response: InvokeResponse,
        _plan_dir: &Path,
        allow_no_block: bool,
        build_retry_request: F,
    ) -> Result<InvokeResponse, WorkflowError>
    where
        F: FnOnce(&str) -> InvokeRequest,
    {
        let block = match extract_last_structured_block(&response.output) {
            Ok(b) => b,
            Err(e) => {
                if allow_no_block {
                    return Ok(response);
                }
                emit_parse_failure_debug(&response, goal_name, None);
                return Err(WorkflowError::ParseError(e));
            }
        };

        if let Err(errors) = validate_output(goal_name, block.json) {
            let schema_path = schema_file_path(goal_name)
                .unwrap_or_else(|| format!("schemas/{}.schema.json", goal_name));
            let retry_prompt = format!(
                "Your previous structured output failed JSON Schema validation against `{}`.\n\n\
                 Read the schema file at `{}` and fix the following errors:\n\n{}\n\n\
                 Output ONLY a corrected <structured-response> block with schema=\"{}\".",
                schema_path,
                schema_path,
                format_validation_errors(&errors),
                schema_path
            );
            let retry_request = build_retry_request(&retry_prompt);
            let retry_response = self
                .backend
                .invoke(retry_request)
                .map_err(WorkflowError::Backend)?;
            let retry_block =
                extract_last_structured_block(&retry_response.output).map_err(|e| {
                    emit_parse_failure_debug(&retry_response, goal_name, None);
                    WorkflowError::ParseError(e)
                })?;
            if let Err(retry_errors) = validate_output(goal_name, retry_block.json) {
                emit_parse_failure_debug(
                    &retry_response,
                    goal_name,
                    Some(&format!(
                        "Retry also failed validation: {}",
                        format_validation_errors(&retry_errors)
                    )),
                );
                self.set_state(WorkflowState::Failed {
                    error: format!(
                        "JSON Schema validation failed after retry: {}",
                        format_validation_errors(&retry_errors)
                    ),
                });
                return Err(WorkflowError::ParseError(
                    crate::error::ParseError::Malformed(format_validation_errors(&retry_errors)),
                ));
            }
            return Ok(retry_response);
        }
        Ok(response)
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
            self.set_state(WorkflowState::Planning);
        }

        let system_prompt = planning::system_prompt();
        let dir_name = slugify_directory_name(input);
        let output_path = output_dir.join(&dir_name);
        std::fs::create_dir_all(&output_path)
            .map_err(|e| WorkflowError::WriteFailed(e.to_string()))?;

        let system_prompt_path = output_path.join("system-prompt-plan.md");
        std::fs::write(&system_prompt_path, &system_prompt)
            .map_err(|e| WorkflowError::WriteFailed(e.to_string()))?;

        // Canonicalize so path is absolute; backend runs with cwd=output_path, so relative paths
        // would resolve incorrectly (e.g. ./plan-dir/system-prompt-plan.md → plan-dir/plan-dir/...).
        let system_prompt_path = std::fs::canonicalize(&system_prompt_path).map_err(|e| {
            WorkflowError::WriteFailed(format!("canonicalize system prompt path: {}", e))
        })?;

        let _ = crate::schema::write_all_schemas_to_dir(&output_path);

        let prompt = match answers {
            None => planning::build_prompt(input),
            Some(a) => planning::build_followup_prompt(input, a),
        };
        let prompt = prepend_context_header(prompt, Some(&output_path));
        // Tell agent where schema lives when working_dir is project root (for discovery).
        let schema_hint = format!(
            "\n\nThe plan schema is at `{}/schemas/plan.schema.json` (relative to working directory).",
            dir_name
        );
        let prompt = format!("{}{}", prompt, schema_hint);

        let (session_id, is_resume) = match &self.session_id {
            None => {
                let sid = uuid::Uuid::new_v4().to_string();
                self.session_id = Some(sid.clone());
                (Some(sid), false)
            }
            Some(sid) => (Some(sid.clone()), answers.is_some()),
        };

        // R5: Write a minimal changeset.yaml with state Init before invoking the backend,
        // so if the agent crashes the user's prompt is preserved and the plan dir is resumable.
        if !is_resume {
            let init_changeset = Changeset {
                initial_prompt: Some(input.to_string()),
                ..Changeset::default()
            };
            eprintln!(
                "[tddy-core] plan: writing initial changeset.yaml to {:?} (state=Init)",
                output_path
            );
            let _ = write_changeset(&output_path, &init_changeset);
        }

        let model = options.model.clone();

        // Use project root (output_dir) as working_dir so agent can discover Cargo.toml,
        // packages/, etc. Sandbox blocks parent access when cwd is plan dir.
        let request = crate::backend::InvokeRequest {
            prompt,
            system_prompt: None,
            system_prompt_path: Some(system_prompt_path.clone()),
            goal: crate::backend::Goal::Plan,
            model,
            session_id,
            is_resume,
            working_dir: Some(output_dir.to_path_buf()),
            debug: options.debug,
            agent_output: options.agent_output,
            agent_output_sink: options.agent_output_sink.clone(),
            conversation_output_path: options.conversation_output_path.clone(),
            inherit_stdin: options.inherit_stdin,
            extra_allowed_tools: options.allowed_tools_extras.clone(),
        };

        let response = match self.backend.invoke(request) {
            Ok(r) => r,
            Err(e) => {
                self.set_state(WorkflowState::Failed {
                    error: e.to_string(),
                });
                return Err(WorkflowError::Backend(e));
            }
        };

        if !response.questions.is_empty() {
            if let Some(ref sid) = response.session_id {
                self.session_id = Some(sid.clone());
            }
            self.pending_clarification_questions = Some(response.questions.clone());
            return Err(WorkflowError::ClarificationNeeded {
                questions: response.questions,
                session_id: response.session_id.unwrap_or_default(),
            });
        }

        let session_id_for_retry = response.session_id.clone();
        let model_for_retry = options.model.clone();
        let build_retry_request = |retry_prompt: &str| crate::backend::InvokeRequest {
            prompt: retry_prompt.to_string(),
            system_prompt: None,
            system_prompt_path: Some(system_prompt_path.clone()),
            goal: crate::backend::Goal::Plan,
            model: model_for_retry.clone(),
            session_id: session_id_for_retry.clone(),
            is_resume: true,
            working_dir: Some(output_path.to_path_buf()),
            debug: options.debug,
            agent_output: options.agent_output,
            agent_output_sink: options.agent_output_sink.clone(),
            conversation_output_path: options.conversation_output_path.clone(),
            inherit_stdin: options.inherit_stdin,
            extra_allowed_tools: options.allowed_tools_extras.clone(),
        };
        let validated_response = self.validate_and_retry(
            "plan",
            response,
            &output_path,
            true, // allow delimited fallback
            build_retry_request,
        )?;

        let planning = match parse_planning_response(&validated_response.output) {
            Ok(out) => out,
            Err(e) => {
                if std::env::var("TDDY_QUIET").is_err() {
                    eprintln!(
                        "--- Failed parse input (length {} bytes) ---",
                        validated_response.output.len()
                    );
                    eprintln!(
                        "Hint: The agent must output a <structured-response> block with the actual PRD and TODO content. \
                         Meta-commentary (e.g. 'I've created the PRD...') without the block causes this error. \
                         See the system prompt for the required format."
                    );
                    eprintln!("{}", validated_response.output);
                    eprintln!("--- End failed parse input ---");
                }
                self.set_state(WorkflowState::Failed {
                    error: e.to_string(),
                });
                return Err(WorkflowError::ParseError(e));
            }
        };

        write_artifacts(&output_path, &planning)?;

        // R1: If the agent supplied a valid plan_dir_suggestion, relocate the staging directory.
        let output_path = if let Some(ref disc) = planning.discovery {
            if let Some(ref suggestion) = disc.plan_dir_suggestion {
                log::debug!("[plan] plan_dir_suggestion={:?}", suggestion);
                match relocate_plan_dir(&output_path, suggestion, &dir_name, output_dir) {
                    Ok(new_path) => {
                        log::debug!("[plan] plan dir relocated to {:?}", new_path);
                        new_path
                    }
                    Err(e) => {
                        log::debug!("[plan] relocation failed (keeping staging): {}", e);
                        output_path
                    }
                }
            } else {
                output_path
            }
        } else {
            output_path
        };

        if let Some(ref sid) = self.session_id {
            let clarification_qa = match (self.pending_clarification_questions.take(), answers) {
                (Some(questions), Some(ans)) => clarification_qa_from_backend(questions, ans),
                _ => Vec::new(),
            };
            let mut changeset = Changeset {
                name: planning.name.clone(),
                initial_prompt: Some(input.to_string()),
                clarification_qa,
                discovery: planning.discovery.clone(),
                ..Changeset::default()
            };
            append_session_and_update_state(
                &mut changeset,
                sid.clone(),
                "plan",
                "Planned",
                self.backend.name(),
                Some("system-prompt-plan.md".to_string()),
            );
            write_changeset(&output_path, &changeset)?;
        }

        self.set_state(WorkflowState::Planned {
            output_dir: output_path.clone(),
        });

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

        let changeset = read_changeset(plan_dir)?;
        let session_id = get_session_for_tag(&changeset, "plan").ok_or_else(|| {
            WorkflowError::ChangesetInvalid("no plan session in changeset".into())
        })?;
        let prd_content = std::fs::read_to_string(&prd_path)
            .map_err(|e| WorkflowError::PlanDirInvalid(e.to_string()))?;

        if can_start {
            self.set_state(WorkflowState::AcceptanceTesting);
        }

        let system_prompt = acceptance_tests::system_prompt();
        let prompt = match answers {
            None => acceptance_tests::build_prompt(&prd_content),
            Some(a) => acceptance_tests::build_followup_prompt(&prd_content, a),
        };
        let prompt = prepend_context_header(prompt, Some(plan_dir));

        let model = resolve_model(
            Some(&changeset),
            "acceptance-tests",
            options.model.as_deref(),
        );

        let request = crate::backend::InvokeRequest {
            prompt,
            system_prompt: Some(system_prompt),
            system_prompt_path: None,
            goal: crate::backend::Goal::AcceptanceTests,
            model,
            session_id: Some(session_id.clone()),
            is_resume: true,
            working_dir: Some(plan_dir.to_path_buf()),
            debug: options.debug,
            agent_output: options.agent_output,
            agent_output_sink: options.agent_output_sink.clone(),
            conversation_output_path: options.conversation_output_path.clone(),
            inherit_stdin: options.inherit_stdin,
            extra_allowed_tools: options.allowed_tools_extras.clone(),
        };

        let response = match self.backend.invoke(request) {
            Ok(r) => r,
            Err(e) => {
                self.set_state(WorkflowState::Failed {
                    error: e.to_string(),
                });
                return Err(WorkflowError::Backend(e));
            }
        };

        if !response.questions.is_empty() {
            if let Some(ref sid) = response.session_id {
                let mut cs = read_changeset(plan_dir)?;
                if let Some(last) = cs.sessions.last_mut() {
                    last.id = sid.clone();
                }
                let _ = write_changeset(plan_dir, &cs);
            }
            return Err(WorkflowError::ClarificationNeeded {
                questions: response.questions,
                session_id: response.session_id.unwrap_or_default(),
            });
        }

        let session_id_for_retry = response.session_id.clone();
        let model_for_retry = resolve_model(
            Some(&changeset),
            "acceptance-tests",
            options.model.as_deref(),
        );
        let build_retry_request = |retry_prompt: &str| crate::backend::InvokeRequest {
            prompt: retry_prompt.to_string(),
            system_prompt: Some(acceptance_tests::system_prompt()),
            system_prompt_path: None,
            goal: crate::backend::Goal::AcceptanceTests,
            model: model_for_retry.clone(),
            session_id: session_id_for_retry.clone(),
            is_resume: true,
            working_dir: Some(plan_dir.to_path_buf()),
            debug: options.debug,
            agent_output: options.agent_output,
            agent_output_sink: options.agent_output_sink.clone(),
            conversation_output_path: options.conversation_output_path.clone(),
            inherit_stdin: options.inherit_stdin,
            extra_allowed_tools: options.allowed_tools_extras.clone(),
        };
        let validated_response = self.validate_and_retry(
            "acceptance-tests",
            response,
            plan_dir,
            false,
            build_retry_request,
        )?;

        let output = match parse_acceptance_tests_response(&validated_response.output) {
            Ok(out) => {
                write_acceptance_tests_file(plan_dir, &out)?;
                let mut cs = read_changeset(plan_dir)?;
                let at_session_id = validated_response
                    .session_id
                    .clone()
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                append_session_and_update_state(
                    &mut cs,
                    at_session_id,
                    "acceptance-tests",
                    "AcceptanceTestsReady",
                    self.backend.name(),
                    None,
                );
                let _ = write_changeset(plan_dir, &cs);
                out
            }
            Err(e) => {
                emit_parse_failure_debug(
                    &validated_response,
                    "acceptance-tests",
                    Some(
                        "Empty output can mean the agent produced no stream-json content, \
                         or the result event had an empty result field (known bug: anthropics/claude-code#7124). \
                         Ensure you have rebuilt with `cargo build -p tddy-coder` and that the plan directory \
                         has a valid changeset.yaml from a prior plan run.",
                    ),
                );
                self.set_state(WorkflowState::Failed {
                    error: e.to_string(),
                });
                return Err(WorkflowError::ParseError(e));
            }
        };

        self.set_state(WorkflowState::AcceptanceTestsReady {
            output: output.clone(),
        });

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

        let changeset = read_changeset(plan_dir).ok();
        let model = resolve_model(changeset.as_ref(), "red", options.model.as_deref());

        let prd_content = std::fs::read_to_string(&prd_path)
            .map_err(|e| WorkflowError::PlanDirInvalid(e.to_string()))?;
        let acceptance_tests_content = std::fs::read_to_string(&at_path)
            .map_err(|e| WorkflowError::PlanDirInvalid(e.to_string()))?;

        if can_start {
            self.set_state(WorkflowState::RedTesting);
        }

        let system_prompt = red::system_prompt();
        let prompt = match answers {
            None => red::build_prompt(&prd_content, &acceptance_tests_content),
            Some(a) => red::build_followup_prompt(&prd_content, &acceptance_tests_content, a),
        };
        let prompt = prepend_context_header(prompt, Some(plan_dir));

        let session_id = uuid::Uuid::new_v4().to_string();

        let request = crate::backend::InvokeRequest {
            prompt,
            system_prompt: Some(system_prompt),
            system_prompt_path: None,
            goal: crate::backend::Goal::Red,
            model: model.clone(),
            session_id: Some(session_id.clone()),
            is_resume: false,
            working_dir: Some(plan_dir.to_path_buf()),
            debug: options.debug,
            agent_output: options.agent_output,
            agent_output_sink: options.agent_output_sink.clone(),
            conversation_output_path: options.conversation_output_path.clone(),
            inherit_stdin: options.inherit_stdin,
            extra_allowed_tools: options.allowed_tools_extras.clone(),
        };

        let response = match self.backend.invoke(request) {
            Ok(r) => r,
            Err(e) => {
                self.set_state(WorkflowState::Failed {
                    error: e.to_string(),
                });
                return Err(WorkflowError::Backend(e));
            }
        };

        if !response.questions.is_empty() {
            return Err(WorkflowError::ClarificationNeeded {
                questions: response.questions,
                session_id: response.session_id.unwrap_or_default(),
            });
        }

        let session_id_for_retry = response.session_id.clone();
        let build_retry_request = |retry_prompt: &str| crate::backend::InvokeRequest {
            prompt: retry_prompt.to_string(),
            system_prompt: Some(red::system_prompt()),
            system_prompt_path: None,
            goal: crate::backend::Goal::Red,
            model: model.clone(),
            session_id: session_id_for_retry.clone(),
            is_resume: true,
            working_dir: Some(plan_dir.to_path_buf()),
            debug: options.debug,
            agent_output: options.agent_output,
            agent_output_sink: options.agent_output_sink.clone(),
            conversation_output_path: options.conversation_output_path.clone(),
            inherit_stdin: options.inherit_stdin,
            extra_allowed_tools: options.allowed_tools_extras.clone(),
        };

        let validated_response =
            self.validate_and_retry("red", response, plan_dir, false, build_retry_request)?;

        let output = match parse_red_response(&validated_response.output) {
            Ok(out) => {
                let _ = write_red_output_file(plan_dir, &out);
                let _ = write_progress_file(plan_dir, &out);
                if let Some(mut cs) = changeset {
                    append_session_and_update_state(
                        &mut cs,
                        session_id.clone(),
                        "impl",
                        "RedTestsReady",
                        self.backend.name(),
                        None,
                    );
                    let _ = write_changeset(plan_dir, &cs);
                } else {
                    let mut cs = Changeset::default();
                    append_session_and_update_state(
                        &mut cs,
                        session_id.clone(),
                        "impl",
                        "RedTestsReady",
                        self.backend.name(),
                        None,
                    );
                    let _ = write_changeset(plan_dir, &cs);
                }
                out
            }
            Err(e) => {
                emit_parse_failure_debug(
                    &validated_response,
                    "red",
                    Some(
                        "The agent must output a <structured-response> block with goal:\"red\", summary, tests, skeletons. \
                         Output must be exactly one valid JSON object — no numbers, arrays, or numbered lists.",
                    ),
                );
                self.set_state(WorkflowState::Failed {
                    error: e.to_string(),
                });
                return Err(WorkflowError::ParseError(e));
            }
        };

        self.set_state(WorkflowState::RedTestsReady {
            output: output.clone(),
        });

        Ok(output)
    }

    /// Run the green step: read progress.md and .impl-session from plan_dir, implement production code.
    /// Starts from RedTestsReady (or Init when plan_dir has red output for standalone CLI runs).
    /// Resumes the red session via .impl-session.
    pub fn green(
        &mut self,
        plan_dir: &Path,
        answers: Option<&str>,
        options: &GreenOptions,
    ) -> Result<crate::output::GreenOutput, WorkflowError> {
        let progress_exists = plan_dir.join("progress.md").exists();
        let changeset_exists = plan_dir.join("changeset.yaml").exists();
        let impl_session_exists = plan_dir.join(".impl-session").exists();
        let has_impl_session = changeset_exists || impl_session_exists;
        let can_start = matches!(self.state, WorkflowState::RedTestsReady { .. })
            || (matches!(self.state, WorkflowState::Init) && progress_exists && has_impl_session);
        let can_continue =
            matches!(self.state, WorkflowState::GreenImplementing) && answers.is_some();

        if !can_start && !can_continue {
            let err_msg = if matches!(self.state, WorkflowState::Init)
                && (!progress_exists || !has_impl_session)
            {
                let missing: Vec<&str> = [
                    (!progress_exists).then_some("progress.md"),
                    (!has_impl_session).then_some("changeset.yaml or .impl-session"),
                ]
                .into_iter()
                .flatten()
                .collect();
                format!(
                    "plan directory missing {} — run the red goal first: tddy-coder --goal red --plan-dir <path>",
                    missing.join(" and ")
                )
            } else {
                format!("cannot run green from {:?}", self.state)
            };
            return Err(WorkflowError::InvalidTransition(err_msg));
        }

        let progress_path = plan_dir.join("progress.md");
        if !progress_path.exists() {
            return Err(WorkflowError::PlanDirInvalid(
                "progress.md not found in plan directory".into(),
            ));
        }

        let progress_content = std::fs::read_to_string(&progress_path)
            .map_err(|e| WorkflowError::PlanDirInvalid(e.to_string()))?;

        let session_id = if changeset_exists {
            let cs = read_changeset(plan_dir)?;
            get_session_for_tag(&cs, "impl").ok_or_else(|| {
                WorkflowError::ChangesetInvalid("no impl session in changeset".into())
            })?
        } else {
            std::fs::read_to_string(plan_dir.join(".impl-session"))
                .map_err(|e| WorkflowError::SessionMissing(e.to_string()))?
                .trim()
                .to_string()
        };

        let changeset = read_changeset(plan_dir).ok();
        let model = resolve_model(changeset.as_ref(), "green", options.model.as_deref());

        let prd_content = std::fs::read_to_string(plan_dir.join("PRD.md")).ok();
        let acceptance_tests_content =
            std::fs::read_to_string(plan_dir.join("acceptance-tests.md")).ok();

        if can_start {
            self.set_state(WorkflowState::GreenImplementing);
        }

        let system_prompt = green::system_prompt(false);
        let prompt = match answers {
            None => green::build_prompt(
                &progress_content,
                prd_content.as_deref(),
                acceptance_tests_content.as_deref(),
            ),
            Some(a) => green::build_followup_prompt(
                &progress_content,
                a,
                prd_content.as_deref(),
                acceptance_tests_content.as_deref(),
            ),
        };

        let request = crate::backend::InvokeRequest {
            prompt,
            system_prompt: Some(system_prompt),
            system_prompt_path: None,
            goal: crate::backend::Goal::Green,
            model: model.clone(),
            session_id: Some(session_id.clone()),
            is_resume: true,
            working_dir: Some(plan_dir.to_path_buf()),
            debug: options.debug,
            agent_output: options.agent_output,
            agent_output_sink: options.agent_output_sink.clone(),
            conversation_output_path: options.conversation_output_path.clone(),
            inherit_stdin: options.inherit_stdin,
            extra_allowed_tools: options.allowed_tools_extras.clone(),
        };

        let response = match self.backend.invoke(request) {
            Ok(r) => r,
            Err(e) => {
                self.set_state(WorkflowState::Failed {
                    error: e.to_string(),
                });
                return Err(WorkflowError::Backend(e));
            }
        };

        if !response.questions.is_empty() {
            return Err(WorkflowError::ClarificationNeeded {
                questions: response.questions,
                session_id: response.session_id.unwrap_or_default(),
            });
        }

        let session_id_for_retry = response.session_id.clone();
        let build_retry_request = |retry_prompt: &str| crate::backend::InvokeRequest {
            prompt: retry_prompt.to_string(),
            system_prompt: Some(green::system_prompt(false)),
            system_prompt_path: None,
            goal: crate::backend::Goal::Green,
            model: model.clone(),
            session_id: session_id_for_retry.clone(),
            is_resume: true,
            working_dir: Some(plan_dir.to_path_buf()),
            debug: options.debug,
            agent_output: options.agent_output,
            agent_output_sink: options.agent_output_sink.clone(),
            conversation_output_path: options.conversation_output_path.clone(),
            inherit_stdin: options.inherit_stdin,
            extra_allowed_tools: options.allowed_tools_extras.clone(),
        };
        let validated_response =
            self.validate_and_retry("green", response, plan_dir, false, build_retry_request)?;

        match parse_green_response(&validated_response.output) {
            Ok(out) => {
                let _ = update_progress_file(plan_dir, &out);
                let _ = update_acceptance_tests_file(plan_dir, &out);
                if let Some(ref demo) = out.demo_results {
                    let _ = write_demo_results_file(plan_dir, &demo.summary, demo.steps_completed);
                }
                if out.all_tests_passing() {
                    if let Some(mut cs) = changeset {
                        update_state(&mut cs, "GreenComplete");
                        let _ = write_changeset(plan_dir, &cs);
                    }
                    self.set_state(WorkflowState::GreenComplete {
                        output: out.clone(),
                    });
                    Ok(out)
                } else {
                    let failing: Vec<_> = out
                        .tests
                        .iter()
                        .filter(|t| t.status != "passing")
                        .map(|t| {
                            format!("{}: {}", t.name, t.reason.as_deref().unwrap_or("failing"))
                        })
                        .collect();
                    let err_msg = format!("Some tests still failing: {}", failing.join("; "));
                    self.set_state(WorkflowState::Failed {
                        error: err_msg.clone(),
                    });
                    Err(WorkflowError::InvalidTransition(err_msg))
                }
            }
            Err(e) => {
                emit_parse_failure_debug(
                    &validated_response,
                    "green",
                    Some(
                        "The agent must output a <structured-response> block with goal:\"green\", summary, tests, implementations. \
                         Output must be exactly one valid JSON object — no numbers, arrays, or numbered lists.",
                    ),
                );
                self.set_state(WorkflowState::Failed {
                    error: e.to_string(),
                });
                Err(WorkflowError::ParseError(e))
            }
        }
    }

    /// Run the standalone demo step: execute demo against plan_dir.
    /// Requires demo-plan.md in plan_dir.
    /// Can start from GreenComplete or Init (standalone mode with plan_dir).
    pub fn demo(
        &mut self,
        plan_dir: &Path,
        _answers: Option<&str>,
        options: &DemoOptions,
    ) -> Result<crate::output::DemoOutput, WorkflowError> {
        eprintln!(
            "{{\"tddy\":{{\"marker_id\":\"M002\",\"scope\":\"workflow::demo\",\"data\":{{}}}}}}"
        );

        let can_start = matches!(self.state, WorkflowState::GreenComplete { .. })
            || matches!(self.state, WorkflowState::Init);

        if !can_start {
            return Err(WorkflowError::InvalidTransition(format!(
                "cannot run demo from {:?}",
                self.state
            )));
        }

        let demo_plan_path = plan_dir.join("demo-plan.md");
        if !demo_plan_path.exists() {
            return Err(WorkflowError::PlanDirInvalid(
                "demo-plan.md not found in plan directory".into(),
            ));
        }

        self.set_state(WorkflowState::DemoRunning);

        let _changeset = read_changeset(plan_dir).ok();
        let model = options.model.clone();
        let session_id = uuid::Uuid::new_v4().to_string();

        let demo_plan_content = std::fs::read_to_string(&demo_plan_path)
            .map_err(|e| WorkflowError::PlanDirInvalid(e.to_string()))?;

        let prompt = format!(
            "Execute the demo described in demo-plan.md:\n\n{}",
            demo_plan_content
        );

        let request = crate::backend::InvokeRequest {
            prompt,
            system_prompt: None,
            system_prompt_path: None,
            goal: crate::backend::Goal::Demo,
            model,
            session_id: Some(session_id),
            is_resume: false,
            working_dir: Some(plan_dir.to_path_buf()),
            debug: options.debug,
            agent_output: options.agent_output,
            agent_output_sink: options.agent_output_sink.clone(),
            conversation_output_path: options.conversation_output_path.clone(),
            inherit_stdin: options.inherit_stdin,
            extra_allowed_tools: options.allowed_tools_extras.clone(),
        };

        let response = match self.backend.invoke(request) {
            Ok(r) => r,
            Err(e) => {
                self.set_state(WorkflowState::Failed {
                    error: e.to_string(),
                });
                return Err(WorkflowError::Backend(e));
            }
        };

        let output = match crate::output::parse_demo_response(&response.output) {
            Ok(out) => out,
            Err(e) => {
                self.set_state(WorkflowState::Failed {
                    error: e.to_string(),
                });
                return Err(WorkflowError::ParseError(e));
            }
        };

        self.set_state(WorkflowState::DemoComplete {
            output: output.clone(),
        });

        Ok(output)
    }

    /// Skip the demo step. Transitions from GreenComplete to DemoSkipped.
    pub fn skip_demo(&mut self) {
        eprintln!("{{\"tddy\":{{\"marker_id\":\"M003\",\"scope\":\"workflow::skip_demo\",\"data\":{{}}}}}}");
        self.set_state(WorkflowState::DemoSkipped);
    }

    /// Run the validate-changes step: analyze current git changes for risks.
    /// Requires plan_dir (or plan staging dir) for schemas and validation-report.md.
    /// Reads changeset/PRD context from plan_dir and includes it in the prompt.
    /// Always starts a fresh session (is_resume: false).
    pub fn validate(
        &mut self,
        _working_dir: &std::path::Path,
        plan_dir: Option<&std::path::Path>,
        _answers: Option<&str>,
        options: &ValidateOptions,
    ) -> Result<crate::output::ValidateOutput, WorkflowError> {
        let plan_dir = plan_dir.ok_or_else(|| {
            WorkflowError::PlanDirInvalid(
                "validate-changes requires --plan-dir for schemas and validation-report.md".into(),
            )
        })?;

        let can_start = matches!(self.state, WorkflowState::Init);
        if !can_start {
            return Err(WorkflowError::InvalidTransition(format!(
                "cannot validate from {:?}",
                self.state
            )));
        }

        let prd_owned = std::fs::read_to_string(plan_dir.join("PRD.md")).ok();
        let changeset_owned = std::fs::read_to_string(plan_dir.join("changeset.yaml")).ok();
        let prd_content = prd_owned.as_deref();
        let changeset_content = changeset_owned.as_deref();

        // Schemas and validation-report go to plan_dir
        let _ = write_schema_to_dir(plan_dir, "validate");

        let system_prompt = validate::system_prompt();
        let prompt = validate::build_prompt(prd_content, changeset_content);

        let session_id = uuid::Uuid::new_v4().to_string();
        let model = options.model.clone();

        let request = crate::backend::InvokeRequest {
            prompt,
            system_prompt: Some(system_prompt),
            system_prompt_path: None,
            goal: crate::backend::Goal::Validate,
            model: model.clone(),
            session_id: Some(session_id.clone()),
            is_resume: false,
            working_dir: Some(plan_dir.to_path_buf()),
            debug: options.debug,
            agent_output: options.agent_output,
            agent_output_sink: options.agent_output_sink.clone(),
            conversation_output_path: options.conversation_output_path.clone(),
            inherit_stdin: options.inherit_stdin,
            extra_allowed_tools: options.allowed_tools_extras.clone(),
        };

        let response = match self.backend.invoke(request) {
            Ok(r) => r,
            Err(e) => {
                self.set_state(WorkflowState::Failed {
                    error: e.to_string(),
                });
                return Err(WorkflowError::Backend(e));
            }
        };

        if !response.questions.is_empty() {
            return Err(WorkflowError::ClarificationNeeded {
                questions: response.questions,
                session_id: response.session_id.unwrap_or_default(),
            });
        }

        let session_id_for_retry = response.session_id.clone();
        let build_retry_request = |retry_prompt: &str| crate::backend::InvokeRequest {
            prompt: retry_prompt.to_string(),
            system_prompt: Some(validate::system_prompt()),
            system_prompt_path: None,
            goal: crate::backend::Goal::Validate,
            model: model.clone(),
            session_id: session_id_for_retry.clone(),
            is_resume: true,
            working_dir: Some(plan_dir.to_path_buf()),
            debug: options.debug,
            agent_output: options.agent_output,
            agent_output_sink: options.agent_output_sink.clone(),
            conversation_output_path: options.conversation_output_path.clone(),
            inherit_stdin: options.inherit_stdin,
            extra_allowed_tools: options.allowed_tools_extras.clone(),
        };
        let validated_response =
            self.validate_and_retry("validate", response, plan_dir, false, build_retry_request)?;

        let output = match parse_validate_response(&validated_response.output) {
            Ok(out) => {
                let _ = write_validation_report(plan_dir, &out);
                out
            }
            Err(e) => {
                emit_parse_failure_debug(
                    &validated_response,
                    "validate",
                    Some(
                        "The agent must output a <structured-response> block with goal:\"validate-changes\", summary, risk_level, issues, build_results. \
                         Output must be exactly one valid JSON object — no numbers, arrays, or numbered lists.",
                    ),
                );
                self.set_state(WorkflowState::Failed {
                    error: e.to_string(),
                });
                return Err(WorkflowError::ParseError(e));
            }
        };

        self.set_state(WorkflowState::Validated {
            output: output.clone(),
        });

        Ok(output)
    }

    /// Run the evaluate-changes step: analyze current git changes for risks, changed files,
    /// affected tests, and validity. Writes evaluation-report.md to plan_dir.
    /// plan_dir is required — unlike validate() which allows None.
    /// Always starts a fresh session (is_resume: false).
    pub fn evaluate(
        &mut self,
        _working_dir: &std::path::Path,
        plan_dir: Option<&std::path::Path>,
        _answers: Option<&str>,
        options: &EvaluateOptions,
    ) -> Result<crate::output::EvaluateOutput, WorkflowError> {
        // plan_dir is required: evaluation-report.md is written there
        let plan_dir = plan_dir.ok_or_else(|| {
            WorkflowError::PlanDirInvalid(
                "evaluate-changes requires a plan_dir to write evaluation-report.md".into(),
            )
        })?;

        let can_start = matches!(
            self.state,
            WorkflowState::Init
                | WorkflowState::GreenComplete { .. }
                | WorkflowState::DemoComplete { .. }
                | WorkflowState::DemoSkipped
        );
        if !can_start {
            return Err(WorkflowError::InvalidTransition(format!(
                "cannot evaluate from {:?}",
                self.state
            )));
        }

        // Read optional plan context from plan_dir
        let prd_owned = std::fs::read_to_string(plan_dir.join("PRD.md")).ok();
        let changeset_owned = std::fs::read_to_string(plan_dir.join("changeset.yaml")).ok();
        let prd_content = prd_owned.as_deref();
        let changeset_content = changeset_owned.as_deref();

        eprintln!(
            "[tddy-core] evaluate: prd={}, changeset={}",
            prd_content.is_some(),
            changeset_content.is_some()
        );

        // Schemas and agent cwd: plan_dir (evaluate always has plan_dir)
        let _ = write_schema_to_dir(plan_dir, "evaluate");

        let system_prompt = evaluate::system_prompt();
        let prompt = evaluate::build_prompt(prd_content, changeset_content);

        let session_id = uuid::Uuid::new_v4().to_string();
        let model = options.model.clone();

        let request = crate::backend::InvokeRequest {
            prompt,
            system_prompt: Some(system_prompt),
            system_prompt_path: None,
            goal: crate::backend::Goal::Evaluate,
            model: model.clone(),
            session_id: Some(session_id.clone()),
            is_resume: false,
            working_dir: Some(plan_dir.to_path_buf()),
            debug: options.debug,
            agent_output: options.agent_output,
            agent_output_sink: options.agent_output_sink.clone(),
            conversation_output_path: options.conversation_output_path.clone(),
            inherit_stdin: options.inherit_stdin,
            extra_allowed_tools: options.allowed_tools_extras.clone(),
        };

        let response = match self.backend.invoke(request) {
            Ok(r) => r,
            Err(e) => {
                self.set_state(WorkflowState::Failed {
                    error: e.to_string(),
                });
                return Err(WorkflowError::Backend(e));
            }
        };

        if !response.questions.is_empty() {
            return Err(WorkflowError::ClarificationNeeded {
                questions: response.questions,
                session_id: response.session_id.unwrap_or_default(),
            });
        }

        let session_id_for_retry = response.session_id.clone();
        let build_retry_request = |retry_prompt: &str| crate::backend::InvokeRequest {
            prompt: retry_prompt.to_string(),
            system_prompt: Some(evaluate::system_prompt()),
            system_prompt_path: None,
            goal: crate::backend::Goal::Evaluate,
            model: model.clone(),
            session_id: session_id_for_retry.clone(),
            is_resume: true,
            working_dir: Some(plan_dir.to_path_buf()),
            debug: options.debug,
            agent_output: options.agent_output,
            agent_output_sink: options.agent_output_sink.clone(),
            conversation_output_path: options.conversation_output_path.clone(),
            inherit_stdin: options.inherit_stdin,
            extra_allowed_tools: options.allowed_tools_extras.clone(),
        };
        let validated_response =
            self.validate_and_retry("evaluate", response, plan_dir, false, build_retry_request)?;

        let output = match parse_evaluate_response(&validated_response.output) {
            Ok(out) => {
                let _ = write_evaluation_report(plan_dir, &out);
                out
            }
            Err(e) => {
                emit_parse_failure_debug(
                    &validated_response,
                    "evaluate",
                    Some(
                        "The agent must output a <structured-response> block with goal:\"evaluate-changes\", \
                         changed_files, affected_tests, and validity_assessment fields. \
                         Output must be exactly one valid JSON object.",
                    ),
                );
                self.set_state(WorkflowState::Failed {
                    error: e.to_string(),
                });
                return Err(WorkflowError::ParseError(e));
            }
        };

        self.set_state(WorkflowState::Evaluated {
            output: output.clone(),
        });

        Ok(output)
    }

    /// Run the validate-refactor step: orchestrate validate-tests, validate-prod-ready,
    /// and analyze-clean-code subagents via the Agent tool.
    /// Requires plan_dir to contain evaluation-report.md from a prior evaluate-changes run.
    pub fn validate_refactor(
        &mut self,
        plan_dir: &std::path::Path,
        _answers: Option<&str>,
        options: &ValidateRefactorOptions,
    ) -> Result<crate::output::ValidateRefactorOutput, WorkflowError> {
        // evaluation-report.md is required as input for the validate-refactor orchestrator
        let eval_report_path = plan_dir.join("evaluation-report.md");
        if !eval_report_path.exists() {
            return Err(WorkflowError::PlanDirInvalid(
                "validate-refactor requires evaluation-report.md in plan_dir — \
                 run evaluate-changes first to generate it"
                    .into(),
            ));
        }

        let evaluation_report_content =
            std::fs::read_to_string(&eval_report_path).map_err(|e| {
                WorkflowError::PlanDirInvalid(format!("failed to read evaluation-report.md: {}", e))
            })?;

        eprintln!(
            "[tddy-core] validate_refactor: evaluation-report.md length={}",
            evaluation_report_content.len()
        );

        let system_prompt = validate_refactor::system_prompt();
        let prompt = validate_refactor::build_prompt(&evaluation_report_content);

        let session_id = uuid::Uuid::new_v4().to_string();
        let model = options.model.clone();

        let request = crate::backend::InvokeRequest {
            prompt,
            system_prompt: Some(system_prompt),
            system_prompt_path: None,
            goal: crate::backend::Goal::ValidateRefactor,
            model: model.clone(),
            session_id: Some(session_id.clone()),
            is_resume: false,
            working_dir: Some(plan_dir.to_path_buf()),
            debug: options.debug,
            agent_output: options.agent_output,
            agent_output_sink: options.agent_output_sink.clone(),
            conversation_output_path: options.conversation_output_path.clone(),
            inherit_stdin: options.inherit_stdin,
            extra_allowed_tools: options.allowed_tools_extras.clone(),
        };

        let response = match self.backend.invoke(request) {
            Ok(r) => r,
            Err(e) => {
                self.set_state(WorkflowState::Failed {
                    error: e.to_string(),
                });
                return Err(WorkflowError::Backend(e));
            }
        };

        if !response.questions.is_empty() {
            return Err(WorkflowError::ClarificationNeeded {
                questions: response.questions,
                session_id: response.session_id.unwrap_or_default(),
            });
        }

        let session_id_for_retry = response.session_id.clone();
        let build_retry_request = |retry_prompt: &str| crate::backend::InvokeRequest {
            prompt: retry_prompt.to_string(),
            system_prompt: Some(validate_refactor::system_prompt()),
            system_prompt_path: None,
            goal: crate::backend::Goal::ValidateRefactor,
            model: model.clone(),
            session_id: session_id_for_retry.clone(),
            is_resume: true,
            working_dir: Some(plan_dir.to_path_buf()),
            debug: options.debug,
            agent_output: options.agent_output,
            agent_output_sink: options.agent_output_sink.clone(),
            conversation_output_path: options.conversation_output_path.clone(),
            inherit_stdin: options.inherit_stdin,
            extra_allowed_tools: options.allowed_tools_extras.clone(),
        };
        let validated_response = self.validate_and_retry(
            "validate-refactor",
            response,
            plan_dir,
            false,
            build_retry_request,
        )?;

        let output = match parse_validate_refactor_response(&validated_response.output) {
            Ok(out) => out,
            Err(e) => {
                emit_parse_failure_debug(
                    &validated_response,
                    "validate-refactor",
                    Some(
                        "The agent must output a <structured-response> block with \
                         goal:\"validate-refactor\", summary, tests_report_written, \
                         prod_ready_report_written, clean_code_report_written. \
                         Output must be exactly one valid JSON object.",
                    ),
                );
                self.set_state(WorkflowState::Failed {
                    error: e.to_string(),
                });
                return Err(WorkflowError::ParseError(e));
            }
        };

        self.set_state(WorkflowState::ValidateRefactorComplete {
            output: output.clone(),
        });

        Ok(output)
    }
}

// ── Plan directory relocation helpers (R1, R2, R4) ───────────────────────────

/// Walk up from `dir` looking for a `.git` directory.
/// Falls back to `dir`'s parent if none found (or to `dir` itself if it has no parent).
fn find_git_root(dir: &Path) -> PathBuf {
    let mut current = dir.to_path_buf();
    loop {
        if current.join(".git").exists() {
            log::debug!("[find_git_root] found .git at {:?}", current);
            return current;
        }
        match current.parent() {
            Some(parent) if parent != current => {
                current = parent.to_path_buf();
            }
            _ => break,
        }
    }
    // R2 fallback: return dir's immediate parent (or dir itself if no parent)
    log::debug!(
        "[find_git_root] no .git found, falling back to parent of {:?}",
        dir
    );
    dir.parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| dir.to_path_buf())
}

/// Recursively copy `src` directory to `dst`. Used for cross-device moves (R4).
fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

/// Relocate the plan directory from the staging location to the path suggested by the agent.
///
/// # Arguments
/// * `staging` – current (staging) path of the plan directory
/// * `suggestion` – raw `plan_dir_suggestion` string from the agent
/// * `dir_name` – the bare directory name (e.g. `"2026-03-08-my-feature"`)
/// * `output_dir` – the original output directory (used to find the git root)
///
/// Returns the final path.  On any invalid suggestion the function falls back to `staging`
/// and returns `Ok(staging.to_path_buf())` — it never returns `Err` for validation failures.
fn relocate_plan_dir(
    staging: &Path,
    suggestion: &str,
    dir_name: &str,
    output_dir: &Path,
) -> Result<PathBuf, WorkflowError> {
    // R3: Reject empty / whitespace-only suggestions
    let suggestion = suggestion.trim();
    if suggestion.is_empty() {
        log::debug!("[relocate_plan_dir] empty suggestion → falling back to staging");
        return Ok(staging.to_path_buf());
    }

    // R3: Reject absolute paths
    if std::path::Path::new(suggestion).is_absolute() {
        log::debug!("[relocate_plan_dir] absolute path rejected: {}", suggestion);
        return Ok(staging.to_path_buf());
    }

    // R3: Reject paths containing `..`
    if suggestion.contains("..") {
        log::debug!("[relocate_plan_dir] dotdot path rejected: {}", suggestion);
        return Ok(staging.to_path_buf());
    }

    // R2: Find the git root relative to the output directory
    let git_root = find_git_root(output_dir);
    log::debug!("[relocate_plan_dir] git_root={:?}", git_root);

    // Build the target: git_root / suggestion (stripped trailing slash) / dir_name
    let target = git_root
        .join(suggestion.trim_end_matches('/'))
        .join(dir_name);
    log::debug!(
        "[relocate_plan_dir] staging={:?} target={:?}",
        staging,
        target
    );

    // R3: If the suggestion resolves to the same path as staging → no-op
    if target == staging {
        log::debug!("[relocate_plan_dir] target == staging → no-op");
        return Ok(staging.to_path_buf());
    }

    // R3: If target already exists → error with a clear message
    if target.exists() {
        return Err(WorkflowError::WriteFailed(format!(
            "relocate_plan_dir: target directory already exists: {}",
            target.display()
        )));
    }

    // Create parent directories for the target
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| WorkflowError::WriteFailed(format!("create target parent dirs: {}", e)))?;
    }

    // R4: Try a fast rename first; on cross-device failure fall back to copy+delete
    if std::fs::rename(staging, &target).is_err() {
        log::debug!(
            "[relocate_plan_dir] rename failed (cross-device?), falling back to copy+delete"
        );
        copy_dir_recursive(staging, &target)
            .map_err(|e| WorkflowError::WriteFailed(format!("copy staging dir: {}", e)))?;
        std::fs::remove_dir_all(staging).map_err(|e| {
            WorkflowError::WriteFailed(format!("remove staging dir after copy: {}", e))
        })?;
    }

    log::debug!("[relocate_plan_dir] relocated {:?} → {:?}", staging, target);
    Ok(target)
}

/// Known artifact filenames to include in the context header.
const KNOWN_ARTIFACTS: &[&str] = &[
    "PRD.md",
    "TODO.md",
    "acceptance-tests.md",
    "progress.md",
    "evaluation-report.md",
    "demo-plan.md",
    "validate-tests-report.md",
    "validate-prod-ready-report.md",
    "analyze-clean-code-report.md",
];

/// Build the context header string listing absolute paths to existing `.md` artifacts
/// in `plan_dir`. Returns an empty string when `plan_dir` is `None` or no files exist.
pub fn build_context_header(plan_dir: Option<&Path>) -> String {
    let dir = match plan_dir {
        None => {
            log::debug!("[build_context_header] plan_dir is None — skipping header");
            return String::new();
        }
        Some(d) => d,
    };

    log::debug!("[build_context_header] scanning {:?} for artifacts", dir);

    let mut lines: Vec<String> = Vec::new();
    for artifact in KNOWN_ARTIFACTS {
        let path = dir.join(artifact);
        if path.exists() {
            let canonical = std::fs::canonicalize(&path).unwrap_or(path);
            log::debug!(
                "[build_context_header] found artifact: {}",
                canonical.display()
            );
            lines.push(format!("{}: {}", artifact, canonical.display()));
        }
    }

    if lines.is_empty() {
        log::debug!(
            "[build_context_header] no artifacts found in {:?} — empty header",
            dir
        );
        return String::new();
    }

    let mut header = String::from("**CRITICAL FOR CONTEXT AND SUMMARY**\n");
    for line in &lines {
        header.push_str(line);
        header.push('\n');
    }
    log::debug!(
        "[build_context_header] built header with {} artifact(s)",
        lines.len()
    );
    header
}

/// Prepend the context header to `prompt`. When the header is empty, returns `prompt` unchanged.
pub fn prepend_context_header(prompt: String, plan_dir: Option<&Path>) -> String {
    let header = build_context_header(plan_dir);
    if header.is_empty() {
        log::debug!("[prepend_context_header] no header — prompt unchanged");
        return prompt;
    }
    log::debug!("[prepend_context_header] prepending context header to prompt");
    format!(
        "<context-reminder>\n{}</context-reminder>\n\n{}",
        header, prompt
    )
}

#[cfg(test)]
mod relocation_tests {
    use super::*;
    use std::fs;

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tddy-wr-{}", label));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    // ── R4: valid suggestion moves directory ─────────────────────────────────

    #[test]
    fn test_relocate_valid_suggestion() {
        let root = temp_dir("relocate-valid");
        fs::create_dir_all(root.join(".git")).unwrap();

        let output_dir = root.join("output");
        fs::create_dir_all(&output_dir).unwrap();

        let dir_name = "2026-03-08-my-feature";
        let staging = output_dir.join(dir_name);
        fs::create_dir_all(&staging).unwrap();
        fs::write(staging.join("PRD.md"), "# PRD").unwrap();

        let result = relocate_plan_dir(&staging, "docs/plans/", dir_name, &output_dir)
            .expect("valid suggestion should succeed");

        let expected = root.join("docs/plans").join(dir_name);
        assert_eq!(
            result, expected,
            "final path should be at suggested location"
        );
        assert!(expected.exists(), "target directory should exist");
        assert!(
            expected.join("PRD.md").exists(),
            "PRD.md should be present at target"
        );

        let _ = fs::remove_dir_all(&root);
    }

    // ── R3: absolute-path suggestion falls back silently ──────────────────────

    #[test]
    fn test_relocate_invalid_absolute_path() {
        let root = temp_dir("relocate-absolute");
        let output_dir = root.join("output");
        fs::create_dir_all(&output_dir).unwrap();

        let dir_name = "2026-03-08-my-feature";
        let staging = output_dir.join(dir_name);
        fs::create_dir_all(&staging).unwrap();

        let result = relocate_plan_dir(&staging, "/tmp/evil", dir_name, &output_dir)
            .expect("absolute path should fall back, not error");

        assert_eq!(result, staging, "should fall back to staging path");

        let _ = fs::remove_dir_all(&root);
    }

    // ── R3: path traversal (dotdot) rejected ─────────────────────────────────

    #[test]
    fn test_relocate_dotdot_rejected() {
        let root = temp_dir("relocate-dotdot");
        let output_dir = root.join("output");
        fs::create_dir_all(&output_dir).unwrap();

        let dir_name = "2026-03-08-my-feature";
        let staging = output_dir.join(dir_name);
        fs::create_dir_all(&staging).unwrap();

        let result = relocate_plan_dir(&staging, "../../outside", dir_name, &output_dir)
            .expect("dotdot path should fall back, not error");

        assert_eq!(result, staging, "dotdot path should fall back to staging");

        let _ = fs::remove_dir_all(&root);
    }

    // ── R3: empty / whitespace suggestion falls back ──────────────────────────

    #[test]
    fn test_relocate_empty_suggestion() {
        let root = temp_dir("relocate-empty");
        let output_dir = root.join("output");
        fs::create_dir_all(&output_dir).unwrap();

        let dir_name = "2026-03-08-my-feature";
        let staging = output_dir.join(dir_name);
        fs::create_dir_all(&staging).unwrap();

        let result = relocate_plan_dir(&staging, "   ", dir_name, &output_dir)
            .expect("whitespace suggestion should fall back, not error");

        assert_eq!(
            result, staging,
            "whitespace-only suggestion should fall back"
        );

        let _ = fs::remove_dir_all(&root);
    }

    // ── R3: suggestion resolves to same path → no-op ──────────────────────────

    #[test]
    fn test_relocate_same_path_noop() {
        let root = temp_dir("relocate-same");
        // No .git here → find_git_root falls back to output_dir.parent() == root.
        // Suggestion "output/" → root / "output" / dir_name == staging → noop.
        let output_dir = root.join("output");
        fs::create_dir_all(&output_dir).unwrap();

        let dir_name = "2026-03-08-my-feature";
        let staging = output_dir.join(dir_name);
        fs::create_dir_all(&staging).unwrap();

        let result = relocate_plan_dir(&staging, "output/", dir_name, &output_dir)
            .expect("same-path suggestion should be a noop, not error");

        assert_eq!(result, staging, "same-path should return staging unchanged");
        assert!(
            staging.is_dir(),
            "staging directory should still exist as a real dir"
        );

        let _ = fs::remove_dir_all(&root);
    }

    // ── R2: find_git_root locates .git ancestor ───────────────────────────────

    #[test]
    fn test_find_git_root_finds_dot_git() {
        let root = temp_dir("git-root-find");
        fs::create_dir_all(root.join(".git")).unwrap();
        let nested = root.join("a/b/c");
        fs::create_dir_all(&nested).unwrap();

        let found = find_git_root(&nested);

        assert_eq!(found, root, "should return the ancestor that contains .git");

        let _ = fs::remove_dir_all(&root);
    }

    // ── R2: find_git_root falls back to parent when no .git found ─────────────

    #[test]
    fn test_find_git_root_fallback() {
        let root = temp_dir("git-root-fallback");
        // `root` has no .git; temp dirs on supported platforms are outside any
        // git repo, so walking up from `nested` will not find one.
        let nested = root.join("a");
        fs::create_dir_all(&nested).unwrap();

        let found = find_git_root(&nested);

        // Must not return `nested` itself — always walks at least one level up.
        assert_ne!(found, nested, "must not return the input directory itself");
        assert!(found.is_absolute(), "result must be an absolute path");
        assert!(found.is_dir(), "result must be an existing directory");

        let _ = fs::remove_dir_all(&root);
    }
}

#[cfg(test)]
mod context_header_tests {
    use super::*;
    use std::fs;

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tddy-ch-{}", label));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    // ── AC1: header lists existing .md files ─────────────────────────────────

    #[test]
    fn test_context_header_lists_existing_md_files() {
        let dir = temp_dir("lists-existing");
        fs::write(dir.join("PRD.md"), "# PRD").unwrap();
        fs::write(dir.join("TODO.md"), "- [ ] Task").unwrap();

        let header = build_context_header(Some(&dir));

        assert!(
            header.starts_with("**CRITICAL FOR CONTEXT AND SUMMARY**\n"),
            "header must start with the marker line, got: {:?}",
            &header[..header.len().min(200)]
        );
        assert!(header.contains("PRD.md:"), "header must list PRD.md");
        assert!(header.contains("TODO.md:"), "header must list TODO.md");

        let _ = fs::remove_dir_all(&dir);
    }

    // ── AC3: missing artifacts are silently omitted ───────────────────────────

    #[test]
    fn test_context_header_omits_missing_files() {
        let dir = temp_dir("omits-missing");
        fs::write(dir.join("PRD.md"), "# PRD").unwrap();
        // TODO.md and acceptance-tests.md are NOT created

        let header = build_context_header(Some(&dir));

        assert!(header.contains("PRD.md:"), "should list PRD.md");
        assert!(
            !header.contains("TODO.md:"),
            "must not list missing TODO.md"
        );
        assert!(
            !header.contains("acceptance-tests.md:"),
            "must not list missing acceptance-tests.md"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    // ── AC2: empty plan directory → no header ────────────────────────────────

    #[test]
    fn test_context_header_empty_for_no_md_files() {
        let dir = temp_dir("empty-dir");
        // No .md files

        let header = build_context_header(Some(&dir));

        assert!(
            header.is_empty(),
            "header must be empty when no md files exist, got: {:?}",
            header
        );

        let _ = fs::remove_dir_all(&dir);
    }

    // ── AC2: None plan_dir → no header ───────────────────────────────────────

    #[test]
    fn test_context_header_empty_for_none_plan_dir() {
        let header = build_context_header(None);

        assert!(
            header.is_empty(),
            "header must be empty when plan_dir is None"
        );
    }

    // ── AC4: paths are absolute ───────────────────────────────────────────────

    #[test]
    fn test_context_header_uses_absolute_paths() {
        let dir = temp_dir("abs-paths");
        fs::write(dir.join("PRD.md"), "# PRD").unwrap();

        let header = build_context_header(Some(&dir));

        let prd_line = header
            .lines()
            .find(|l| l.starts_with("PRD.md:"))
            .expect("header must contain a PRD.md line");
        let path_str = prd_line.trim_start_matches("PRD.md:").trim();

        assert!(
            std::path::Path::new(path_str).is_absolute(),
            "PRD.md path must be absolute, got: {}",
            path_str
        );
        assert!(
            std::path::Path::new(path_str).exists(),
            "listed path must exist on disk: {}",
            path_str
        );

        let _ = fs::remove_dir_all(&dir);
    }

    // ── AC6: original prompt appears after header ─────────────────────────────

    #[test]
    fn test_prepend_adds_header_before_prompt() {
        let dir = temp_dir("prepend-adds");
        fs::write(dir.join("PRD.md"), "# PRD").unwrap();

        let original = "Do the task.".to_string();
        let result = prepend_context_header(original.clone(), Some(&dir));

        assert!(
            result.starts_with("<context-reminder>"),
            "result must start with context-reminder tag"
        );
        assert!(
            result.contains("**CRITICAL FOR CONTEXT AND SUMMARY**"),
            "result must contain header marker inside context-reminder"
        );
        assert!(
            result.contains("</context-reminder>"),
            "result must contain closing context-reminder tag"
        );

        let close_tag = "</context-reminder>";
        let close_pos = result.find(close_tag).expect("must find closing tag");
        let after_tag = &result[close_pos + close_tag.len()..];
        let body = after_tag.trim_start_matches('\n');
        assert_eq!(
            body, original,
            "original prompt must appear verbatim after the context-reminder block"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    // ── context-reminder tags wrap the header block ────────────────────────────

    #[test]
    fn test_prepend_wraps_header_in_context_reminder_tags() {
        let dir = temp_dir("wrap-tags");
        fs::write(dir.join("PRD.md"), "# PRD").unwrap();

        let result = prepend_context_header("Task.".to_string(), Some(&dir));

        assert!(
            result.starts_with("<context-reminder>\n"),
            "header block must start with <context-reminder> and newline"
        );
        let inner_start = "<context-reminder>\n".len();
        let inner = &result[inner_start..];
        assert!(
            inner.starts_with("**CRITICAL FOR CONTEXT AND SUMMARY**"),
            "first line inside tags must be the marker"
        );
        assert!(
            result.contains("\n</context-reminder>\n"),
            "closing tag must be followed by newline before prompt body"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    // ── AC7: no-op when header is empty ──────────────────────────────────────

    #[test]
    fn test_prepend_returns_original_when_no_header() {
        let dir = temp_dir("prepend-noop");
        // No .md files → build_context_header returns ""

        let original = "Do the task.".to_string();
        let result = prepend_context_header(original.clone(), Some(&dir));

        assert_eq!(
            result, original,
            "prompt must be unchanged when no header is needed"
        );

        let _ = fs::remove_dir_all(&dir);
    }
}
