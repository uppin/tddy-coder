//! Presenter — orchestrates workflow and owns application state.

use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

use crate::{ClarificationQuestion, SharedBackend};

use crate::presenter::intent::UserIntent;
use crate::presenter::presenter_events::PresenterEvent;
use crate::presenter::state::{ActivityEntry, ActivityKind, AppMode, PresenterState};
use crate::presenter::view::PresenterView;
use crate::presenter::workflow_runner;
use crate::presenter::WorkflowEvent;

/// Instruction prefix for dequeued inbox prompts.
const QUEUED_INSTRUCTION_PREFIX: &str =
    "[QUEUED] The following prompt was queued while you were busy. Please address it:\n\n";

/// Presenter: owns state, receives UserIntents, orchestrates workflow thread.
pub struct Presenter<V: PresenterView> {
    state: PresenterState,
    view: V,
    workflow_event_rx: Option<mpsc::Receiver<WorkflowEvent>>,
    answer_tx: Option<mpsc::Sender<String>>,
    workflow_backend: Option<SharedBackend>,
    workflow_output_dir: Option<PathBuf>,
    /// Stored when WorkflowComplete is received; used to print result on TUI exit.
    workflow_result: Option<Result<String, String>>,
    pending_questions: Vec<ClarificationQuestion>,
    current_question_index: usize,
    collected_answers: Vec<String>,
    agent_output_buffer: String,
    workflow_handle: Option<thread::JoinHandle<()>>,
    /// When set, events are broadcast for gRPC subscribers.
    broadcast_tx: Option<tokio::sync::broadcast::Sender<PresenterEvent>>,
    /// When true, next AnswerText is refinement feedback (not clarification).
    plan_refinement_pending: bool,
}

