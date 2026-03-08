//! Workflow state machine for tddy-coder.

mod acceptance_tests;
mod evaluate;
mod green;
mod planning;
mod red;
mod validate;
mod validate_refactor;

use crate::backend::{CodingBackend, InvokeResponse};
use crate::changeset::{
    append_session_and_update_state, clarification_qa_from_backend, get_session_for_tag,
    read_changeset, resolve_model, update_state, write_changeset, Changeset,
};
use crate::error::WorkflowError;
use crate::output::{
    parse_acceptance_tests_response, parse_evaluate_response, parse_green_response,
    parse_planning_response, parse_red_response, parse_validate_refactor_response,
    parse_validate_response, slugify_directory_name, update_acceptance_tests_file,
    update_progress_file, write_acceptance_tests_file, write_artifacts, write_demo_results_file,
    write_evaluation_report, write_progress_file, write_red_output_file, write_validation_report,
};
use std::path::{Path, PathBuf};

/// Emit debug output when parsing agent response fails. Shows output, raw stream, exit code.
fn emit_parse_failure_debug(response: &InvokeResponse, goal: &str, empty_hint: Option<&str>) {
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
    pub conversation_output_path: Option<PathBuf>,
    pub inherit_stdin: bool,
    pub allowed_tools_extras: Option<Vec<String>>,
    pub debug: bool,
}

/// Options for the green step.
#[derive(Debug, Default)]
pub struct GreenOptions {
    pub model: Option<String>,
    pub agent_output: bool,
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

        let model = options.model.clone();

