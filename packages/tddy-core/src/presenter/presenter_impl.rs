//! Presenter — orchestrates workflow and owns application state.

use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

use crate::backend::QuestionOption;
use crate::toolcall::{ToolCallRequest, ToolCallResponse};
use crate::{ClarificationQuestion, SharedBackend, WorkflowRecipe};

use crate::presenter::activity_prompt_log;
use crate::presenter::agent_activity;
use crate::presenter::intent::UserIntent;
use crate::presenter::presenter_events::{ModeChangedDetails, PresenterEvent, ViewConnection};
use crate::presenter::state::{
    ActivityEntry, ActivityKind, AppMode, CriticalPresenterState, PresenterState,
};
use crate::presenter::workflow_runner;
use crate::presenter::worktree_display::format_worktree_for_status_bar;
use crate::presenter::{WorkflowCompletePayload, WorkflowEvent};

/// Pending tool call response: Ask sends answers string, Approve sends allow/deny.
enum PendingToolCallResponse {
    Ask(tokio::sync::oneshot::Sender<ToolCallResponse>),
    Approve(tokio::sync::oneshot::Sender<ToolCallResponse>),
}

/// Instruction prefix for dequeued inbox prompts.
const QUEUED_INSTRUCTION_PREFIX: &str =
    "[QUEUED] The following prompt was queued while you were busy. Please address it:\n\n";

/// Resolves CLI workflow recipe name (`tdd`, `bugfix`) after `/recipe` slash selection.
type RecipeResolverFn = dyn Fn(&str) -> Result<Arc<dyn WorkflowRecipe>, String> + Send + Sync;

/// Creates the coding backend after the user picks an agent (tddy-coder); returns `Err` for e.g. missing tddy-tools.
pub type DeferredBackendFactory = Box<dyn FnOnce(&str) -> Result<SharedBackend, String> + Send>;

/// Parameters for the first [`Presenter::start_workflow`] after interactive backend selection (CLI).
#[derive(Debug)]
pub struct PendingWorkflowStart {
    pub output_dir: PathBuf,
    pub session_dir: Option<PathBuf>,
    pub initial_prompt: Option<String>,
    pub conversation_output_path: Option<PathBuf>,
    pub debug_output_path: Option<PathBuf>,
    pub debug: bool,
    pub session_id: Option<String>,
    pub socket_path: Option<PathBuf>,
    pub tool_call_rx: Option<mpsc::Receiver<ToolCallRequest>>,
}

/// Presenter: owns state, receives UserIntents, orchestrates workflow thread.
/// Views observe state via connect_view() → ViewConnection (broadcast events).
pub struct Presenter {
    state: PresenterState,
    workflow_event_rx: Option<mpsc::Receiver<WorkflowEvent>>,
    answer_tx: Option<mpsc::Sender<String>>,
    workflow_backend: Option<SharedBackend>,
    workflow_output_dir: Option<PathBuf>,
    /// Directory containing `changeset.yaml` for the active workflow (session / plan dir).
    /// When set, used for `read_changeset` in Continue with agent / resume; `workflow_output_dir`
    /// alone may be `.` while the changeset lives under this path.
    workflow_session_dir: Option<PathBuf>,
    workflow_conversation_output: Option<PathBuf>,
    workflow_debug_output: Option<PathBuf>,
    workflow_debug: bool,
    /// Stored when WorkflowComplete is received; used to print result on TUI exit.
    workflow_result: Option<Result<WorkflowCompletePayload, String>>,
    pending_questions: Vec<ClarificationQuestion>,
    current_question_index: usize,
    collected_answers: Vec<String>,
    agent_output_buffer: String,
    /// When true, the last `activity_log` row is the in-progress agent line (updated incrementally until `\n`).
    agent_output_partial_row_active: bool,
    /// Set when ClarificationNeeded is received with no questions; the workflow thread
    /// is blocked on `answer_rx` waiting for the next prompt (e.g. free-prompting multi-turn).
    awaiting_open_answer: bool,
    workflow_handle: Option<thread::JoinHandle<()>>,
    /// When set, events are broadcast for gRPC subscribers.
    broadcast_tx: Option<tokio::sync::broadcast::Sender<PresenterEvent>>,
    /// When set, connect_view() returns this for external views to send intents.
    intent_tx: Option<mpsc::Sender<UserIntent>>,
    /// Receiver for tddy-tools relay requests (Submit, Ask, Approve).
    tool_call_rx: Option<mpsc::Receiver<ToolCallRequest>>,
    /// When set, answers go to tool call response (Ask/Approve from tddy-tools) instead of answer_tx.
    pending_tool_call_response: Option<PendingToolCallResponse>,
    /// Stored socket path for workflow restart (dequeued prompts).
    workflow_socket_path: Option<PathBuf>,
    /// Pre-set worktree dir to skip git fetch/worktree creation in hooks.
    workflow_worktree_dir: Option<PathBuf>,
    /// When true, the next `AnswerSelect` resolves interactive backend choice (session start).
    backend_selection_pending: bool,
    /// When set with [`Self::configure_deferred_workflow_start`], backend selection creates the backend and starts the workflow.
    deferred_backend_factory: Option<DeferredBackendFactory>,
    pending_workflow_start: Option<PendingWorkflowStart>,
    /// When set, overrides per-backend default model after selection (CLI `--model`).
    deferred_cli_model: Option<String>,
    /// Active workflow definition (TDD, bug-fix, …).
    workflow_recipe: Arc<dyn WorkflowRecipe>,
    /// After `/recipe` from the feature slash menu: user is picking TDD vs bugfix.
    recipe_slash_selection_pending: bool,
    /// Resolves CLI recipe name to a new [`WorkflowRecipe`] (wired from `tddy-coder`).
    recipe_resolver: Option<Arc<RecipeResolverFn>>,
    /// Set when the user started a non-`free-prompting` workflow via `/start-*`; cleared after
    /// `WorkflowComplete` restores the session to free prompting (or on workflow error).
    start_slash_structured_run_active: bool,
    /// Shared critical state for broadcast lag recovery.
    /// Updated on every GoalStarted/StateChanged; views read after Lagged.
    critical_state: Arc<std::sync::Mutex<CriticalPresenterState>>,
}

fn format_session_id_for_log(id: &str) -> String {
    const MAX: usize = 12;
    if id.len() <= MAX {
        id.to_string()
    } else {
        format!("{}…", &id[..MAX])
    }
}

impl Presenter {
    /// Create a new Presenter in FeatureInput mode.
    pub fn new(
        agent: impl Into<String>,
        model: impl Into<String>,
        workflow_recipe: Arc<dyn WorkflowRecipe>,
    ) -> Self {
        let state = PresenterState {
            agent: agent.into(),
            model: model.into(),
            mode: AppMode::FeatureInput,
            current_goal: None,
            current_state: None,
            workflow_session_id: None,
            goal_start_time: std::time::Instant::now(),
            activity_log: Vec::new(),
            inbox: Vec::new(),
            should_quit: false,
            exit_action: None,
            plan_refinement_pending: false,
            skills_project_root: None,
            active_worktree_display: None,
        };
        Presenter {
            state,
            workflow_event_rx: None,
            answer_tx: None,
            workflow_backend: None,
            workflow_output_dir: None,
            workflow_session_dir: None,
            workflow_conversation_output: None,
            workflow_debug_output: None,
            workflow_debug: false,
            workflow_result: None,
            pending_questions: Vec::new(),
            current_question_index: 0,
            collected_answers: Vec::new(),
            agent_output_buffer: String::new(),
            agent_output_partial_row_active: false,
            awaiting_open_answer: false,
            workflow_handle: None,
            broadcast_tx: None,
            intent_tx: None,
            tool_call_rx: None,
            pending_tool_call_response: None,
            workflow_socket_path: None,
            workflow_worktree_dir: None,
            backend_selection_pending: false,
            deferred_backend_factory: None,
            pending_workflow_start: None,
            deferred_cli_model: None,
            workflow_recipe,
            recipe_slash_selection_pending: false,
            recipe_resolver: None,
            start_slash_structured_run_active: false,
            critical_state: Arc::new(std::sync::Mutex::new(CriticalPresenterState::default())),
        }
    }

    /// Enable broadcast of PresenterEvents (for gRPC subscribers).
    pub fn with_broadcast(mut self, tx: tokio::sync::broadcast::Sender<PresenterEvent>) -> Self {
        self.broadcast_tx = Some(tx);
        self
    }

    /// Enable connect_view() by providing an intent sender for external views.
    pub fn with_intent_sender(mut self, tx: mpsc::Sender<UserIntent>) -> Self {
        self.intent_tx = Some(tx);
        self
    }

