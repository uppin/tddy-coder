//! Presenter — orchestrates workflow and owns application state.

use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

use crate::backend::QuestionOption;
use crate::toolcall::{store_submit_result, ToolCallRequest, ToolCallResponse};
use crate::{ClarificationQuestion, SharedBackend};

use crate::presenter::intent::UserIntent;
use crate::presenter::presenter_events::{PresenterEvent, ViewConnection};
use crate::presenter::state::{ActivityEntry, ActivityKind, AppMode, PresenterState};
use crate::presenter::workflow_runner;
use crate::presenter::{WorkflowCompletePayload, WorkflowEvent};

/// Pending tool call response: Ask sends answers string, Approve sends allow/deny.
enum PendingToolCallResponse {
    Ask(tokio::sync::oneshot::Sender<ToolCallResponse>),
    Approve(tokio::sync::oneshot::Sender<ToolCallResponse>),
}

/// Instruction prefix for dequeued inbox prompts.
const QUEUED_INSTRUCTION_PREFIX: &str =
    "[QUEUED] The following prompt was queued while you were busy. Please address it:\n\n";

/// Presenter: owns state, receives UserIntents, orchestrates workflow thread.
/// Views observe state via connect_view() → ViewConnection (broadcast events).
pub struct Presenter {
    state: PresenterState,
    workflow_event_rx: Option<mpsc::Receiver<WorkflowEvent>>,
    answer_tx: Option<mpsc::Sender<String>>,
    workflow_backend: Option<SharedBackend>,
    workflow_output_dir: Option<PathBuf>,
    workflow_conversation_output: Option<PathBuf>,
    workflow_debug_output: Option<PathBuf>,
    workflow_debug: bool,
    /// Stored when WorkflowComplete is received; used to print result on TUI exit.
    workflow_result: Option<Result<WorkflowCompletePayload, String>>,
    pending_questions: Vec<ClarificationQuestion>,
    current_question_index: usize,
    collected_answers: Vec<String>,
    agent_output_buffer: String,
    workflow_handle: Option<thread::JoinHandle<()>>,
    /// When set, events are broadcast for gRPC subscribers.
    broadcast_tx: Option<tokio::sync::broadcast::Sender<PresenterEvent>>,
    /// When set, connect_view() returns this for external views to send intents.
    intent_tx: Option<mpsc::Sender<UserIntent>>,
    /// When true, next AnswerText is refinement feedback (not clarification).
    plan_refinement_pending: bool,
    /// Receiver for tddy-tools relay requests (Submit, Ask, Approve).
    tool_call_rx: Option<mpsc::Receiver<ToolCallRequest>>,
    /// When set, answers go to tool call response (Ask/Approve from tddy-tools) instead of answer_tx.
    pending_tool_call_response: Option<PendingToolCallResponse>,
    /// Stored socket path for workflow restart (dequeued prompts).
    workflow_socket_path: Option<PathBuf>,
}