        let request = crate::backend::InvokeRequest {
            prompt,
            system_prompt: None,
            system_prompt_path: Some(system_prompt_path),
            goal: crate::backend::Goal::Plan,
            model,
            session_id,
            is_resume,
            working_dir: Some(output_dir.to_path_buf()),
            debug: options.debug,
            agent_output: options.agent_output,
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

        let planning = match parse_planning_response(&response.output) {
            Ok(out) => out,
            Err(e) => {
                eprintln!(
                    "--- Failed parse input (length {} bytes) ---",
                    response.output.len()
                );
                eprintln!(
                    "Hint: The agent must output a <structured-response> block with the actual PRD and TODO content. \
                     Meta-commentary (e.g. 'I've created the PRD...') without the block causes this error. \
                     See the system prompt for the required format."
                );
                eprintln!("{}", response.output);
                eprintln!("--- End failed parse input ---");
                self.set_state(WorkflowState::Failed {
                    error: e.to_string(),
                });
                return Err(WorkflowError::ParseError(e));
            }
        };

        write_artifacts(&output_path, &planning)?;

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
            working_dir: plan_dir.parent().map(std::path::Path::to_path_buf),
            debug: options.debug,
            agent_output: options.agent_output,
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

        let output = match parse_acceptance_tests_response(&response.output) {
            Ok(out) => {
                write_acceptance_tests_file(plan_dir, &out)?;
                let mut cs = read_changeset(plan_dir)?;
                let at_session_id = response
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
                    &response,
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

        let session_id = uuid::Uuid::new_v4().to_string();

        let request = crate::backend::InvokeRequest {
            prompt,
            system_prompt: Some(system_prompt),
            system_prompt_path: None,
            goal: crate::backend::Goal::Red,
            model,
            session_id: Some(session_id.clone()),
            is_resume: false,
            working_dir: plan_dir.parent().map(std::path::Path::to_path_buf),
            debug: options.debug,
            agent_output: options.agent_output,
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

        let output = match parse_red_response(&response.output) {
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
                    &response,
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

        let system_prompt = green::system_prompt();
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
            model,
            session_id: Some(session_id.clone()),
            is_resume: true,
            working_dir: plan_dir.parent().map(std::path::Path::to_path_buf),
            debug: options.debug,
            agent_output: options.agent_output,
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

        match parse_green_response(&response.output) {
            Ok(out) => {
                let _ = update_progress_file(plan_dir, &out);
                let _ = update_acceptance_tests_file(plan_dir, &out);
                if out.all_tests_passing() {
                    if let Some(mut cs) = changeset {
                        update_state(&mut cs, "GreenComplete");
                        let _ = write_changeset(plan_dir, &cs);
                    }
                    if plan_dir.join("demo-plan.md").exists() {
                        let (summary, steps) = out
                            .demo_results
                            .as_ref()
                            .map(|dr| (dr.summary.as_str(), dr.steps_completed))
                            .unwrap_or(("Demo verified with implementation.", 1));
                        let _ = write_demo_results_file(plan_dir, summary, steps);
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
                    &response,
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

    /// Run the validate-changes step: analyze current git changes for risks.
    /// Callable from Init state without a prior plan/red/green run (standalone).
    /// When `plan_dir` is provided, reads changeset/PRD context and includes it in the prompt.
    /// Always starts a fresh session (is_resume: false).
    pub fn validate(
        &mut self,
        working_dir: &std::path::Path,
        plan_dir: Option<&std::path::Path>,
        _answers: Option<&str>,
        options: &ValidateOptions,
    ) -> Result<crate::output::ValidateOutput, WorkflowError> {
        let can_start = matches!(self.state, WorkflowState::Init);
        if !can_start {
            return Err(WorkflowError::InvalidTransition(format!(
                "cannot validate from {:?}",
                self.state
            )));
        }

        let prd_owned = plan_dir.and_then(|d| std::fs::read_to_string(d.join("PRD.md")).ok());
        let changeset_owned =
            plan_dir.and_then(|d| std::fs::read_to_string(d.join("changeset.yaml")).ok());
        let prd_content = prd_owned.as_deref();
        let changeset_content = changeset_owned.as_deref();

        let system_prompt = validate::system_prompt();
        let prompt = validate::build_prompt(prd_content, changeset_content);

        let session_id = uuid::Uuid::new_v4().to_string();
        let model = options.model.clone();

        let request = crate::backend::InvokeRequest {
            prompt,
            system_prompt: Some(system_prompt),
            system_prompt_path: None,
            goal: crate::backend::Goal::Validate,
            model,
            session_id: Some(session_id.clone()),
            is_resume: false,
            working_dir: Some(working_dir.to_path_buf()),
            debug: options.debug,
            agent_output: options.agent_output,
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

        let output = match parse_validate_response(&response.output) {
            Ok(out) => {
                let _ = write_validation_report(working_dir, &out);
                out
            }
            Err(e) => {
                emit_parse_failure_debug(
                    &response,
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
        working_dir: &std::path::Path,
        plan_dir: Option<&std::path::Path>,
        _answers: Option<&str>,
        options: &EvaluateOptions,
    ) -> Result<crate::output::EvaluateOutput, WorkflowError> {
        eprintln!(
            r#"{{"tddy":{{"marker_id":"M009","scope":"workflow::mod::evaluate","data":{{}}}}}}"#
        );

        // plan_dir is required: evaluation-report.md is written there
        let plan_dir = plan_dir.ok_or_else(|| {
            WorkflowError::PlanDirInvalid(
                "evaluate-changes requires a plan_dir to write evaluation-report.md".into(),
            )
        })?;

        let can_start = matches!(self.state, WorkflowState::Init);
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

        let system_prompt = evaluate::system_prompt();
        let prompt = evaluate::build_prompt(prd_content, changeset_content);

        let session_id = uuid::Uuid::new_v4().to_string();
        let model = options.model.clone();

        let request = crate::backend::InvokeRequest {
            prompt,
            system_prompt: Some(system_prompt),
            system_prompt_path: None,
            goal: crate::backend::Goal::Evaluate,
            model,
            session_id: Some(session_id),
            is_resume: false,
            working_dir: Some(working_dir.to_path_buf()),
            debug: options.debug,
            agent_output: options.agent_output,
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

        let output = match parse_evaluate_response(&response.output) {
            Ok(out) => {
                let _ = write_evaluation_report(plan_dir, &out);
                out
            }
            Err(e) => {
                emit_parse_failure_debug(
                    &response,
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
        eprintln!(
            r#"{{"tddy":{{"marker_id":"M010","scope":"workflow::mod::validate_refactor","data":{{}}}}}}"#
        );

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
            model,
            session_id: Some(session_id),
            is_resume: false,
            working_dir: Some(plan_dir.to_path_buf()),
            debug: options.debug,
            agent_output: options.agent_output,
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

        let output = match parse_validate_refactor_response(&response.output) {
            Ok(out) => out,
            Err(e) => {
                emit_parse_failure_debug(
                    &response,
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
