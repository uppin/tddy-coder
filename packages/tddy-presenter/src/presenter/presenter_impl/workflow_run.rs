//! The running workflow: starting, restarting and respawning it, the operator input that drives
//! it, and what happens when it completes.

use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

use super::{format_session_id_for_log, Presenter, QUEUED_INSTRUCTION_PREFIX};
use crate::presenter::activity_prompt_log;
use crate::presenter::presenter_events::PresenterEvent;
use crate::presenter::state::{ActivityKind, AppMode};
use crate::presenter::workflow_runner;
use crate::presenter::WorkflowCompletePayload;
use crate::toolcall::ToolCallRequest;
use crate::SharedBackend;

impl Presenter {
    /// Start the workflow with the given backend.
    #[allow(clippy::too_many_arguments)]
    pub fn start_workflow(
        &mut self,
        backend: SharedBackend,
        output_dir: PathBuf,
        session_dir: Option<PathBuf>,
        initial_prompt: Option<String>,
        conversation_output_path: Option<PathBuf>,
        debug_output_path: Option<PathBuf>,
        debug: bool,
        session_id: Option<String>,
        socket_path: Option<PathBuf>,
        tool_call_rx: Option<mpsc::Receiver<ToolCallRequest>>,
    ) {
        self.workflow.backend = Some(backend.clone());
        self.workflow.output_dir = Some(output_dir.clone());
        self.state.skills_project_root = Some(output_dir.clone());
        self.workflow.session_dir = session_dir.clone();
        self.workflow.conversation_output = conversation_output_path.clone();
        self.workflow.debug_output = debug_output_path.clone();
        self.workflow.debug = debug;
        self.views.tool_call_rx = tool_call_rx;
        self.workflow.socket_path = socket_path.clone();
        self.state.workflow_session_id = session_id.clone();
        log::debug!(
            "start_workflow: initial workflow_session_id={}",
            self.state
                .workflow_session_id
                .as_deref()
                .map(format_session_id_for_log)
                .unwrap_or_else(|| "None".to_string())
        );
        self.spawn_workflow(
            backend,
            output_dir,
            session_dir,
            initial_prompt,
            conversation_output_path,
            debug_output_path,
            debug,
            session_id,
            socket_path,
            self.workflow.worktree_dir.clone(),
        );
    }