impl Presenter {
    /// Create a new Presenter in FeatureInput mode.
    pub fn new(agent: impl Into<String>, model: impl Into<String>) -> Self {
        let state = PresenterState {
            agent: agent.into(),
            model: model.into(),
            mode: AppMode::FeatureInput,
            current_goal: None,
            current_state: None,
            goal_start_time: std::time::Instant::now(),
            activity_log: Vec::new(),
            inbox: Vec::new(),
            should_quit: false,
        };
        Presenter {
            state,
            workflow_event_rx: None,
            answer_tx: None,
            workflow_backend: None,
            workflow_output_dir: None,
            workflow_conversation_output: None,
            workflow_debug_output: None,
            workflow_debug: false,
            workflow_result: None,
            pending_questions: Vec::new(),
            current_question_index: 0,
            collected_answers: Vec::new(),
            agent_output_buffer: String::new(),
            workflow_handle: None,
            broadcast_tx: None,
            intent_tx: None,
            plan_refinement_pending: false,
            tool_call_rx: None,
            pending_tool_call_response: None,
            workflow_socket_path: None,
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

    /// Create a new view connection: state snapshot + event subscription + intent sender.
    /// Returns None if broadcast or intent_tx is not configured.
    pub fn connect_view(&self) -> Option<ViewConnection> {
        let broadcast_tx = self.broadcast_tx.as_ref()?;
        let intent_tx = self.intent_tx.clone()?;
        Some(ViewConnection {
            state_snapshot: self.state.clone(),
            event_rx: broadcast_tx.subscribe(),
            intent_tx,
        })
    }

    fn broadcast(&self, event: PresenterEvent) {
        if let Some(ref tx) = self.broadcast_tx {
            let _ = tx.send(event);
        }
    }

    /// Handle a user intent. Updates state and may send answers to workflow.
    pub fn handle_intent(&mut self, intent: UserIntent) {
        self.broadcast(PresenterEvent::IntentReceived(intent.clone()));
        match intent {
            UserIntent::SubmitFeatureInput(text) => {
                if !text.is_empty() {
                    if let Some(ref tx) = self.answer_tx {
                        let _ = tx.send(text);
                    }
                }
            }
            UserIntent::ApprovePlan => {
                if matches!(self.state.mode, AppMode::PlanReview { .. }) {
                    // Existing: approve from PlanReview menu
                    if let Some(ref tx) = self.answer_tx {
                        let _ = tx.send("Approve".to_string());
                    }
                    self.state.mode = AppMode::Running;
                    self.broadcast(PresenterEvent::ModeChanged(self.state.mode.clone()));
                } else if matches!(self.state.mode, AppMode::MarkdownViewer { .. }) {
                    if let Some(ref tx) = self.answer_tx {
                        let _ = tx.send("Approve".to_string());
                    }
                    self.state.mode = AppMode::Running;
                    self.broadcast(PresenterEvent::ModeChanged(self.state.mode.clone()));
                }
            }
            UserIntent::ViewPlan => {
                if let AppMode::PlanReview { ref prd_content } = self.state.mode {
                    self.state.mode = AppMode::MarkdownViewer {
                        content: prd_content.clone(),
                    };
                    self.broadcast(PresenterEvent::ModeChanged(self.state.mode.clone()));
                }
            }
            UserIntent::RefinePlan => {
                self.plan_refinement_pending = true;
                self.state.mode = AppMode::TextInput {
                    prompt: "Enter refinement feedback:".to_string(),
                };
                self.broadcast(PresenterEvent::ModeChanged(self.state.mode.clone()));
            }
            UserIntent::DismissViewer => {
                if let AppMode::MarkdownViewer { ref content } = self.state.mode {
                    self.state.mode = AppMode::PlanReview {
                        prd_content: content.clone(),
                    };
                    self.broadcast(PresenterEvent::ModeChanged(self.state.mode.clone()));
                }
            }
            UserIntent::AnswerSelect(idx) => {
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
                if self.plan_refinement_pending {
                    self.plan_refinement_pending = false;
                    if let Some(ref tx) = self.answer_tx {
                        let _ = tx.send(text);
                    }
                    self.state.mode = AppMode::Running;
                    self.broadcast(PresenterEvent::ModeChanged(self.state.mode.clone()));
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
                if !text.is_empty() {
                    self.state.inbox.push(text);
                    self.broadcast(PresenterEvent::InboxChanged(self.state.inbox.clone()));
                }
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
            UserIntent::Quit => {
                self.state.should_quit = true;
            }
            UserIntent::ResumeFromError => {
                log::info!(
                    "ResumeFromError: looking up last session for goal {:?}",
                    self.state.current_goal
                );
                let session_id = if let (Some(output_dir), Some(goal)) = (
                    self.workflow_output_dir.as_ref(),
                    self.state.current_goal.as_deref(),
                ) {
                    match crate::changeset::read_changeset(output_dir) {
                        Ok(cs) => {
                            let sid = crate::changeset::get_session_for_tag(&cs, goal);
                            log::info!("ResumeFromError: session_id={:?} for tag={}", sid, goal);
                            sid
                        }
                        Err(e) => {
                            log::warn!("ResumeFromError: could not read changeset: {}", e);
                            None
                        }
                    }
                } else {
                    log::warn!("ResumeFromError: no output_dir or goal available, spawning fresh");
                    None
                };
                self.state.mode = AppMode::Running;
                self.broadcast(PresenterEvent::ModeChanged(self.state.mode.clone()));
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
                        None,
                        None,
                        self.workflow_conversation_output.clone(),
                        self.workflow_debug_output.clone(),
                        self.workflow_debug,
                        session_id,
                        self.workflow_socket_path.clone(),
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
            self.broadcast(PresenterEvent::ModeChanged(self.state.mode.clone()));
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
                };
            }
            self.broadcast(PresenterEvent::ModeChanged(self.state.mode.clone()));
        }
    }

    fn flush_agent_output_buffer(&mut self) {
        if !self.agent_output_buffer.is_empty() {
            let line = std::mem::take(&mut self.agent_output_buffer);
            self.log_activity(line, ActivityKind::AgentOutput);
        }
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
                ToolCallRequest::Submit {
                    goal,
                    data,
                    response_tx,
                } => {
                    self.log_activity(
                        format!("⚙ tddy-tools submit (goal: {})", goal),
                        ActivityKind::ToolUse,
                    );
                    let json_str = serde_json::to_string(&data).unwrap_or_default();
                    store_submit_result(&goal, &json_str);
                    let _ = response_tx.send(ToolCallResponse::SubmitOk { goal: goal.clone() });
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
                    self.state.current_goal = Some(goal.clone());
                    self.state.goal_start_time = std::time::Instant::now();
                    if matches!(self.state.mode, AppMode::FeatureInput) {
                        self.state.mode = AppMode::Running;
                        self.broadcast(PresenterEvent::ModeChanged(self.state.mode.clone()));
                    }
                    self.broadcast(PresenterEvent::GoalStarted(goal.clone()));
                }
                WorkflowEvent::ClarificationNeeded { questions } => {
                    self.flush_agent_output_buffer();
                    self.pending_questions = questions;
                    self.current_question_index = 0;
                    self.collected_answers.clear();
                    self.advance_to_next_question();
                }
                WorkflowEvent::PlanApprovalNeeded { prd_content } => {
                    self.flush_agent_output_buffer();
                    self.state.mode = AppMode::PlanReview { prd_content };
                    self.broadcast(PresenterEvent::ModeChanged(self.state.mode.clone()));
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
                WorkflowEvent::WorkflowComplete(result) => {
                    self.flush_agent_output_buffer();
                    self.workflow_result = Some(result.clone());
                    self.broadcast(PresenterEvent::WorkflowComplete(result.clone()));
                    if result.is_ok() && !self.state.inbox.is_empty() {
                        let item = self.state.inbox.remove(0);
                        let prefixed = format!("{}{}", QUEUED_INSTRUCTION_PREFIX, item);
                        self.broadcast(PresenterEvent::InboxChanged(self.state.inbox.clone()));
                        self.state.mode = AppMode::Running;
                        self.broadcast(PresenterEvent::ModeChanged(self.state.mode.clone()));
                        // Workflow thread has exited; restart with dequeued prompt
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
                                None,
                                Some(prefixed),
                                self.workflow_conversation_output.clone(),
                                self.workflow_debug_output.clone(),
                                self.workflow_debug,
                                None,
                                self.workflow_socket_path.clone(),
                            );
                        }
                    } else {
                        match result {
                            Err(ref msg) => {
                                log::info!("WorkflowComplete Err → ErrorRecovery: {}", msg);
                                self.log_activity(
                                    format!("Workflow failed: {}", msg),
                                    ActivityKind::Info,
                                );
                                self.state.mode = AppMode::ErrorRecovery {
                                    error_message: msg.clone(),
                                };
                            }
                            Ok(_) => {
                                log::info!("WorkflowComplete Ok → Done");
                                self.state.mode = AppMode::Done;
                            }
                        }
                        self.broadcast(PresenterEvent::ModeChanged(self.state.mode.clone()));
                    }
                }
                WorkflowEvent::AgentOutput(text) => {
                    for part in text.split_inclusive('\n') {
                        if part.ends_with('\n') {
                            self.agent_output_buffer
                                .push_str(part.trim_end_matches('\n'));
                            let line = std::mem::take(&mut self.agent_output_buffer);
                            if !line.is_empty() {
                                let entry = ActivityEntry {
                                    text: line,
                                    kind: ActivityKind::AgentOutput,
                                };
                                self.state.activity_log.push(entry.clone());
                                self.broadcast(PresenterEvent::ActivityLogged(entry));
                            }
                        } else {
                            self.agent_output_buffer.push_str(part);
                        }
                    }
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
        plan_dir: Option<PathBuf>,
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
        self.workflow_conversation_output = conversation_output_path.clone();
        self.workflow_debug_output = debug_output_path.clone();
        self.workflow_debug = debug;
        self.tool_call_rx = tool_call_rx;
        self.workflow_socket_path = socket_path.clone();
        self.spawn_workflow(
            backend,
            output_dir,
            plan_dir,
            initial_prompt,
            conversation_output_path,
            debug_output_path,
            debug,
            session_id,
            socket_path,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn spawn_workflow(
        &mut self,
        backend: SharedBackend,
        output_dir: PathBuf,
        plan_dir: Option<PathBuf>,
        initial_prompt: Option<String>,
        conversation_output_path: Option<PathBuf>,
        debug_output_path: Option<PathBuf>,
        debug: bool,
        session_id: Option<String>,
        socket_path: Option<PathBuf>,
    ) {
        let (event_tx, event_rx) = mpsc::channel();
        let (answer_tx, answer_rx) = mpsc::channel();

        let handle = thread::spawn(move || {
            workflow_runner::run_workflow(
                backend,
                event_tx,
                answer_rx,
                output_dir,
                plan_dir,
                session_id,
                None,
                initial_prompt,
                conversation_output_path,
                debug_output_path,
                debug,
                socket_path,
            );
        });

        self.workflow_event_rx = Some(event_rx);
        self.answer_tx = Some(answer_tx);
        self.workflow_handle = Some(handle);
    }

    /// Reference to current state.
    pub fn state(&self) -> &PresenterState {
        &self.state
    }

    /// True when workflow is complete and TUI can exit.
    pub fn is_done(&self) -> bool {
        matches!(self.state.mode, AppMode::Done)
    }

    /// Take the workflow result (if any) for printing on TUI exit.
    pub fn take_workflow_result(&mut self) -> Option<Result<WorkflowCompletePayload, String>> {
        self.workflow_result.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presenter::state::AppMode;

    fn make_presenter() -> Presenter {
        Presenter::new("agent", "model")
    }

    fn inject_workflow_event(presenter: &mut Presenter, event: WorkflowEvent) {
        let (tx, rx) = mpsc::channel();
        tx.send(event).unwrap();
        presenter.workflow_event_rx = Some(rx);
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
    fn test_workflow_success_transitions_to_done() {
        let mut p = make_presenter();
        inject_workflow_event(
            &mut p,
            WorkflowEvent::WorkflowComplete(Ok(WorkflowCompletePayload {
                summary: "all done".to_string(),
                plan_dir: None,
            })),
        );
        p.poll_workflow();
        assert!(
            matches!(p.state().mode, AppMode::Done),
            "Expected Done mode, got {:?}",
            p.state().mode
        );
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
}