impl<V: PresenterView> Presenter<V> {
    /// Create a new Presenter in FeatureInput mode.
    pub fn new(view: V, agent: impl Into<String>, model: impl Into<String>) -> Self {
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
            view,
            workflow_event_rx: None,
            answer_tx: None,
            workflow_backend: None,
            workflow_output_dir: None,
            workflow_result: None,
            pending_questions: Vec::new(),
            current_question_index: 0,
            collected_answers: Vec::new(),
            agent_output_buffer: String::new(),
            workflow_handle: None,
            broadcast_tx: None,
            plan_refinement_pending: false,
        }
    }

    /// Enable broadcast of PresenterEvents (for gRPC subscribers).
    pub fn with_broadcast(mut self, tx: tokio::sync::broadcast::Sender<PresenterEvent>) -> Self {
        self.broadcast_tx = Some(tx);
        self
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
                if let Some(ref tx) = self.answer_tx {
                    let _ = tx.send("Approve".to_string());
                }
                self.state.mode = AppMode::Running;
                self.view.on_mode_changed(&self.state.mode);
                self.broadcast(PresenterEvent::ModeChanged(self.state.mode.clone()));
            }
            UserIntent::ViewPlan => {
                if let AppMode::PlanReview { ref prd_content } = self.state.mode {
                    self.state.mode = AppMode::MarkdownViewer {
                        content: prd_content.clone(),
                    };
                    self.view.on_mode_changed(&self.state.mode);
                    self.broadcast(PresenterEvent::ModeChanged(self.state.mode.clone()));
                }
            }
            UserIntent::RefinePlan => {
                self.plan_refinement_pending = true;
                self.state.mode = AppMode::TextInput {
                    prompt: "Enter refinement feedback:".to_string(),
                };
                self.view.on_mode_changed(&self.state.mode);
                self.broadcast(PresenterEvent::ModeChanged(self.state.mode.clone()));
            }
            UserIntent::DismissViewer => {
                if let AppMode::MarkdownViewer { ref content } = self.state.mode {
                    self.state.mode = AppMode::PlanReview {
                        prd_content: content.clone(),
                    };
                    self.view.on_mode_changed(&self.state.mode);
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
                            if let Some(ref tx) = self.answer_tx {
                                let _ = tx.send(self.collect_answers());
                            }
                        }
                    }
                }
            }
            UserIntent::AnswerOther(text) => {
                self.collected_answers.push(text);
                self.current_question_index += 1;
                self.advance_to_next_question();
                if self.clarification_answers_ready() {
                    if let Some(ref tx) = self.answer_tx {
                        let _ = tx.send(self.collect_answers());
                    }
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
                        if let Some(ref tx) = self.answer_tx {
                            let _ = tx.send(self.collect_answers());
                        }
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
                    self.view.on_mode_changed(&self.state.mode);
                    self.broadcast(PresenterEvent::ModeChanged(self.state.mode.clone()));
                } else {
                    self.collected_answers.push(text);
                    self.current_question_index += 1;
                    self.advance_to_next_question();
                    if self.clarification_answers_ready() {
                        if let Some(ref tx) = self.answer_tx {
                            let _ = tx.send(self.collect_answers());
                        }
                    }
                }
            }
            UserIntent::QueuePrompt(text) => {
                if !text.is_empty() {
                    self.state.inbox.push(text);
                    self.view.on_inbox_changed(&self.state.inbox);
                    self.broadcast(PresenterEvent::InboxChanged(self.state.inbox.clone()));
                }
            }
            UserIntent::EditInboxItem { index, text } => {
                if index < self.state.inbox.len() {
                    self.state.inbox[index] = text;
                    self.view.on_inbox_changed(&self.state.inbox);
                    self.broadcast(PresenterEvent::InboxChanged(self.state.inbox.clone()));
                }
            }
            UserIntent::DeleteInboxItem(index) => {
                if index < self.state.inbox.len() {
                    self.state.inbox.remove(index);
                    self.view.on_inbox_changed(&self.state.inbox);
                    self.broadcast(PresenterEvent::InboxChanged(self.state.inbox.clone()));
                }
            }
            UserIntent::Scroll(_) => {
                // View-local; no-op in Presenter
            }
            UserIntent::Quit => {
                self.state.should_quit = true;
            }
        }
    }

    fn clarification_answers_ready(&self) -> bool {
        !self.pending_questions.is_empty()
            && self.current_question_index >= self.pending_questions.len()
            && matches!(self.state.mode, AppMode::Running)
    }

    fn collect_answers(&self) -> String {
        self.collected_answers.join("\n")
    }

    fn advance_to_next_question(&mut self) {
        if self.current_question_index >= self.pending_questions.len() {
            self.state.mode = AppMode::Running;
            self.view.on_mode_changed(&self.state.mode);
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
            self.view.on_mode_changed(&self.state.mode);
            self.broadcast(PresenterEvent::ModeChanged(self.state.mode.clone()));
        }
    }

    fn flush_agent_output_buffer(&mut self) {
        if !self.agent_output_buffer.is_empty() {
            let line = std::mem::take(&mut self.agent_output_buffer);
            let entry = ActivityEntry {
                text: line,
                kind: ActivityKind::AgentOutput,
            };
            self.state.activity_log.push(entry.clone());
            self.view.on_activity_logged(&entry);
            self.broadcast(PresenterEvent::ActivityLogged(entry));
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
                    };
                    self.state.activity_log.push(entry.clone());
                    self.view.on_activity_logged(&entry);
                    self.broadcast(PresenterEvent::ActivityLogged(entry));
                }
                WorkflowEvent::StateChange { from, to } => {
                    self.state.current_state = Some(to.clone());
                    let entry = ActivityEntry {
                        text: format!("State: {} → {}", from, to),
                        kind: ActivityKind::StateChange,
                    };
                    self.state.activity_log.push(entry.clone());
                    self.view.on_activity_logged(&entry);
                    self.view.on_state_changed(&from, &to);
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
                        self.view.on_mode_changed(&self.state.mode);
                        self.broadcast(PresenterEvent::ModeChanged(self.state.mode.clone()));
                    }
                    self.view.on_goal_started(&goal);
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
                    self.view.on_mode_changed(&self.state.mode);
                    self.broadcast(PresenterEvent::ModeChanged(self.state.mode.clone()));
                }
                WorkflowEvent::WorkflowComplete(result) => {
                    self.flush_agent_output_buffer();
                    self.workflow_result = Some(result.clone());
                    self.view.on_workflow_complete(&result);
                    self.broadcast(PresenterEvent::WorkflowComplete(result.clone()));
                    if result.is_ok() && !self.state.inbox.is_empty() {
                        let item = self.state.inbox.remove(0);
                        let prefixed = format!("{}{}", QUEUED_INSTRUCTION_PREFIX, item);
                        self.view.on_inbox_changed(&self.state.inbox);
                        self.broadcast(PresenterEvent::InboxChanged(self.state.inbox.clone()));
                        self.state.mode = AppMode::Running;
                        self.view.on_mode_changed(&self.state.mode);
                        self.broadcast(PresenterEvent::ModeChanged(self.state.mode.clone()));
                        // Workflow thread has exited; restart with dequeued prompt
                        if let (Some(backend), Some(output_dir)) = (
                            self.workflow_backend.clone(),
                            self.workflow_output_dir.clone(),
                        ) {
                            if let Some(h) = self.workflow_handle.take() {
                                let _ = h.join();
                            }
                            self.spawn_workflow(backend, output_dir, Some(prefixed));
                        }
                    } else {
                        self.state.mode = AppMode::Done;
                        self.view.on_mode_changed(&self.state.mode);
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
                                self.view.on_activity_logged(&entry);
                                self.broadcast(PresenterEvent::ActivityLogged(entry));
                            }
                        } else {
                            self.agent_output_buffer.push_str(part);
                        }
                    }
                    self.view.on_agent_output(&text);
                    self.broadcast(PresenterEvent::AgentOutput(text.clone()));
                }
            }
        }
    }

    /// Start the workflow with the given backend.
    pub fn start_workflow(
        &mut self,
        backend: SharedBackend,
        output_dir: PathBuf,
        initial_prompt: Option<String>,
    ) {
        self.workflow_backend = Some(backend.clone());
        self.workflow_output_dir = Some(output_dir.clone());
        self.spawn_workflow(backend, output_dir, initial_prompt);
    }

    fn spawn_workflow(
        &mut self,
        backend: SharedBackend,
        output_dir: PathBuf,
        initial_prompt: Option<String>,
    ) {
        let (event_tx, event_rx) = mpsc::channel();
        let (answer_tx, answer_rx) = mpsc::channel();

        let handle = thread::spawn(move || {
            workflow_runner::run_workflow(
                backend,
                event_tx,
                answer_rx,
                output_dir,
                None,
                None,
                initial_prompt,
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

    /// Reference to the view.
    pub fn view(&self) -> &V {
        &self.view
    }

    /// Mutable reference to the view (for tests to extract events).
    pub fn view_mut(&mut self) -> &mut V {
        &mut self.view
    }

    /// True when workflow is complete and TUI can exit.
    pub fn is_done(&self) -> bool {
        matches!(self.state.mode, AppMode::Done)
    }

    /// Take the workflow result (if any) for printing on TUI exit.
    pub fn take_workflow_result(&mut self) -> Option<Result<String, String>> {
        self.workflow_result.take()
    }
}