    /// Starts another workflow run with `prompt`. Reuses [`WorkflowRun::session_dir`] when set so
    /// web/daemon/CLI-bound session folders keep receiving runs; only passes `None` when the first
    /// run also had no session dir (fresh allocation under `TDDY_SESSIONS_DIR`).
    fn restart_workflow(&mut self, prompt: String) {
        if let (Some(backend), Some(output_dir)) = (
            self.workflow.backend.clone(),
            self.workflow.output_dir.clone(),
        ) {
            if let Some(h) = self.workflow.handle.take() {
                // Drop the answer sender so a workflow blocked on `answer_rx.recv()` (initial
                // feature line or clarification) unblocks and exits; otherwise `join()` deadlocks
                // (e.g. `/start-*` from FeatureInput before any text was sent to the channel).
                self.workflow.answer_tx = None;
                let _ = h.join();
            }
            self.workflow.result = None;
            self.state.mode = AppMode::Running;
            self.broadcast_mode_changed();
            self.spawn_workflow(
                backend,
                output_dir,
                self.workflow.session_dir.clone(),
                Some(prompt),
                self.workflow.conversation_output.clone(),
                self.workflow.debug_output.clone(),
                self.workflow.debug,
                None,
                self.workflow.socket_path.clone(),
                self.workflow.worktree_dir.clone(),
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn spawn_workflow(
        &mut self,
        backend: SharedBackend,
        output_dir: PathBuf,
        session_dir: Option<PathBuf>,
        initial_prompt: Option<String>,
        conversation_output_path: Option<PathBuf>,
        debug_output_path: Option<PathBuf>,
        debug: bool,
        session_id: Option<String>,
        socket_path: Option<PathBuf>,
        worktree_dir: Option<PathBuf>,
    ) {
        let (event_tx, event_rx) = mpsc::channel();
        let (answer_tx, answer_rx) = mpsc::channel();

        let model_for_workflow = self.state.model.clone();
        let recipe = self.backend.recipe.clone();
        let tddy_data_dir = self.tddy_data_dir.clone();
        let handle = thread::spawn(move || {
            workflow_runner::run_workflow(
                recipe,
                backend,
                event_tx,
                answer_rx,
                output_dir,
                session_dir,
                session_id,
                Some(model_for_workflow),
                initial_prompt,
                conversation_output_path,
                debug_output_path,
                debug,
                socket_path,
                worktree_dir,
                tddy_data_dir,
            );
        });

        self.workflow.event_rx = Some(event_rx);
        self.workflow.answer_tx = Some(answer_tx);
        self.workflow.handle = Some(handle);
    }

    /// Prefer session/plan dir for `changeset.yaml`; fall back to [`WorkflowRun::output_dir`].
    fn changeset_read_dir(&self) -> Option<&PathBuf> {
        self.workflow
            .session_dir
            .as_ref()
            .or(self.workflow.output_dir.as_ref())
    }

    /// After a successful `/start-*` structured run, switch active recipe back to free prompting.
    fn finish_start_slash_structured_run_if_needed(&mut self) {
        if !self.backend.start_slash_structured_run_active {
            return;
        }
        self.backend.start_slash_structured_run_active = false;
        let Some(ref resolve) = self.backend.recipe_resolver else {
            log::debug!("finish_start_slash_structured_run: no recipe_resolver");
            return;
        };
        let fp_name = crate::feature_start_slash::DEFAULT_UNSPECIFIED_WORKFLOW_RECIPE_CLI_NAME;
        match resolve(fp_name) {
            Ok(r) => {
                self.backend.recipe = r;
                if let Some(dir) = self.changeset_read_dir().cloned() {
                    let mut cs = crate::changeset::read_changeset(&dir).unwrap_or_default();
                    cs.recipe = Some(fp_name.to_string());
                    if let Err(e) = crate::changeset::write_changeset(&dir, &cs) {
                        log::warn!("finish_start_slash_structured_run: write_changeset: {}", e);
                    }
                }
                log::info!(
                    "finish_start_slash_structured_run: active recipe restored to {:?}",
                    fp_name
                );
            }
            Err(e) => log::warn!(
                "finish_start_slash_structured_run: resolve free-prompting: {}",
                e
            ),
        }
    }

    /// Handle `/start-<recipe>` from feature input: switch recipe, persist, restart workflow with remainder.
    /// Returns `true` if the line was fully handled as a start-slash command (including parse errors).
    /// Returns `false` when a resolver is required but missing so the caller treats the line as a normal feature submit.
    fn try_handle_start_slash_line(&mut self, full_line: &str) -> bool {
        if !matches!(self.state.mode, AppMode::FeatureInput) {
            return false;
        }
        let Some(parsed) = crate::feature_start_slash::parse_feature_start_slash_line(full_line)
        else {
            return false;
        };
        match parsed {
            Err(msg) => {
                self.log_activity(format!("/start-: {msg}"), ActivityKind::Info);
                true
            }
            Ok(cli_name) => {
                let Some(ref resolve) = self.backend.recipe_resolver else {
                    log::debug!(
                        "try_handle_start_slash_line: no recipe_resolver; pass through as normal submit"
                    );
                    return false;
                };
                match resolve(&cli_name) {
                    Err(e) => {
                        self.log_activity(
                            format!("Unknown or unsupported recipe `{cli_name}`: {e}"),
                            ActivityKind::Info,
                        );
                        true
                    }
                    Ok(new_recipe) => {
                        let structured = new_recipe.name() != "free-prompting";
                        self.backend.start_slash_structured_run_active = structured;
                        self.backend.recipe = new_recipe;
                        if let Some(dir) = self.changeset_read_dir().cloned() {
                            let mut cs = crate::changeset::read_changeset(&dir).unwrap_or_default();
                            cs.recipe = Some(cli_name.clone());
                            if let Err(e) = crate::changeset::write_changeset(&dir, &cs) {
                                log::warn!("try_handle_start_slash_line: write_changeset: {}", e);
                            }
                        }
                        let rest = crate::feature_start_slash::remainder_after_start_slash_line(
                            full_line, &cli_name,
                        );
                        self.restart_workflow(rest);
                        true
                    }
                }
            }
        }
    }

    /// Start, continue or restart the workflow with the operator's feature input.
    pub(super) fn submit_feature_input(&mut self, text: String) {
        if text.is_empty() {
            return;
        }
        if self.try_handle_start_slash_line(&text) {
            return;
        }
        if let Some(ref dir) = self.workflow.session_dir {
            let mut cs = crate::changeset::read_changeset(dir)
                .unwrap_or_else(|_| crate::changeset::Changeset::default());
            cs.initial_prompt = Some(text.clone());
            if let Err(e) = crate::changeset::write_changeset(dir, &cs) {
                log::warn!("SubmitFeatureInput: persist changeset: {}", e);
            }
        }
        let user_line = activity_prompt_log::format_user_prompt_line(&text);
        if !user_line.is_empty() {
            self.log_activity(user_line, ActivityKind::UserPrompt);
        }
        // Previous run finished (`workflow_result` set): start a new workflow. Do not send on
        // `answer_tx` — it may still be `Some` until the worker thread exits, and a buffered
        // send would skip `restart_workflow` and drop the second run.
        if self.is_done() {
            self.restart_workflow(text);
            return;
        }
        let text_for_restart = if let Some(ref tx) = self.workflow.answer_tx {
            match tx.send(text) {
                Ok(()) => None,
                Err(std::sync::mpsc::SendError(t)) => Some(t),
            }
        } else {
            Some(text)
        };
        if let Some(prompt) = text_for_restart {
            self.restart_workflow(prompt);
        }
    }

    /// Hand the session to the agent: resolve its session id, then quit with that exit action.
    pub(super) fn continue_with_agent(&mut self) {
        let session_id = if let Some(cs_dir) = self.changeset_read_dir() {
            log::info!(
                "ContinueWithAgent: read_changeset from {} (workflow_session_dir={:?}, workflow_output_dir={:?})",
                cs_dir.display(),
                self.workflow.session_dir.as_ref().map(|p| p.display().to_string()),
                self.workflow.output_dir.as_ref().map(|p| p.display().to_string()),
            );
            match crate::changeset::read_changeset(cs_dir) {
                Ok(cs) => {
                    // Prefer persisted active session; else same lookup as ResumeFromError
                    // (Cursor often stores thread id on tagged sessions only).
                    cs.state
                        .session_id
                        .clone()
                        .or_else(|| {
                            self.state.current_goal.as_deref().and_then(|goal| {
                                let sid = crate::changeset::get_session_for_tag(&cs, goal);
                                log::info!(
                                    "ContinueWithAgent: session from tag {:?} → {:?}",
                                    goal,
                                    sid
                                );
                                sid
                            })
                        })
                        .or_else(|| {
                            cs.sessions.last().map(|s| {
                                log::info!(
                                    "ContinueWithAgent: fallback to last session entry id={}",
                                    s.id
                                );
                                s.id.clone()
                            })
                        })
                }
                Err(e) => {
                    log::warn!(
                        "ContinueWithAgent: could not read changeset at {}: {}",
                        cs_dir.display(),
                        e
                    );
                    None
                }
            }
        } else {
            log::warn!("ContinueWithAgent: no workflow_session_dir or workflow_output_dir set");
            None
        };
        if let Some(sid) = session_id {
            self.state.exit_action =
                Some(crate::presenter::state::ExitAction::ContinueWithAgent { session_id: sid });
            self.state.should_quit = true;
            self.broadcast(PresenterEvent::ShouldQuit);
        } else {
            log::warn!(
                "ContinueWithAgent: no session id resolved; not setting exit_action or ShouldQuit"
            );
        }
    }

    /// Respawn the workflow after an error, resuming the goal's last session if there is one.
    pub(super) fn resume_from_error(&mut self) {
        log::info!(
            "ResumeFromError: looking up last session for goal {:?}",
            self.state.current_goal
        );
        let session_id = if let (Some(cs_dir), Some(goal)) = (
            self.changeset_read_dir(),
            self.state.current_goal.as_deref(),
        ) {
            log::info!(
                "ResumeFromError: read_changeset from {} for tag {}",
                cs_dir.display(),
                goal
            );
            match crate::changeset::read_changeset(cs_dir) {
                Ok(cs) => {
                    let sid = crate::changeset::get_session_for_tag(&cs, goal);
                    log::info!("ResumeFromError: session_id={:?} for tag={}", sid, goal);
                    sid
                }
                Err(e) => {
                    log::warn!(
                        "ResumeFromError: could not read changeset at {}: {}",
                        cs_dir.display(),
                        e
                    );
                    None
                }
            }
        } else {
            log::warn!(
                "ResumeFromError: no changeset dir or goal (session_dir={:?}, output_dir={:?}, goal={:?}), spawning fresh",
                self.workflow.session_dir,
                self.workflow.output_dir,
                self.state.current_goal
            );
            None
        };
        self.state.mode = AppMode::Running;
        self.broadcast_mode_changed();
        if let (Some(backend), Some(output_dir)) = (
            self.workflow.backend.clone(),
            self.workflow.output_dir.clone(),
        ) {
            if let Some(h) = self.workflow.handle.take() {
                let _ = h.join();
            }
            self.spawn_workflow(
                backend,
                output_dir,
                self.workflow.session_dir.clone(),
                None,
                self.workflow.conversation_output.clone(),
                self.workflow.debug_output.clone(),
                self.workflow.debug,
                session_id,
                self.workflow.socket_path.clone(),
                self.workflow.worktree_dir.clone(),
            );
        }
    }

    /// Record the workflow's result, then restart it with a queued prompt or settle the mode.
    pub(super) fn on_workflow_complete(&mut self, result: Result<WorkflowCompletePayload, String>) {
        self.questions.awaiting_open_answer = false;
        self.flush_agent_output_buffer();
        self.workflow.result = Some(result.clone());
        self.broadcast(PresenterEvent::WorkflowComplete(result.clone()));
        if result.is_ok() && !self.state.inbox.is_empty() {
            let item = self.state.inbox.remove(0);
            let prefixed = format!("{}{}", QUEUED_INSTRUCTION_PREFIX, item);
            self.broadcast(PresenterEvent::InboxChanged(self.state.inbox.clone()));
            self.state.mode = AppMode::Running;
            self.broadcast_mode_changed();
            // Workflow thread has exited; restart with dequeued prompt.
            // Pass session_dir so we resume in the same session (avoids re-creating worktree).
            let session_dir = result.as_ref().ok().and_then(|p| p.session_dir.clone());
            if let (Some(backend), Some(output_dir)) = (
                self.workflow.backend.clone(),
                self.workflow.output_dir.clone(),
            ) {
                if let Some(h) = self.workflow.handle.take() {
                    let _ = h.join();
                }
                self.workflow.result = None;
                self.state.workflow_session_id = None;
                log::debug!(
                    "WorkflowComplete: inbox restart — cleared workflow_session_id until SessionStarted"
                );
                self.spawn_workflow(
                    backend,
                    output_dir,
                    session_dir,
                    Some(prefixed),
                    self.workflow.conversation_output.clone(),
                    self.workflow.debug_output.clone(),
                    self.workflow.debug,
                    None,
                    self.workflow.socket_path.clone(),
                    self.workflow.worktree_dir.clone(),
                );
            }
        } else {
            match &result {
                Ok(_) => {
                    self.finish_start_slash_structured_run_if_needed();
                }
                Err(_) => {
                    self.backend.start_slash_structured_run_active = false;
                }
            }
            match result {
                Err(ref msg) => {
                    log::error!("Workflow failed: {}", msg);
                    self.state.workflow_session_id = None;
                    self.log_activity(format!("Workflow failed: {}", msg), ActivityKind::Info);
                    self.state.mode = AppMode::ErrorRecovery {
                        error_message: msg.clone(),
                    };
                }
                Ok(_) => {
                    log::info!("WorkflowComplete Ok → FeatureInput (ready for new workflow)");
                    self.state.workflow_session_id = None;
                    self.state.mode = AppMode::FeatureInput;
                }
            }
            self.broadcast_mode_changed();
        }
    }
}
