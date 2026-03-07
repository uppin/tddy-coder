//! Workflow state machine for tddy-coder.

mod acceptance_tests;
mod green;
mod planning;
mod red;

use crate::backend::CodingBackend;
use crate::changeset::{
    append_session_and_update_state, clarification_qa_from_backend, get_session_for_tag,
    read_changeset, resolve_model, update_state, write_changeset, Changeset,
};
use crate::error::WorkflowError;
use crate::output::{
    parse_acceptance_tests_response, parse_green_response, parse_planning_response,
    parse_red_response, slugify_directory_name, update_acceptance_tests_file, update_progress_file,
    write_acceptance_tests_file, write_artifacts, write_demo_results_file, write_progress_file,
    write_red_output_file,
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

/// Options for the green step.
#[derive(Debug, Default)]
pub struct GreenOptions {
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
    GreenImplementing,
    GreenComplete {
        output: crate::output::GreenOutput,
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

        let mut allowed_tools = crate::permission::plan_allowlist();
        if let Some(extras) = &options.allowed_tools_extras {
            allowed_tools.extend(extras.iter().cloned());
        }

        let model = options.model.clone();

        let request = crate::backend::InvokeRequest {
            prompt,
            system_prompt: None,
            system_prompt_path: Some(system_prompt_path),
            permission_mode: crate::backend::PermissionMode::Plan,
            model,
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
                self.set_state(WorkflowState::Failed {
                    error: e.to_string(),
                });
                return Err(WorkflowError::Backend(e));
            }
        };

        if !response.questions.is_empty() {
            if !response.session_id.is_empty() {
                self.session_id = Some(response.session_id.clone());
            }
            self.pending_clarification_questions = Some(response.questions.clone());
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

        let mut allowed_tools = crate::permission::acceptance_tests_allowlist();
        if let Some(extras) = &options.allowed_tools_extras {
            allowed_tools.extend(extras.iter().cloned());
        }

        let model = resolve_model(
            Some(&changeset),
            "acceptance-tests",
            options.model.as_deref(),
        );

        let request = crate::backend::InvokeRequest {
            prompt,
            system_prompt: Some(system_prompt),
            system_prompt_path: None,
            permission_mode: crate::backend::PermissionMode::AcceptEdits,
            model,
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
                self.set_state(WorkflowState::Failed {
                    error: e.to_string(),
                });
                return Err(WorkflowError::Backend(e));
            }
        };

        if !response.questions.is_empty() {
            if !response.session_id.is_empty() {
                let mut cs = read_changeset(plan_dir)?;
                if let Some(last) = cs.sessions.last_mut() {
                    last.id = response.session_id.clone();
                }
                let _ = write_changeset(plan_dir, &cs);
            }
            return Err(WorkflowError::ClarificationNeeded {
                questions: response.questions,
                session_id: response.session_id,
            });
        }

        let output = match parse_acceptance_tests_response(&response.output) {
            Ok(out) => {
                write_acceptance_tests_file(plan_dir, &out)?;
                let mut cs = read_changeset(plan_dir)?;
                let at_session_id = if response.session_id.is_empty() {
                    uuid::Uuid::new_v4().to_string()
                } else {
                    response.session_id.clone()
                };
                append_session_and_update_state(
                    &mut cs,
                    at_session_id,
                    "acceptance-tests",
                    "AcceptanceTestsReady",
                    None,
                );
                let _ = write_changeset(plan_dir, &cs);
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
                         has a valid changeset.yaml from a prior plan run."
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

        let mut allowed_tools = crate::permission::red_allowlist();
        if let Some(extras) = &options.allowed_tools_extras {
            allowed_tools.extend(extras.iter().cloned());
        }

        let request = crate::backend::InvokeRequest {
            prompt,
            system_prompt: Some(system_prompt),
            system_prompt_path: None,
            permission_mode: crate::backend::PermissionMode::AcceptEdits,
            model,
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
                self.set_state(WorkflowState::Failed {
                    error: e.to_string(),
                });
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
                if let Some(mut cs) = changeset {
                    append_session_and_update_state(
                        &mut cs,
                        session_id.clone(),
                        "impl",
                        "RedTestsReady",
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
                        None,
                    );
                    let _ = write_changeset(plan_dir, &cs);
                }
                out
            }
            Err(e) => {
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

        let mut allowed_tools = crate::permission::green_allowlist();
        if let Some(extras) = &options.allowed_tools_extras {
            allowed_tools.extend(extras.iter().cloned());
        }

        let request = crate::backend::InvokeRequest {
            prompt,
            system_prompt: Some(system_prompt),
            system_prompt_path: None,
            permission_mode: crate::backend::PermissionMode::AcceptEdits,
            model,
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
                self.set_state(WorkflowState::Failed {
                    error: e.to_string(),
                });
                return Err(WorkflowError::Backend(e));
            }
        };

        if !response.questions.is_empty() {
            return Err(WorkflowError::ClarificationNeeded {
                questions: response.questions,
                session_id: response.session_id,
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
                self.set_state(WorkflowState::Failed {
                    error: e.to_string(),
                });
                Err(WorkflowError::ParseError(e))
            }
        }
    }
}