    /// Resolve workflow recipe CLI names when the user picks `/recipe` → TDD or Bugfix.
    pub fn with_recipe_resolver(mut self, resolver: Arc<RecipeResolverFn>) -> Self {
        self.recipe_resolver = Some(resolver);
        self
    }

    /// Pre-set worktree dir so the workflow skips git fetch / worktree creation.
    pub fn with_worktree_dir(mut self, dir: PathBuf) -> Self {
        self.workflow_worktree_dir = Some(dir);
        self
    }

    /// Create a new view connection: state snapshot + event subscription + intent sender.
    /// Returns None if broadcast or intent_tx is not configured.
    pub fn connect_view(&self) -> Option<ViewConnection> {
        let broadcast_tx = self.broadcast_tx.as_ref()?;
        let intent_tx = self.intent_tx.clone()?;
        Some(ViewConnection {
            state_snapshot: self.state.clone(),
            event_rx: broadcast_tx.subscribe(),
            intent_tx,
            critical_state: self.critical_state.clone(),
        })
    }

    fn broadcast(&self, event: PresenterEvent) {
        if let Some(ref tx) = self.broadcast_tx {
            let _ = tx.send(event);
        }
    }

    fn broadcast_mode_changed(&mut self) {
        log::debug!(
            "broadcast_mode_changed: mode={:?} plan_refinement_pending={}",
            self.state.mode,
            self.state.plan_refinement_pending
        );
        self.broadcast(PresenterEvent::ModeChanged(ModeChangedDetails {
            mode: self.state.mode.clone(),
            plan_refinement_pending: self.state.plan_refinement_pending,
            skills_project_root: self.state.skills_project_root.clone(),
        }));
    }

    fn prd_body_for_plan_review(&self, content_fallback: &str) -> String {
        self.workflow_session_dir
            .as_ref()
            .and_then(|d| self.workflow_recipe.read_primary_session_document_utf8(d))
            .unwrap_or_else(|| content_fallback.to_string())
    }

    /// Send workflow answer `Approve`, switch to [`AppMode::Running`], and broadcast (shared by DocumentReview and MarkdownViewer).
    fn approve_plan_from_review_or_viewer(&mut self) {
        if let Some(ref tx) = self.answer_tx {
            let _ = tx.send("Approve".to_string());
        }
        self.state.mode = AppMode::Running;
        self.broadcast_mode_changed();
    }

    /// Show interactive backend selection (synthetic single-select question).
    pub fn show_backend_selection(
        &mut self,
        question: ClarificationQuestion,
        initial_selected: usize,
    ) {
        self.backend_selection_pending = true;
        self.pending_questions = vec![question.clone()];
        self.current_question_index = 0;
        self.collected_answers.clear();
        self.state.mode = AppMode::Select {
            question,
            question_index: 0,
            total_questions: 1,
            initial_selected,
        };
        self.broadcast_mode_changed();
    }

    /// Configure backend creation + first workflow start after interactive backend selection (tddy-coder TUI).
    pub fn configure_deferred_workflow_start(
        &mut self,
        factory: DeferredBackendFactory,
        pending: PendingWorkflowStart,
        cli_model_override: Option<String>,
    ) {
        self.state.skills_project_root = Some(pending.output_dir.clone());
        self.deferred_backend_factory = Some(factory);
        self.pending_workflow_start = Some(pending);
        self.deferred_cli_model = cli_model_override;
    }

    /// True while waiting for user to pick a coding backend at session start.
    #[must_use]
    pub fn is_backend_selection_pending(&self) -> bool {
        self.backend_selection_pending
    }

    fn broadcast_error_recovery(&mut self, error_message: String) {
        self.state.mode = AppMode::ErrorRecovery { error_message };
        self.broadcast_mode_changed();
    }

    fn start_workflow_from_pending_if_any(&mut self, backend: SharedBackend) {
        let Some(pending) = self.pending_workflow_start.take() else {
            return;
        };
        self.deferred_cli_model = None;
        self.start_workflow(
            backend,
            pending.output_dir,
            pending.session_dir,
            pending.initial_prompt,
            pending.conversation_output_path,
            pending.debug_output_path,
            pending.debug,
            pending.session_id,
            pending.socket_path,
            pending.tool_call_rx,
        );
    }

    fn apply_deferred_backend_factory(&mut self, factory: DeferredBackendFactory, agent_str: &str) {
        match factory(agent_str) {
            Ok(backend) => self.start_workflow_from_pending_if_any(backend),
            Err(msg) => self.broadcast_error_recovery(msg),
        }
    }

    /// Resolves interactive backend selection (`show_backend_selection`). No-op if the index is invalid.
    fn handle_backend_selection_answer(&mut self, idx: usize) {
        let Some(q) = self.pending_questions.first() else {
            return;
        };
        if idx >= q.options.len() {
            return;
        }
        let label = q.options[idx].label.clone();
        let (agent, model) = crate::backend::backend_from_label(&label);
        let agent_str = agent.to_string();
        self.state.agent = agent_str.clone();
        self.state.model = model.to_string();
        if let Some(ref m) = self.deferred_cli_model {
            self.state.model = m.clone();
        }
        self.backend_selection_pending = false;
        self.pending_questions.clear();
        self.current_question_index = 0;
        self.collected_answers.clear();
        self.state.mode = AppMode::FeatureInput;
        self.broadcast_mode_changed();
        self.broadcast(PresenterEvent::BackendSelected {
            agent: agent_str.clone(),
            model: self.state.model.clone(),
        });
        let Some(factory) = self.deferred_backend_factory.take() else {
            return;
        };
        self.apply_deferred_backend_factory(factory, agent_str.as_str());
    }

    fn handle_recipe_slash_selection_answer(&mut self, idx: usize) {
        let Some(q) = self.pending_questions.first() else {
            self.recipe_slash_selection_pending = false;
            return;
        };
        if idx >= q.options.len() {
            return;
        }
        let label = q.options[idx].label.clone();
        self.recipe_slash_selection_pending = false;
        self.pending_questions.clear();
        self.current_question_index = 0;
        self.collected_answers.clear();

        let Some(cli_name) = crate::backend::recipe_cli_name_from_selection_label(&label) else {
            log::warn!("recipe slash: unknown option label {:?}", label);
            self.state.mode = AppMode::FeatureInput;
            self.broadcast_mode_changed();
            return;
        };
        if let Some(ref resolve) = self.recipe_resolver {
            match resolve(cli_name) {
                Ok(new_recipe) => {
                    log::info!("recipe slash: active workflow recipe set to `{cli_name}`");
                    self.workflow_recipe = new_recipe;
                }
                Err(e) => {
                    log::warn!("recipe slash: could not resolve `{cli_name}`: {e}");
                }
            }
        } else {
            log::debug!("recipe slash: no recipe_resolver; recipe unchanged after UI pick");
        }
        self.state.mode = AppMode::FeatureInput;
        self.broadcast_mode_changed();
    }

    fn select_highlight_matches(&self, idx: usize) -> bool {
        matches!(
            &self.state.mode,
            AppMode::Select {
                initial_selected,
                ..
            } if *initial_selected == idx
        )
    }

    /// Sync presenter Select highlight (for reconnect snapshots). No-op if not in Select mode.
    fn sync_select_highlight(&mut self, idx: usize) {
        let (question, question_index, total_questions) = match &self.state.mode {
            AppMode::Select {
                question,
                question_index,
                total_questions,
                ..
            } => (question.clone(), *question_index, *total_questions),
            _ => return,
        };
        let max = question.options.len() + if question.allow_other { 1 } else { 0 };
        if max == 0 || idx >= max {
            return;
        }
        self.state.mode = AppMode::Select {
            question,
            question_index,
            total_questions,
            initial_selected: idx,
        };
        self.broadcast_mode_changed();
    }

    /// Handle a user intent. Updates state and may send answers to workflow.
    pub fn handle_intent(&mut self, intent: UserIntent) {
        if let UserIntent::SelectHighlightChanged(idx) = &intent {
            if self.select_highlight_matches(*idx) {
                return;
            }
        }
        self.broadcast(PresenterEvent::IntentReceived(intent.clone()));
        match intent {
            UserIntent::SubmitFeatureInput(text) => {
                if text.is_empty() {
                    return;
                }
                if self.try_handle_start_slash_line(&text) {
                    return;
                }
                if let Some(ref dir) = self.workflow_session_dir {
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
                let text_for_restart = if let Some(ref tx) = self.answer_tx {
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
            UserIntent::FeatureSlashBuiltinRecipe => {
                self.apply_feature_slash_builtin_recipe();
            }
            UserIntent::ApproveSessionDocument => {
                self.state.plan_refinement_pending = false;
                log::info!("ApproveSessionDocument: mode={:?}", self.state.mode);
                if matches!(
                    self.state.mode,
                    AppMode::DocumentReview { .. } | AppMode::MarkdownViewer { .. }
                ) {
                    self.approve_plan_from_review_or_viewer();
                }
            }
            UserIntent::ViewSessionDocument => {
                if let AppMode::DocumentReview { ref content } = self.state.mode {
                    let viewer_content = self.prd_body_for_plan_review(content);
                    self.state.plan_refinement_pending = false;
                    self.state.mode = AppMode::MarkdownViewer {
                        content: viewer_content,
                    };
                    self.broadcast_mode_changed();
                }
            }
            UserIntent::RejectSessionDocument => {
                if matches!(
                    self.state.mode,
                    AppMode::DocumentReview { .. } | AppMode::MarkdownViewer { .. }
                ) {
                    self.state.plan_refinement_pending = false;
                    if let Some(ref tx) = self.answer_tx {
                        let _ = tx.send("reject".to_string());
                    }
                }
            }
            UserIntent::RefineSessionDocument => {
                log::info!("RefineSessionDocument: mode={:?}", self.state.mode);
                self.state.plan_refinement_pending = true;
                match self.state.mode.clone() {
                    AppMode::MarkdownViewer { .. } => {
                        log::debug!(
                            "RefineSessionDocument: keep MarkdownViewer; refinement via prompt bar"
                        );
                        self.broadcast_mode_changed();
                    }
                    AppMode::DocumentReview { content } => {
                        let viewer_content = self.prd_body_for_plan_review(&content);
                        self.state.mode = AppMode::MarkdownViewer {
                            content: viewer_content,
                        };
                        log::debug!(
                            "RefineSessionDocument: opened MarkdownViewer from DocumentReview"
                        );
                        self.broadcast_mode_changed();
                    }
                    _ => {
                        log::warn!(
                            "RefineSessionDocument: unexpected mode {:?}; using TextInput fallback",
                            self.state.mode
                        );
                        self.state.mode = AppMode::TextInput {
                            prompt: "Enter refinement feedback:".to_string(),
                        };
                        self.broadcast_mode_changed();
                    }
                }
            }
            UserIntent::DismissViewer => {
                self.state.plan_refinement_pending = false;
                if let AppMode::MarkdownViewer { ref content } = self.state.mode {
                    self.state.mode = AppMode::DocumentReview {
                        content: content.clone(),
                    };
                    self.broadcast_mode_changed();
                }
            }
            UserIntent::AnswerSelect(idx) => {
                if self.backend_selection_pending {
                    self.handle_backend_selection_answer(idx);
                    return;
                }
                if self.recipe_slash_selection_pending {
                    self.handle_recipe_slash_selection_answer(idx);
                    return;
                }
                if let Some(q) = self.pending_questions.get(self.current_question_index) {
                    if idx < q.options.len() {
                        let answer = q.options[idx].label.clone();
                        self.collected_answers.push(answer);
                        self.current_question_index += 1;
                        self.advance_to_next_question();
                        if self.clarification_answers_ready() {
                            self.send_clarification_answers();
                        }
                    }
                }
            }
            UserIntent::AnswerOther(text) => {
                self.collected_answers.push(text);
                self.current_question_index += 1;
                self.advance_to_next_question();
                if self.clarification_answers_ready() {
                    self.send_clarification_answers();
                }
            }
            UserIntent::AnswerMultiSelect(indices, other) => {
                if let Some(q) = self.pending_questions.get(self.current_question_index) {
                    let mut parts: Vec<String> = indices
                        .iter()
                        .filter_map(|&i| q.options.get(i).map(|o| o.label.clone()))
                        .collect();
                    if let Some(o) = other {
                        parts.push(o);
                    }
                    self.collected_answers.push(parts.join(", "));
                    self.current_question_index += 1;
                    self.advance_to_next_question();
                    if self.clarification_answers_ready() {
                        self.send_clarification_answers();
                    }
                }
            }
            UserIntent::AnswerText(text) => {
                if self.state.plan_refinement_pending {
                    log::info!("AnswerText: plan refinement feedback (len={})", text.len());
                    self.state.plan_refinement_pending = false;
                    if let Some(ref tx) = self.answer_tx {
                        let _ = tx.send(text);
                    }
                    self.state.mode = AppMode::Running;
                    self.broadcast_mode_changed();
                } else if matches!(self.state.mode, AppMode::MarkdownViewer { .. })
                    && !text.is_empty()
                {
                    log::info!(
                        "AnswerText: plan refinement (direct entry, len={})",
                        text.len()
                    );
                    if let Some(ref tx) = self.answer_tx {
                        let _ = tx.send(text);
                    }
                    self.state.mode = AppMode::Running;
                    self.broadcast_mode_changed();
                } else {
                    self.collected_answers.push(text);
                    self.current_question_index += 1;
                    self.advance_to_next_question();
                    if self.clarification_answers_ready() {
                        self.send_clarification_answers();
                    }
                }
            }
            UserIntent::QueuePrompt(text) => {
                if text.is_empty() {
                    return;
                }
                if self.awaiting_open_answer {
                    if let Some(ref tx) = self.answer_tx {
                        log::debug!(
                            "QueuePrompt → answer_tx (awaiting_open_answer, len={})",
                            text.len()
                        );
                        let _ = tx.send(text);
                        return;
                    }
                }
                let queued_line = activity_prompt_log::format_queued_prompt_line(&text);
                if !queued_line.is_empty() {
                    self.log_activity(queued_line, ActivityKind::UserPrompt);
                }
                self.state.inbox.push(text);
                self.broadcast(PresenterEvent::InboxChanged(self.state.inbox.clone()));
            }
            UserIntent::EditInboxItem { index, text } => {
                if index < self.state.inbox.len() {
                    self.state.inbox[index] = text;
                    self.broadcast(PresenterEvent::InboxChanged(self.state.inbox.clone()));
                }
            }
            UserIntent::DeleteInboxItem(index) => {
                if index < self.state.inbox.len() {
                    self.state.inbox.remove(index);
                    self.broadcast(PresenterEvent::InboxChanged(self.state.inbox.clone()));
                }
            }
            UserIntent::Scroll(_) => {
                // View-local; no-op in Presenter
            }
            UserIntent::SelectHighlightChanged(idx) => {
                self.sync_select_highlight(idx);
            }
            UserIntent::Quit => {
                self.state.should_quit = true;
            }
            UserIntent::Interrupt => {
                // TUI / VirtualTui call `ctrl_c_interrupt_session` without sending this intent.
            }
            UserIntent::ContinueWithAgent => {
                let session_id = if let Some(cs_dir) = self.changeset_read_dir() {
                    log::info!(
                        "ContinueWithAgent: read_changeset from {} (workflow_session_dir={:?}, workflow_output_dir={:?})",
                        cs_dir.display(),
                        self.workflow_session_dir.as_ref().map(|p| p.display().to_string()),
                        self.workflow_output_dir.as_ref().map(|p| p.display().to_string()),
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
                    log::warn!(
                        "ContinueWithAgent: no workflow_session_dir or workflow_output_dir set"
                    );
                    None
                };
                if let Some(sid) = session_id {
                    self.state.exit_action =
                        Some(crate::presenter::state::ExitAction::ContinueWithAgent {
                            session_id: sid,
                        });
                    self.state.should_quit = true;
                    self.broadcast(PresenterEvent::ShouldQuit);
                } else {
                    log::warn!(
                        "ContinueWithAgent: no session id resolved; not setting exit_action or ShouldQuit"
                    );
                }
            }
            UserIntent::ResumeFromError => {
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
                        self.workflow_session_dir,
                        self.workflow_output_dir,
                        self.state.current_goal
                    );
                    None
                };
                self.state.mode = AppMode::Running;
                self.broadcast_mode_changed();
                if let (Some(backend), Some(output_dir)) = (
                    self.workflow_backend.clone(),
                    self.workflow_output_dir.clone(),
                ) {
                    if let Some(h) = self.workflow_handle.take() {
                        let _ = h.join();
                    }
                    self.spawn_workflow(
                        backend,
                        output_dir,
                        self.workflow_session_dir.clone(),
                        None,
                        self.workflow_conversation_output.clone(),
                        self.workflow_debug_output.clone(),
                        self.workflow_debug,
                        session_id,
                        self.workflow_socket_path.clone(),
                        self.workflow_worktree_dir.clone(),
                    );
                }
            }
        }
    }

    fn clarification_answers_ready(&self) -> bool {
        !self.pending_questions.is_empty()
            && self.current_question_index >= self.pending_questions.len()
            && matches!(self.state.mode, AppMode::Running)
    }

    fn send_clarification_answers(&mut self) {
        let answers = self.collect_answers();
        let is_approve = matches!(
            self.pending_tool_call_response,
            Some(PendingToolCallResponse::Approve(_))
        );
        let is_tool_call = self.pending_tool_call_response.is_some();
        if let Some(pending) = self.pending_tool_call_response.take() {
            match pending {
                PendingToolCallResponse::Ask(tx) => {
                    let _ = tx.send(ToolCallResponse::AskAnswer {
                        answers: answers.clone(),
                    });
                    // `tddy-tools ask` does not go through WaitForInput; merge answers into workflow
                    // context via grill hooks reading this file in `after_task("grill")`.
                    if let Some(ref dir) = self.workflow_session_dir {
                        let wf = dir.join(".workflow");
                        if let Err(e) = std::fs::create_dir_all(&wf) {
                            log::warn!("grill ask answers: create_dir_all {}: {}", wf.display(), e);
                        } else {
                            let path = wf.join("grill_ask_answers.txt");
                            if let Err(e) = std::fs::write(&path, &answers) {
                                log::warn!("grill ask answers: write {}: {}", path.display(), e);
                            } else {
                                log::debug!(
                                    "grill ask answers: wrote {} bytes to {}",
                                    answers.len(),
                                    path.display()
                                );
                            }
                        }
                    }
                }
                PendingToolCallResponse::Approve(tx) => {
                    let allow = self
                        .collected_answers
                        .first()
                        .map(|a| a.eq_ignore_ascii_case("Allow"))
                        .unwrap_or(false);
                    let _ = tx.send(ToolCallResponse::ApproveResult { allow });
                }
            }
        } else if let Some(ref answer_tx) = self.answer_tx {
            let _ = answer_tx.send(answers.clone());
        }
        if is_tool_call {
            let preview: String = answers.chars().take(80).collect();
            let suffix = if answers.len() > 80 { "…" } else { "" };
            let msg = if is_approve {
                format!("✓ permission: {}{}", preview, suffix)
            } else {
                format!("✓ ask answered: {}{}", preview, suffix)
            };
            self.log_activity(msg, ActivityKind::ToolUse);
        }
    }

    fn collect_answers(&self) -> String {
        self.collected_answers.join("\n")
    }

    fn advance_to_next_question(&mut self) {
        if self.current_question_index >= self.pending_questions.len() {
            self.state.mode = AppMode::Running;
            self.broadcast_mode_changed();
        } else {
            let q = self.pending_questions[self.current_question_index].clone();
            let total = self.pending_questions.len();
            if q.multi_select {
                self.state.mode = AppMode::MultiSelect {
                    question: q,
                    question_index: self.current_question_index,
                    total_questions: total,
                };
            } else {
                self.state.mode = AppMode::Select {
                    question: q,
                    question_index: self.current_question_index,
                    total_questions: total,
                    initial_selected: 0,
                };
            }
            self.broadcast_mode_changed();
        }
    }

    fn flush_agent_output_buffer(&mut self) {
        if !self.agent_output_buffer.is_empty() {
            let line = std::mem::take(&mut self.agent_output_buffer);
            log::debug!(
                "flush_agent_output_buffer: len={}, partial_row_active={}",
                line.len(),
                self.agent_output_partial_row_active
            );
            // Avoid a duplicate `activity_log` row when the partial row already shows this text.
            if self.agent_output_partial_row_active {
                if let Some(last) = self.state.activity_log.last() {
                    if last.kind == ActivityKind::AgentOutput && last.text == line {
                        self.agent_output_partial_row_active = false;
                        self.broadcast(PresenterEvent::ActivityLogged(ActivityEntry {
                            text: line,
                            kind: ActivityKind::AgentOutput,
                        }));
                        return;
                    }
                }
                self.agent_output_partial_row_active = false;
            }
            self.log_activity(line, ActivityKind::AgentOutput);
        }
    }

    /// Completes a full agent line in `activity_log` after a newline (no `ActivityLogged` broadcast;
    /// streaming consumers use [`PresenterEvent::AgentOutput`]).
    fn finalize_agent_line_in_activity_log(&mut self, line: String) {
        if line.is_empty() {
            return;
        }
        log::debug!(
            "finalize_agent_line_in_activity_log: len={}, partial_row_active={}",
            line.len(),
            self.agent_output_partial_row_active
        );
        if self.agent_output_partial_row_active {
            if let Some(last) = self.state.activity_log.last_mut() {
                if last.kind == ActivityKind::AgentOutput {
                    last.text = line;
                    self.agent_output_partial_row_active = false;
                    return;
                }
            }
            self.agent_output_partial_row_active = false;
        }
        self.state.activity_log.push(ActivityEntry {
            text: line,
            kind: ActivityKind::AgentOutput,
        });
    }

    /// Syncs the visible tail of the current incomplete agent line into `activity_log` (incremental).
    fn sync_agent_partial_activity_log(&mut self) {
        let tail = agent_activity::visible_tail_for_incremental_log(&self.agent_output_buffer);
        if tail.is_empty() {
            return;
        }
        log::debug!(
            "sync_agent_partial_activity_log: tail_len={}, partial_row_active={}",
            tail.len(),
            self.agent_output_partial_row_active
        );
        if self.agent_output_partial_row_active {
            if let Some(last) = self.state.activity_log.last_mut() {
                if last.kind == ActivityKind::AgentOutput {
                    last.text = tail;
                    return;
                }
            }
        }
        self.state.activity_log.push(ActivityEntry {
            text: tail,
            kind: ActivityKind::AgentOutput,
        });
        self.agent_output_partial_row_active = true;
    }

    fn log_activity(&mut self, text: String, kind: ActivityKind) {
        let entry = ActivityEntry { text, kind };
        self.state.activity_log.push(entry.clone());
        self.broadcast(PresenterEvent::ActivityLogged(entry));
    }

    /// Poll for tool call requests (tddy-tools relay). Call from main loop.
    pub fn poll_tool_calls(&mut self) {
        let rx = match self.tool_call_rx.as_ref() {
            Some(r) => r,
            None => return,
        };
        let mut requests = Vec::new();
        while let Ok(req) = rx.try_recv() {
            requests.push(req);
        }
        for req in requests {
            match req {
                ToolCallRequest::SubmitActivity { goal, .. } => {
                    self.log_activity(
                        format!("⚙ tddy-tools submit (goal: {})", goal),
                        ActivityKind::ToolUse,
                    );
                    self.log_activity(
                        format!("✓ submit accepted (goal: {})", goal),
                        ActivityKind::ToolUse,
                    );
                }
                ToolCallRequest::Ask {
                    questions,
                    response_tx,
                } => {
                    let summary: Vec<String> = questions
                        .iter()
                        .map(|q| {
                            let truncated: String = q.question.chars().take(60).collect();
                            if q.question.len() > 60 {
                                format!("{}…", truncated)
                            } else {
                                truncated
                            }
                        })
                        .collect();
                    self.log_activity(
                        format!(
                            "⚙ tddy-tools ask ({} question{}): {}",
                            questions.len(),
                            if questions.len() == 1 { "" } else { "s" },
                            summary.join(" | ")
                        ),
                        ActivityKind::ToolUse,
                    );
                    self.log_activity(
                        "Answer in the TUI question strip at the top (↑/↓ Enter). Not in Cursor."
                            .to_string(),
                        ActivityKind::ToolUse,
                    );
                    self.flush_agent_output_buffer();
                    self.pending_questions = questions;
                    self.current_question_index = 0;
                    self.collected_answers.clear();
                    self.pending_tool_call_response =
                        Some(PendingToolCallResponse::Ask(response_tx));
                    self.advance_to_next_question();
                }
                ToolCallRequest::Approve {
                    tool_name,
                    input,
                    response_tx,
                } => {
                    let detail = match input.get("command").and_then(|c| c.as_str()) {
                        Some(cmd) => {
                            if cmd.len() > 80 {
                                format!("{}…", &cmd[..80])
                            } else {
                                cmd.to_string()
                            }
                        }
                        None => {
                            let s = input.to_string();
                            if s.len() > 80 {
                                format!("{}…", &s[..80])
                            } else {
                                s
                            }
                        }
                    };
                    self.log_activity(
                        format!("⚙ Permission request: {} — {}", tool_name, detail),
                        ActivityKind::ToolUse,
                    );
                    self.flush_agent_output_buffer();
                    let question = ClarificationQuestion {
                        header: "Permission".to_string(),
                        question: format!("Allow {}?", detail),
                        options: vec![
                            QuestionOption {
                                label: "Allow".to_string(),
                                description: "Allow this tool".to_string(),
                            },
                            QuestionOption {
                                label: "Deny".to_string(),
                                description: "Deny this tool".to_string(),
                            },
                        ],
                        multi_select: false,
                        allow_other: false,
                    };
                    self.pending_questions = vec![question];
                    self.current_question_index = 0;
                    self.collected_answers.clear();
                    self.pending_tool_call_response =
                        Some(PendingToolCallResponse::Approve(response_tx));
                    self.advance_to_next_question();
                }
            }
        }
    }

    /// Poll for workflow events. Call from main loop.
    /// Drains all pending events per call to minimize latency between tasks.
    pub fn poll_workflow(&mut self) {
        let rx = match self.workflow_event_rx.as_ref() {
            Some(r) => r,
            None => return,
        };

        let mut events = Vec::new();
        while let Ok(ev) = rx.try_recv() {
            events.push(ev);
        }

        for ev in events {
            match ev {
                WorkflowEvent::Progress(pev) => {
                    if let crate::ProgressEvent::SessionStarted { session_id } = &pev {
                        log::info!(
                            "Workflow engine session started; TUI status segment will use id prefix"
                        );
                        log::debug!(
                            "workflow_session_id set for status bar: {}",
                            format_session_id_for_log(session_id)
                        );
                        self.state.workflow_session_id = Some(session_id.clone());
                    }
                    let entry = match &pev {
                        crate::ProgressEvent::ToolUse {
                            name,
                            detail: Some(d),
                        } => ActivityEntry {
                            text: format!("Tool: {} {}", name, d),
                            kind: ActivityKind::ToolUse,
                        },
                        crate::ProgressEvent::ToolUse { name, detail: None } => ActivityEntry {
                            text: format!("Tool: {}", name),
                            kind: ActivityKind::ToolUse,
                        },
                        crate::ProgressEvent::TaskStarted { description } => ActivityEntry {
                            text: description.clone(),
                            kind: ActivityKind::TaskStarted,
                        },
                        crate::ProgressEvent::TaskProgress { description, .. } => ActivityEntry {
                            text: description.clone(),
                            kind: ActivityKind::TaskProgress,
                        },
                        crate::ProgressEvent::SessionStarted { .. } => ActivityEntry {
                            text: "Session connected".to_string(),
                            kind: ActivityKind::Info,
                        },
                        crate::ProgressEvent::AgentExited { exit_code, goal } => ActivityEntry {
                            text: format!("Agent exited (code {}) for {}", exit_code, goal),
                            kind: ActivityKind::Info,
                        },
                    };
                    self.state.activity_log.push(entry.clone());
                    self.broadcast(PresenterEvent::ActivityLogged(entry));
                }
                WorkflowEvent::StateChange { from, to } => {
                    self.state.current_state = Some(to.clone());
                    if let Ok(mut cs) = self.critical_state.lock() {
                        cs.current_state = Some(to.clone());
                    }
                    let entry = ActivityEntry {
                        text: format!("State: {} → {}", from, to),
                        kind: ActivityKind::StateChange,
                    };
                    self.state.activity_log.push(entry.clone());
                    self.broadcast(PresenterEvent::ActivityLogged(entry));
                    self.broadcast(PresenterEvent::StateChanged {
                        from: from.clone(),
                        to: to.clone(),
                    });
                }
                WorkflowEvent::GoalStarted(goal) => {
                    self.awaiting_open_answer = false;
                    self.state.current_goal = Some(goal.clone());
                    if let Ok(mut cs) = self.critical_state.lock() {
                        cs.current_goal = Some(goal.clone());
                    }
                    self.state.goal_start_time = std::time::Instant::now();
                    if matches!(self.state.mode, AppMode::FeatureInput) {
                        self.state.mode = AppMode::Running;
                        self.broadcast_mode_changed();
                    }
                    self.broadcast(PresenterEvent::GoalStarted(goal.clone()));
                }
                WorkflowEvent::ClarificationNeeded { questions } => {
                    self.flush_agent_output_buffer();
                    self.awaiting_open_answer = questions.is_empty();
                    self.pending_questions = questions;
                    self.current_question_index = 0;
                    self.collected_answers.clear();
                    self.advance_to_next_question();
                }
                WorkflowEvent::AwaitingFeatureInput => {
                    self.state.mode = AppMode::FeatureInput;
                    self.broadcast_mode_changed();
                }
                WorkflowEvent::SessionDocumentApprovalNeeded { content } => {
                    self.flush_agent_output_buffer();
                    self.state.mode = AppMode::DocumentReview { content };
                    self.broadcast_mode_changed();
                    // Resync goal/state in case client missed GoalStarted/StateChanged due to broadcast Lagged
                    if let Some(ref g) = self.state.current_goal {
                        self.broadcast(PresenterEvent::GoalStarted(g.clone()));
                    }
                    if let Some(ref s) = self.state.current_state {
                        self.broadcast(PresenterEvent::StateChanged {
                            from: "Planning".to_string(),
                            to: s.clone(),
                        });
                    }
                }
                WorkflowEvent::WorktreeSwitched { ref path } => {
                    let entry = ActivityEntry {
                        text: format!("Worktree: {}", path.display()),
                        kind: ActivityKind::Info,
                    };
                    self.state.activity_log.push(entry.clone());
                    self.broadcast(PresenterEvent::ActivityLogged(entry));
                    let wtd = format_worktree_for_status_bar(path.as_path());
                    if !wtd.is_empty() {
                        log::info!("WorktreeSwitched: active_worktree_display set to {:?}", wtd);
                        self.state.active_worktree_display = Some(wtd);
                    } else {
                        log::debug!(
                            "WorktreeSwitched: format_worktree_for_status_bar returned empty for {:?}",
                            path
                        );
                    }
                }
                WorkflowEvent::WorkflowComplete(result) => {
                    self.awaiting_open_answer = false;
                    self.flush_agent_output_buffer();
                    self.workflow_result = Some(result.clone());
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
                            self.workflow_backend.clone(),
                            self.workflow_output_dir.clone(),
                        ) {
                            if let Some(h) = self.workflow_handle.take() {
                                let _ = h.join();
                            }
                            self.workflow_result = None;
                            self.state.workflow_session_id = None;
                            log::debug!(
                                "WorkflowComplete: inbox restart — cleared workflow_session_id until SessionStarted"
                            );
                            self.spawn_workflow(
                                backend,
                                output_dir,
                                session_dir,
                                Some(prefixed),
                                self.workflow_conversation_output.clone(),
                                self.workflow_debug_output.clone(),
                                self.workflow_debug,
                                None,
                                self.workflow_socket_path.clone(),
                                self.workflow_worktree_dir.clone(),
                            );
                        }
                    } else {
                        match &result {
                            Ok(_) => {
                                self.finish_start_slash_structured_run_if_needed();
                            }
                            Err(_) => {
                                self.start_slash_structured_run_active = false;
                            }
                        }
                        match result {
                            Err(ref msg) => {
                                log::error!("Workflow failed: {}", msg);
                                self.state.workflow_session_id = None;
                                self.log_activity(
                                    format!("Workflow failed: {}", msg),
                                    ActivityKind::Info,
                                );
                                self.state.mode = AppMode::ErrorRecovery {
                                    error_message: msg.clone(),
                                };
                            }
                            Ok(_) => {
                                log::info!(
                                    "WorkflowComplete Ok → FeatureInput (ready for new workflow)"
                                );
                                self.state.workflow_session_id = None;
                                self.state.mode = AppMode::FeatureInput;
                            }
                        }
                        self.broadcast_mode_changed();
                    }
                }
                WorkflowEvent::AgentOutput(text) => {
                    agent_activity::on_agent_chunk_received(&text);
                    let channels = agent_activity::authoritative_channels_per_completed_line();
                    log::info!(
                        "poll_workflow: AgentOutput chunk len={}, policy_authoritative_channels={}",
                        text.len(),
                        channels
                    );
                    for part in text.split_inclusive('\n') {
                        if part.ends_with('\n') {
                            self.agent_output_buffer
                                .push_str(part.trim_end_matches('\n'));
                            let line = std::mem::take(&mut self.agent_output_buffer);
                            if !line.is_empty() {
                                self.finalize_agent_line_in_activity_log(line);
                            }
                        } else {
                            self.agent_output_buffer.push_str(part);
                        }
                    }
                    self.sync_agent_partial_activity_log();
                    self.broadcast(PresenterEvent::AgentOutput(text.clone()));
                }
            }
        }
    }

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
        self.workflow_backend = Some(backend.clone());
        self.workflow_output_dir = Some(output_dir.clone());
        self.state.skills_project_root = Some(output_dir.clone());
        self.workflow_session_dir = session_dir.clone();
        self.workflow_conversation_output = conversation_output_path.clone();
        self.workflow_debug_output = debug_output_path.clone();
        self.workflow_debug = debug;
        self.tool_call_rx = tool_call_rx;
        self.workflow_socket_path = socket_path.clone();
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
            self.workflow_worktree_dir.clone(),
        );
    }

    /// Starts another workflow run with `prompt`. Reuses [`Self::workflow_session_dir`] when set so
    /// web/daemon/CLI-bound session folders keep receiving runs; only passes `None` when the first
    /// run also had no session dir (fresh allocation under `TDDY_SESSIONS_DIR`).
    fn restart_workflow(&mut self, prompt: String) {
        if let (Some(backend), Some(output_dir)) = (
            self.workflow_backend.clone(),
            self.workflow_output_dir.clone(),
        ) {
            if let Some(h) = self.workflow_handle.take() {
                let _ = h.join();
            }
            self.workflow_result = None;
            self.state.mode = AppMode::Running;
            self.broadcast_mode_changed();
            self.spawn_workflow(
                backend,
                output_dir,
                self.workflow_session_dir.clone(),
                Some(prompt),
                self.workflow_conversation_output.clone(),
                self.workflow_debug_output.clone(),
                self.workflow_debug,
                None,
                self.workflow_socket_path.clone(),
                self.workflow_worktree_dir.clone(),
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
        let recipe = self.workflow_recipe.clone();
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
            );
        });

        self.workflow_event_rx = Some(event_rx);
        self.answer_tx = Some(answer_tx);
        self.workflow_handle = Some(handle);
    }

    /// Prefer session/plan dir for `changeset.yaml`; fall back to `workflow_output_dir`.
    fn changeset_read_dir(&self) -> Option<&PathBuf> {
        self.workflow_session_dir
            .as_ref()
            .or(self.workflow_output_dir.as_ref())
    }

    /// After a successful `/start-*` structured run, switch active recipe back to free prompting.
    fn finish_start_slash_structured_run_if_needed(&mut self) {
        if !self.start_slash_structured_run_active {
            return;
        }
        self.start_slash_structured_run_active = false;
        let Some(ref resolve) = self.recipe_resolver else {
            log::debug!("finish_start_slash_structured_run: no recipe_resolver");
            return;
        };
        let fp_name = crate::feature_start_slash::DEFAULT_UNSPECIFIED_WORKFLOW_RECIPE_CLI_NAME;
        match resolve(fp_name) {
            Ok(r) => {
                self.workflow_recipe = r;
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
    /// Returns `true` if the line was a start-slash command (even if invalid or resolver missing).
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
                let Some(ref resolve) = self.recipe_resolver else {
                    log::debug!("try_handle_start_slash_line: no recipe_resolver; ignoring");
                    return true;
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
                        self.start_slash_structured_run_active = structured;
                        self.workflow_recipe = new_recipe;
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

    /// Reference to current state.
    pub fn state(&self) -> &PresenterState {
        &self.state
    }

    /// True when workflow is complete (workflow_result is set).
    pub fn is_done(&self) -> bool {
        self.workflow_result.is_some()
    }

    /// Take the workflow result (if any) for printing on TUI exit.
    pub fn take_workflow_result(&mut self) -> Option<Result<WorkflowCompletePayload, String>> {
        self.workflow_result.take()
    }

    /// User accepted the `/recipe` built-in from the feature slash menu (PRD).
    pub fn apply_feature_slash_builtin_recipe(&mut self) {
        if !matches!(self.state.mode, AppMode::FeatureInput) {
            log::debug!(
                "apply_feature_slash_builtin_recipe: no-op (mode={:?})",
                self.state.mode
            );
            return;
        }
        log::info!("apply_feature_slash_builtin_recipe: showing workflow recipe selection");
        self.recipe_slash_selection_pending = true;
        self.pending_questions = vec![crate::backend::workflow_recipe_selection_question()];
        self.current_question_index = 0;
        self.collected_answers.clear();
        self.advance_to_next_question();
    }

    /// Whether the presenter is in recipe selection after `/recipe` from slash menu.
    pub fn recipe_slash_selection_active(&self) -> bool {
        let active = self.recipe_slash_selection_pending
            && matches!(self.state.mode, AppMode::Select { .. });
        log::debug!("recipe_slash_selection_active: {active}");
        active
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presenter::state::AppMode;
    use crate::{ClarificationQuestion, QuestionOption};

    fn make_presenter() -> Presenter {
        Presenter::new(
            "agent",
            "model",
            std::sync::Arc::new(crate::presenter::presenter_test_recipe::EmptyPresenterTestRecipe)
                as std::sync::Arc<dyn WorkflowRecipe>,
        )
    }

    fn inject_workflow_event(presenter: &mut Presenter, event: WorkflowEvent) {
        let (tx, rx) = mpsc::channel();
        tx.send(event).unwrap();
        presenter.workflow_event_rx = Some(rx);
    }

    fn inject_workflow_events(presenter: &mut Presenter, events: Vec<WorkflowEvent>) {
        let (tx, rx) = mpsc::channel();
        for e in events {
            tx.send(e).unwrap();
        }
        drop(tx);
        presenter.workflow_event_rx = Some(rx);
    }

    #[test]
    fn progress_session_started_sets_workflow_session_id() {
        let mut p = make_presenter();
        let sid = "550e8400-e29b-41d4-a716-446655440000";
        inject_workflow_event(
            &mut p,
            WorkflowEvent::Progress(crate::ProgressEvent::SessionStarted {
                session_id: sid.to_string(),
            }),
        );
        p.poll_workflow();
        assert_eq!(p.state().workflow_session_id.as_deref(), Some(sid));
    }

    #[test]
    fn workflow_complete_ok_clears_workflow_session_id_after_session_started() {
        let mut p = make_presenter();
        inject_workflow_events(
            &mut p,
            vec![
                WorkflowEvent::Progress(crate::ProgressEvent::SessionStarted {
                    session_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
                }),
                WorkflowEvent::WorkflowComplete(Ok(WorkflowCompletePayload {
                    summary: "done".to_string(),
                    session_dir: None,
                })),
            ],
        );
        p.poll_workflow();
        assert!(
            p.state().workflow_session_id.is_none(),
            "expected workflow_session_id cleared after successful completion"
        );
    }

    #[test]
    fn workflow_complete_err_clears_workflow_session_id_after_session_started() {
        let mut p = make_presenter();
        inject_workflow_events(
            &mut p,
            vec![
                WorkflowEvent::Progress(crate::ProgressEvent::SessionStarted {
                    session_id: "deadbeef-cafe-0000-0000-000000000001".to_string(),
                }),
                WorkflowEvent::WorkflowComplete(Err("boom".to_string())),
            ],
        );
        p.poll_workflow();
        assert!(
            p.state().workflow_session_id.is_none(),
            "expected workflow_session_id cleared after error completion"
        );
    }

    #[test]
    fn test_workflow_error_transitions_to_error_recovery() {
        let mut p = make_presenter();
        inject_workflow_event(
            &mut p,
            WorkflowEvent::WorkflowComplete(Err("backend timeout".to_string())),
        );
        p.poll_workflow();
        assert!(
            matches!(
                p.state().mode,
                AppMode::ErrorRecovery { ref error_message } if error_message == "backend timeout"
            ),
            "Expected ErrorRecovery mode with correct message, got {:?}",
            p.state().mode
        );
    }

    #[test]
    fn test_workflow_success_transitions_to_feature_input() {
        let mut p = make_presenter();
        inject_workflow_event(
            &mut p,
            WorkflowEvent::WorkflowComplete(Ok(WorkflowCompletePayload {
                summary: "all done".to_string(),
                session_dir: None,
            })),
        );
        p.poll_workflow();
        assert!(
            matches!(p.state().mode, AppMode::FeatureInput),
            "Expected FeatureInput mode (ready for new workflow), got {:?}",
            p.state().mode
        );
    }

    #[test]
    fn awaiting_feature_input_event_switches_to_feature_input_mode() {
        let mut p = make_presenter();
        inject_workflow_event(&mut p, WorkflowEvent::AwaitingFeatureInput);
        p.poll_workflow();
        assert!(
            matches!(p.state().mode, AppMode::FeatureInput),
            "Expected FeatureInput when plan awaits description, got {:?}",
            p.state().mode
        );
    }

    #[test]
    fn continue_with_agent_sets_exit_action_and_quits_when_session_available() {
        let mut p = make_presenter();

        // Create a temp dir with a changeset that has a session_id.
        let tmp = std::env::temp_dir().join("tddy-test-continue-agent-exit");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let mut cs = crate::changeset::Changeset::default();
        cs.state.session_id = Some("agent-session-42".to_string());
        crate::changeset::write_changeset(&tmp, &cs).unwrap();

        // Set up presenter with output_dir pointing to changeset location.
        p.workflow_output_dir = Some(tmp.clone());

        // Put presenter in ErrorRecovery mode (precondition for this intent).
        p.state.mode = AppMode::ErrorRecovery {
            error_message: "test error".to_string(),
        };

        p.handle_intent(UserIntent::ContinueWithAgent);

        assert!(
            p.state().should_quit,
            "ContinueWithAgent should set should_quit when session is available"
        );
        assert!(
            matches!(
                p.state().exit_action,
                Some(crate::presenter::state::ExitAction::ContinueWithAgent { ref session_id })
                if session_id == "agent-session-42"
            ),
            "exit_action should be ContinueWithAgent with the session_id from changeset, got {:?}",
            p.state().exit_action
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn continue_with_agent_stays_in_error_recovery_when_no_session() {
        let mut p = make_presenter();

        // Create a temp dir with a changeset that has NO session_id.
        let tmp = std::env::temp_dir().join("tddy-test-continue-agent-no-session");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let cs = crate::changeset::Changeset::default(); // session_id is None
        crate::changeset::write_changeset(&tmp, &cs).unwrap();

        p.workflow_output_dir = Some(tmp.clone());
        p.state.mode = AppMode::ErrorRecovery {
            error_message: "test error".to_string(),
        };

        p.handle_intent(UserIntent::ContinueWithAgent);

        assert!(
            !p.state().should_quit,
            "ContinueWithAgent should NOT quit when no session_id is available"
        );
        assert!(
            p.state().exit_action.is_none(),
            "exit_action should remain None when no session_id"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Cursor stores the agent thread id on the session list (tagged `evaluate`, `green`, etc.);
    /// `state.session_id` may be unset. Continue with agent must still resolve a session id
    /// (same idea as `ResumeFromError` via `get_session_for_tag`), otherwise the user stays in
    /// error recovery and no `claude --resume` runs.
    ///
    /// Reproduces: session `019d105b-ac0f-78d3-9a89-409731145a36` visible in logs but Continue
    /// with agent appeared to do nothing.
    ///
    /// Note: choosing **Resume** restarts the workflow from `next_goal_for_state`; if the next
    /// step is `validate` and the backend is Cursor, invocation fails with
    /// `validate is not supported on the Cursor backend` — that is a different path from
    /// Continue with agent (exec resume).
    #[test]
    fn continue_with_agent_resolves_tagged_session_when_state_session_id_missing() {
        let mut p = make_presenter();

        let tmp = std::env::temp_dir().join("tddy-test-continue-agent-tag-fallback");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let cursor_thread = "019d105b-ac0f-78d3-9a89-409731145a36";
        let mut cs = crate::changeset::Changeset::default();
        cs.state.session_id = None;
        cs.sessions.push(crate::changeset::SessionEntry {
            id: cursor_thread.to_string(),
            agent: "cursor".to_string(),
            tag: "evaluate".to_string(),
            created_at: "2026-03-21T12:00:00Z".to_string(),
            system_prompt_file: None,
        });
        crate::changeset::write_changeset(&tmp, &cs).unwrap();

        p.workflow_output_dir = Some(tmp.clone());
        p.state.current_goal = Some("evaluate".to_string());
        p.state.mode = AppMode::ErrorRecovery {
            error_message: "validate is not supported on the Cursor backend".to_string(),
        };

        p.handle_intent(UserIntent::ContinueWithAgent);

        assert!(
            p.state().should_quit,
            "ContinueWithAgent should quit to exec claude --resume when a tagged session exists"
        );
        assert!(
            matches!(
                p.state().exit_action,
                Some(crate::presenter::state::ExitAction::ContinueWithAgent { ref session_id })
                if session_id == cursor_thread
            ),
            "exit_action should use session id from get_session_for_tag(evaluate), got {:?}",
            p.state().exit_action
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// When the failing goal has no matching tagged session (e.g. validate failed before a
    /// validate session was recorded), `ContinueWithAgent` must still resolve an agent session
    /// from the changeset so Enter does not appear to do nothing.
    #[test]
    fn continue_with_agent_resolves_session_when_current_goal_tag_has_no_entry() {
        let mut p = make_presenter();

        let tmp = std::env::temp_dir().join("tddy-test-continue-agent-wrong-tag");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let sid = "session-from-prior-step";
        let mut cs = crate::changeset::Changeset::default();
        cs.state.session_id = None;
        cs.sessions.push(crate::changeset::SessionEntry {
            id: sid.to_string(),
            agent: "cursor".to_string(),
            tag: "evaluate".to_string(),
            created_at: "2026-03-21T12:00:00Z".to_string(),
            system_prompt_file: None,
        });
        crate::changeset::write_changeset(&tmp, &cs).unwrap();

        p.workflow_output_dir = Some(tmp.clone());
        p.state.current_goal = Some("validate".to_string());
        p.state.mode = AppMode::ErrorRecovery {
            error_message: "validate is not supported on the Cursor backend".to_string(),
        };

        p.handle_intent(UserIntent::ContinueWithAgent);

        assert!(
            p.state().should_quit,
            "ContinueWithAgent must quit with a resume session when any session exists in changeset"
        );
        assert!(
            matches!(
                p.state().exit_action,
                Some(crate::presenter::state::ExitAction::ContinueWithAgent { ref session_id })
                if session_id == sid
            ),
            "expected resume id from an existing session entry when goal tag misses, got {:?}",
            p.state().exit_action
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// `start_workflow` passes `output_dir` as `.` while `session_dir` points at the session folder
    /// (`~/.tddy/sessions/...`). Continue with agent must read `changeset.yaml` from `session_dir`.
    #[test]
    fn continue_with_agent_reads_changeset_from_workflow_session_dir() {
        let mut p = make_presenter();

        let tmp_plan = std::env::temp_dir().join("tddy-test-continue-plan-dir");
        let tmp_wrong = std::env::temp_dir().join("tddy-test-continue-wrong-dir");
        let _ = std::fs::remove_dir_all(&tmp_plan);
        let _ = std::fs::remove_dir_all(&tmp_wrong);
        std::fs::create_dir_all(&tmp_plan).unwrap();
        std::fs::create_dir_all(&tmp_wrong).unwrap();

        let resume_id = "resume-from-plan-dir";
        let mut cs = crate::changeset::Changeset::default();
        cs.state.session_id = Some(resume_id.to_string());
        crate::changeset::write_changeset(&tmp_plan, &cs).unwrap();

        p.workflow_output_dir = Some(tmp_wrong.clone());
        p.workflow_session_dir = Some(tmp_plan.clone());
        p.state.mode = AppMode::ErrorRecovery {
            error_message: "read refactoring-plan.md: No such file or directory (os error 2)"
                .to_string(),
        };

        p.handle_intent(UserIntent::ContinueWithAgent);

        assert!(p.state().should_quit);
        assert!(
            matches!(
                p.state().exit_action,
                Some(crate::presenter::state::ExitAction::ContinueWithAgent { ref session_id })
                if session_id == resume_id
            ),
            "expected session id from changeset at workflow_session_dir, got {:?}",
            p.state().exit_action
        );

        let _ = std::fs::remove_dir_all(&tmp_plan);
        let _ = std::fs::remove_dir_all(&tmp_wrong);
    }

    #[test]
    fn connect_view_returns_none_without_broadcast() {
        let p = make_presenter();
        assert!(p.connect_view().is_none());
    }

    #[test]
    fn connect_view_returns_none_without_intent_tx() {
        let (tx, _) = tokio::sync::broadcast::channel(16);
        let p = make_presenter().with_broadcast(tx);
        assert!(p.connect_view().is_none());
    }

    #[test]
    fn connect_view_returns_connection_with_matching_snapshot() {
        let (event_tx, _) = tokio::sync::broadcast::channel(16);
        let (intent_tx, _) = mpsc::channel();
        let p = make_presenter()
            .with_broadcast(event_tx)
            .with_intent_sender(intent_tx);
        let conn = p.connect_view().expect("connect_view should return Some");
        assert_eq!(conn.state_snapshot.agent, "agent");
        assert_eq!(conn.state_snapshot.model, "model");
        assert!(matches!(conn.state_snapshot.mode, AppMode::FeatureInput));
    }

    #[test]
    fn connect_view_event_rx_receives_broadcast_events() {
        let (event_tx, _) = tokio::sync::broadcast::channel(16);
        let (intent_tx, _) = mpsc::channel();
        let p = make_presenter()
            .with_broadcast(event_tx.clone())
            .with_intent_sender(intent_tx);
        let mut conn = p.connect_view().expect("connect_view should return Some");
        let _ = event_tx.send(PresenterEvent::GoalStarted("plan".to_string()));
        let ev = conn.event_rx.try_recv();
        assert!(
            matches!(ev, Ok(PresenterEvent::GoalStarted(ref g)) if g == "plan"),
            "Expected GoalStarted event, got {:?}",
            ev
        );
    }

    #[test]
    fn show_backend_selection_transitions_to_select_mode() {
        let mut p = make_presenter();
        let q = crate::backend::backend_selection_question();
        p.show_backend_selection(q, 0);
        assert!(matches!(p.state().mode, AppMode::Select { .. }));
        assert!(p.is_backend_selection_pending());
    }

    /// Regression: workflow may show plan review first, then `tddy-tools ask` clarification.
    /// Presenter must leave DocumentReview and enter Select when clarification arrives.
    #[test]
    fn clarification_needed_after_document_review_enters_select_mode() {
        let mut p = make_presenter();
        p.state.mode = AppMode::DocumentReview {
            content: "# Plan".to_string(),
        };
        inject_workflow_event(
            &mut p,
            WorkflowEvent::ClarificationNeeded {
                questions: vec![ClarificationQuestion {
                    header: "Scope".to_string(),
                    question: "Follow-up?".to_string(),
                    options: vec![QuestionOption {
                        label: "Yes".to_string(),
                        description: String::new(),
                    }],
                    multi_select: false,
                    allow_other: false,
                }],
            },
        );
        p.poll_workflow();
        assert!(
            matches!(p.state().mode, AppMode::Select { .. }),
            "expected Select after ClarificationNeeded; got {:?}",
            p.state().mode
        );
    }

    #[test]
    fn backend_selection_answer_transitions_to_feature_input() {
        let mut p = make_presenter();
        let q = crate::backend::backend_selection_question();
        p.show_backend_selection(q, 0);
        p.handle_intent(UserIntent::AnswerSelect(2));
        assert!(matches!(p.state().mode, AppMode::FeatureInput));
        assert!(!p.is_backend_selection_pending());
        assert_eq!(p.state().agent, "cursor");
        assert_eq!(p.state().model, "composer-2");
    }

    #[test]
    fn backend_selection_answer_claude_acp() {
        let mut p = make_presenter();
        let q = crate::backend::backend_selection_question();
        p.show_backend_selection(q, 0);
        p.handle_intent(UserIntent::AnswerSelect(1));
        assert_eq!(p.state().agent, "claude-acp");
        assert_eq!(p.state().model, "opus");
    }

    #[test]
    fn quit_broadcasts_intent_received_for_tui_should_quit_sync() {
        let (event_tx, _) = tokio::sync::broadcast::channel(16);
        let (intent_tx, _) = mpsc::channel();
        let mut p = make_presenter()
            .with_broadcast(event_tx.clone())
            .with_intent_sender(intent_tx);
        let mut conn = p.connect_view().expect("connect_view should return Some");
        p.state.mode = AppMode::ErrorRecovery {
            error_message: "workflow failed".to_string(),
        };
        p.handle_intent(UserIntent::Quit);
        assert!(
            p.state().should_quit,
            "presenter must set should_quit on Quit"
        );
        let ev = conn
            .event_rx
            .try_recv()
            .expect("subscriber must receive IntentReceived(Quit) for apply_event");
        assert!(
            matches!(ev, PresenterEvent::IntentReceived(UserIntent::Quit)),
            "TUI apply_event relies on this event to set local should_quit, got {:?}",
            ev
        );
    }

    #[test]
    fn view_session_document_markdown_viewer_shows_disk_not_stale_snapshot() {
        let tmp = std::env::temp_dir().join("tddy-test-view-doc-disk");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("artifacts")).unwrap();
        let on_disk = "# Plan\n\nDOC_FROM_DISK_UNIQUE_42\n";
        std::fs::write(tmp.join("artifacts").join("SessionDoc.md"), on_disk).unwrap();

        let mut p = make_presenter();
        p.workflow_session_dir = Some(tmp.clone());
        p.state.mode = AppMode::DocumentReview {
            content: "STALE_SNAPSHOT_NOT_ON_DISK".to_string(),
        };

        p.handle_intent(UserIntent::ViewSessionDocument);

        match &p.state().mode {
            AppMode::MarkdownViewer { content } => {
                assert!(
                    content.contains("DOC_FROM_DISK_UNIQUE_42"),
                    "View session document must show primary artifact from workflow_session_dir; got: {:?}",
                    content
                );
                assert!(
                    !content.contains("STALE_SNAPSHOT_NOT_ON_DISK"),
                    "must not show stale in-memory snapshot when disk differs; got: {:?}",
                    content
                );
            }
            other => panic!("expected MarkdownViewer, got {:?}", other),
        }

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn view_session_document_markdown_viewer_shows_uuid_root_when_workflow_dir_nested() {
        let root =
            std::env::temp_dir().join(format!("tddy-test-view-doc-nested-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let uuid = root
            .join("sessions")
            .join("a97addd3-c31b-442b-a6b0-a63abe99e11d");
        let nested = uuid.join("2026-03-24-feature-slug");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(
            uuid.join("SessionDoc.md"),
            "# Full\n\nCANONICAL_UUID_BODY\n",
        )
        .unwrap();
        std::fs::write(
            nested.join("SessionDoc.md"),
            "## Related\nlegacy nested only\n",
        )
        .unwrap();

        let mut p = make_presenter();
        p.workflow_session_dir = Some(nested.clone());
        p.state.mode = AppMode::DocumentReview {
            content: "STALE".to_string(),
        };

        p.handle_intent(UserIntent::ViewSessionDocument);

        match &p.state().mode {
            AppMode::MarkdownViewer { content } => {
                assert!(
                    content.contains("CANONICAL_UUID_BODY"),
                    "View session document must prefer sessions/<uuid>/ primary doc when nested; got: {:?}",
                    content
                );
                assert!(
                    !content.contains("legacy nested only"),
                    "must not show nested duplicate doc; got: {:?}",
                    content
                );
            }
            other => panic!("expected MarkdownViewer, got {:?}", other),
        }

        let _ = std::fs::remove_dir_all(&root);
    }

    fn make_presenter_with_broadcast(
    ) -> (Presenter, tokio::sync::broadcast::Receiver<PresenterEvent>) {
        let (tx, rx) = tokio::sync::broadcast::channel(256);
        let p = make_presenter().with_broadcast(tx);
        (p, rx)
    }

    /// Counts how many presenter events would cause a remote/UI consumer to show the same
    /// completed agent line (PRD: at most one authoritative channel per logical line).
    fn agent_line_authoritative_channel_count(
        events: &[PresenterEvent],
        line_without_newline: &str,
    ) -> usize {
        let mut n = 0;
        for ev in events {
            match ev {
                PresenterEvent::ActivityLogged(e)
                    if e.kind == ActivityKind::AgentOutput && e.text == line_without_newline =>
                {
                    n += 1;
                }
                PresenterEvent::AgentOutput(s) => {
                    let t = s.strip_suffix('\n').unwrap_or(s.as_str());
                    if t == line_without_newline {
                        n += 1;
                    }
                }
                _ => {}
            }
        }
        n
    }

    /// PRD: incremental agent text — partial chunks without `\n` must become visible in the activity
    /// log (or equivalent incremental state), not only after a newline flush.
    #[test]
    fn agent_output_chunk_visible_before_newline() {
        let mut p = make_presenter();
        inject_workflow_event(
            &mut p,
            WorkflowEvent::AgentOutput("partial_without_newline".to_string()),
        );
        p.poll_workflow();
        let last_agent = p
            .state()
            .activity_log
            .iter()
            .rev()
            .find(|e| e.kind == ActivityKind::AgentOutput);
        assert_eq!(
            last_agent.map(|e| e.text.as_str()),
            Some("partial_without_newline"),
            "expected partial chunk to appear in activity log before first newline (PRD incremental visibility)"
        );
    }

    /// PRD: do not emit both `ActivityLogged(AgentOutput)` and `AgentOutput` for the same logical
    /// line in a way that duplicates full-line content for activity + remote consumers.
    #[test]
    fn agent_output_not_duplicated_across_activity_and_agent_output_events() {
        let (mut p, mut rx) = make_presenter_with_broadcast();
        inject_workflow_event(
            &mut p,
            WorkflowEvent::AgentOutput("single_line\n".to_string()),
        );
        p.poll_workflow();
        let mut events = Vec::new();
        while let Ok(ev) = rx.try_recv() {
            events.push(ev);
        }
        let channels = agent_line_authoritative_channel_count(&events, "single_line");
        assert_eq!(
            channels, 1,
            "expected a single authoritative representation for agent line text in presenter events; got {} (events: {:?})",
            channels, events
        );
    }
}
