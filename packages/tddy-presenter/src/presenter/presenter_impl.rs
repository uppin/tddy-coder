//! Presenter — orchestrates workflow and owns application state.

use std::path::PathBuf;
use std::sync::mpsc;

use crate::toolcall::ToolCallRequest;
use crate::SharedBackend;

use crate::presenter::presenter_events::{ModeChangedDetails, PresenterEvent};
use crate::presenter::state::{ActivityEntry, ActivityKind, AppMode, PresenterState};
use crate::presenter::state_groups::{
    ActivityRecorder, BackendSelection, PendingQuestions, ViewChannels, WorkflowRun,
};
use crate::presenter::{WorkflowCompletePayload, WorkflowEvent};

/// Instruction prefix for dequeued inbox prompts.
const QUEUED_INSTRUCTION_PREFIX: &str =
    "[QUEUED] The following prompt was queued while you were busy. Please address it:\n\n";

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
    /// The running workflow: its channels, its artifact directories, its result.
    workflow: WorkflowRun,
    /// The clarification questions awaiting an operator, and the answers collected so far.
    questions: PendingQuestions,
    /// What this session records about the agent's own tool calls, and where.
    activity: ActivityRecorder,
    /// The surfaces outside the presenter that read its events or send it intents.
    views: ViewChannels,
    /// Which backend and recipe this session runs, including the deferred-start path.
    backend: BackendSelection,
    /// Tddy data directory root — passed to workflow runner to avoid global state.
    tddy_data_dir: PathBuf,
}

/// Milliseconds since the Unix epoch, for agent-activity timestamps.
fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn format_session_id_for_log(id: &str) -> String {
    const MAX: usize = 12;
    if id.len() <= MAX {
        id.to_string()
    } else {
        format!("{}…", &id[..MAX])
    }
}

mod activity;
mod backend_selection;
mod questions;
mod view_channels;
mod wiring;
mod workflow_run;

/// Private helpers called from more than one partition. They stay in the parent because a private
/// method declared here is visible to each child module, while one declared in a child is visible
/// only to that child.
impl Presenter {
    fn broadcast(&self, event: PresenterEvent) {
        if let Some(ref tx) = self.views.broadcast_tx {
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
            awaiting_open_answer: self.questions.awaiting_open_answer,
        }));
    }

    fn log_activity(&mut self, text: String, kind: ActivityKind) {
        let entry = ActivityEntry { text, kind };
        self.state.activity_log.push(entry.clone());
        self.broadcast(PresenterEvent::ActivityLogged(entry));
    }

    fn flush_agent_output_buffer(&mut self) {
        if !self.activity.output_buffer.is_empty() {
            let line = std::mem::take(&mut self.activity.output_buffer);
            log::debug!(
                "flush_agent_output_buffer: len={}, partial_row_active={}",
                line.len(),
                self.activity.output_partial_row_active
            );
            // Avoid a duplicate `activity_log` row when the partial row already shows this text.
            if self.activity.output_partial_row_active {
                if let Some(last) = self.state.activity_log.last() {
                    if last.kind == ActivityKind::AgentOutput && last.text == line {
                        self.activity.output_partial_row_active = false;
                        self.broadcast(PresenterEvent::ActivityLogged(ActivityEntry {
                            text: line,
                            kind: ActivityKind::AgentOutput,
                        }));
                        return;
                    }
                }
                self.activity.output_partial_row_active = false;
            }
            self.log_activity(line, ActivityKind::AgentOutput);
        }
    }

    fn advance_to_next_question(&mut self) {
        if self.questions.current_index >= self.questions.questions.len() {
            self.state.mode = AppMode::Running;
            self.broadcast_mode_changed();
        } else {
            let q = self.questions.questions[self.questions.current_index].clone();
            let total = self.questions.questions.len();
            if q.multi_select {
                self.state.mode = AppMode::MultiSelect {
                    question: q,
                    question_index: self.questions.current_index,
                    total_questions: total,
                };
            } else {
                self.state.mode = AppMode::Select {
                    question: q,
                    question_index: self.questions.current_index,
                    total_questions: total,
                    initial_selected: 0,
                };
            }
            self.broadcast_mode_changed();
        }
    }
}

/// The workflow-event hub. It dispatches each event to the partition that owns the state the event
/// changes, so it lives above all of them.
impl Presenter {
    /// Poll for workflow events. Call from main loop.
    /// Drains all pending events per call to minimize latency between tasks.
    pub fn poll_workflow(&mut self) {
        let rx = match self.workflow.event_rx.as_ref() {
            Some(r) => r,
            None => return,
        };

        let mut events = Vec::new();
        while let Ok(ev) = rx.try_recv() {
            events.push(ev);
        }

        for ev in events {
            match ev {
                WorkflowEvent::Progress(pev) => self.on_progress(pev),
                WorkflowEvent::StateChange { from, to } => self.on_state_change(from, to),
                WorkflowEvent::GoalStarted(goal) => self.on_goal_started(goal),
                WorkflowEvent::ClarificationNeeded { questions } => {
                    self.on_clarification_needed(questions)
                }
                WorkflowEvent::AwaitingFeatureInput => {
                    self.state.mode = AppMode::FeatureInput;
                    self.broadcast_mode_changed();
                }
                WorkflowEvent::SessionDocumentApprovalNeeded { content } => {
                    self.on_session_document_approval_needed(content)
                }
                WorkflowEvent::WorktreeSwitched { path } => self.on_worktree_switched(path),
                WorkflowEvent::WorkflowComplete(result) => self.on_workflow_complete(result),
                WorkflowEvent::AgentOutput(text) => self.on_agent_output(text),
            }
        }
    }
}

/// The three accessors on the state the presenter owns directly.
impl Presenter {
    /// Reference to current state.
    pub fn state(&self) -> &PresenterState {
        &self.state
    }

    /// True when workflow is complete (workflow_result is set).
    pub fn is_done(&self) -> bool {
        self.workflow.result.is_some()
    }

    /// Take the workflow result (if any) for printing on TUI exit.
    pub fn take_workflow_result(&mut self) -> Option<Result<WorkflowCompletePayload, String>> {
        self.workflow.result.take()
    }
}

/// What every activity record this presenter writes is stamped with — the commit its checkout is on
/// and the paths its call declared (`docs/ft/daemon/session-worktree-sync.md` AC1, AC2).
#[cfg(test)]
mod agent_activity_stamping;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presenter::state::AppMode;
    use std::sync::Arc;

    use crate::presenter::intent::UserIntent;
    use crate::presenter::WorkflowEvent;
    use crate::{ClarificationQuestion, QuestionOption, WorkflowRecipe};

    fn make_presenter() -> Presenter {
        let tmp = tempfile::tempdir().expect("make_presenter: create temp dir");
        Presenter::new(
            "agent",
            "model",
            std::sync::Arc::new(crate::presenter::presenter_test_recipe::EmptyPresenterTestRecipe)
                as std::sync::Arc<dyn WorkflowRecipe>,
            tmp.path().to_path_buf(),
        )
    }

    fn inject_workflow_event(presenter: &mut Presenter, event: WorkflowEvent) {
        let (tx, rx) = mpsc::channel();
        tx.send(event).unwrap();
        presenter.workflow.event_rx = Some(rx);
    }

    fn inject_workflow_events(presenter: &mut Presenter, events: Vec<WorkflowEvent>) {
        let (tx, rx) = mpsc::channel();
        for e in events {
            tx.send(e).unwrap();
        }
        drop(tx);
        presenter.workflow.event_rx = Some(rx);
    }

    #[test]
    fn progress_session_started_sets_workflow_session_id() {
        // Given
        let mut p = make_presenter();
        let sid = "550e8400-e29b-41d4-a716-446655440000";
        inject_workflow_event(
            &mut p,
            WorkflowEvent::Progress(crate::ProgressEvent::SessionStarted {
                session_id: sid.to_string(),
            }),
        );

        // When
        p.poll_workflow();

        // Then
        assert_eq!(p.state().workflow_session_id.as_deref(), Some(sid));
    }

    #[tokio::test]
    async fn tool_use_then_tool_result_persists_and_broadcasts_agent_activity() {
        use crate::agent_activity::{read_agent_activity, STATUS_COMPLETED, STATUS_RUNNING};

        // Given — a presenter with a broadcast channel and an agent-activity session dir
        let tmp = tempfile::tempdir().unwrap();
        let session_dir = tmp.path().join("sessions").join("s1");
        let (tx, mut rx) = tokio::sync::broadcast::channel(16);
        let mut p = Presenter::new(
            "cursor",
            "model",
            std::sync::Arc::new(crate::presenter::presenter_test_recipe::EmptyPresenterTestRecipe)
                as std::sync::Arc<dyn WorkflowRecipe>,
            tmp.path().to_path_buf(),
        )
        .with_broadcast(tx);
        p.set_agent_activity_context(session_dir.clone(), None, "coder");
        inject_workflow_events(
            &mut p,
            vec![
                WorkflowEvent::Progress(crate::ProgressEvent::ToolUse {
                    name: "Bash".to_string(),
                    detail: None,
                    input_json: Some(r#"{"command":"cargo build"}"#.to_string()),
                    call_id: Some("call-1".to_string()),
                }),
                WorkflowEvent::Progress(crate::ProgressEvent::ToolResult {
                    call_id: "call-1".to_string(),
                    result_json: r#"{"stdout":"ok"}"#.to_string(),
                    is_error: false,
                }),
            ],
        );

        // When
        p.poll_workflow();

        // Then — the running and completed rows coalesce into one completed call on disk
        let records = read_agent_activity(&session_dir).unwrap();
        assert_eq!(
            records.len(),
            1,
            "running + completed rows must coalesce into one call"
        );
        assert_eq!(records[0].call_id, "call-1");
        assert_eq!(records[0].tool_name, "Bash");
        assert_eq!(
            records[0].input,
            serde_json::json!({ "command": "cargo build" })
        );
        assert_eq!(records[0].status, STATUS_COMPLETED);
        assert_eq!(records[0].result, serde_json::json!({ "stdout": "ok" }));
        assert_eq!(records[0].source, "coder");

        // And — two AgentActivity events were broadcast: the running row then the completed row
        let mut activity: Vec<crate::agent_activity::AgentActivityRecord> = Vec::new();
        while activity.len() < 2 {
            let ev = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
                .await
                .expect("expected two AgentActivity broadcasts")
                .expect("broadcast channel closed early");
            if let PresenterEvent::AgentActivity(rec) = ev {
                activity.push(rec);
            }
        }
        assert_eq!(
            activity[0].status, STATUS_RUNNING,
            "first broadcast is running"
        );
        assert_eq!(
            activity[1].status, STATUS_COMPLETED,
            "second broadcast is completed"
        );
        assert_eq!(activity[1].call_id, "call-1");
    }

    #[test]
    fn workflow_complete_ok_clears_workflow_session_id_after_session_started() {
        // Given
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

        // When
        p.poll_workflow();

        // Then
        assert!(
            p.state().workflow_session_id.is_none(),
            "expected workflow_session_id cleared after successful completion"
        );
    }

    #[test]
    fn workflow_complete_err_clears_workflow_session_id_after_session_started() {
        // Given
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

        // When
        p.poll_workflow();

        // Then
        assert!(
            p.state().workflow_session_id.is_none(),
            "expected workflow_session_id cleared after error completion"
        );
    }

    #[test]
    fn workflow_error_transitions_to_error_recovery() {
        // Given
        let mut p = make_presenter();
        inject_workflow_event(
            &mut p,
            WorkflowEvent::WorkflowComplete(Err("backend timeout".to_string())),
        );

        // When
        p.poll_workflow();

        // Then
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
    fn workflow_success_transitions_to_feature_input() {
        // Given
        let mut p = make_presenter();
        inject_workflow_event(
            &mut p,
            WorkflowEvent::WorkflowComplete(Ok(WorkflowCompletePayload {
                summary: "all done".to_string(),
                session_dir: None,
            })),
        );

        // When
        p.poll_workflow();

        // Then
        assert!(
            matches!(p.state().mode, AppMode::FeatureInput),
            "Expected FeatureInput mode (ready for new workflow), got {:?}",
            p.state().mode
        );
    }

    #[test]
    fn awaiting_feature_input_event_switches_to_feature_input_mode() {
        // Given
        let mut p = make_presenter();
        inject_workflow_event(&mut p, WorkflowEvent::AwaitingFeatureInput);

        // When
        p.poll_workflow();

        // Then
        assert!(
            matches!(p.state().mode, AppMode::FeatureInput),
            "Expected FeatureInput when plan awaits description, got {:?}",
            p.state().mode
        );
    }

    #[test]
    fn continue_with_agent_sets_exit_action_and_quits_when_session_available() {
        // Given
        let mut p = make_presenter();
        let tmp = tempfile::tempdir().unwrap();
        let tmp = tmp.path();
        let mut cs = crate::changeset::Changeset::default();
        cs.state.session_id = Some("agent-session-42".to_string());
        crate::changeset::write_changeset(tmp, &cs).unwrap();
        p.workflow.output_dir = Some(tmp.to_path_buf());
        p.state.mode = AppMode::ErrorRecovery {
            error_message: "test error".to_string(),
        };

        // When
        p.handle_intent(UserIntent::ContinueWithAgent);

        // Then
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
    }

    #[test]
    fn continue_with_agent_stays_in_error_recovery_when_no_session() {
        // Given
        let mut p = make_presenter();
        let tmp = tempfile::tempdir().unwrap();
        let tmp = tmp.path();
        let cs = crate::changeset::Changeset::default(); // session_id is None
        crate::changeset::write_changeset(tmp, &cs).unwrap();
        p.workflow.output_dir = Some(tmp.to_path_buf());
        p.state.mode = AppMode::ErrorRecovery {
            error_message: "test error".to_string(),
        };

        // When
        p.handle_intent(UserIntent::ContinueWithAgent);

        // Then
        assert!(
            !p.state().should_quit,
            "ContinueWithAgent should NOT quit when no session_id is available"
        );
        assert!(
            p.state().exit_action.is_none(),
            "exit_action should remain None when no session_id"
        );
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
        // Given
        let mut p = make_presenter();
        let tmp = tempfile::tempdir().unwrap();
        let tmp = tmp.path();
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
        crate::changeset::write_changeset(tmp, &cs).unwrap();
        p.workflow.output_dir = Some(tmp.to_path_buf());
        p.state.current_goal = Some("evaluate".to_string());
        p.state.mode = AppMode::ErrorRecovery {
            error_message: "validate is not supported on the Cursor backend".to_string(),
        };

        // When
        p.handle_intent(UserIntent::ContinueWithAgent);

        // Then
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
    }

    /// When the failing goal has no matching tagged session (e.g. validate failed before a
    /// validate session was recorded), `ContinueWithAgent` must still resolve an agent session
    /// from the changeset so Enter does not appear to do nothing.
    #[test]
    fn continue_with_agent_resolves_session_when_current_goal_tag_has_no_entry() {
        // Given
        let mut p = make_presenter();
        let tmp = tempfile::tempdir().unwrap();
        let tmp = tmp.path();
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
        crate::changeset::write_changeset(tmp, &cs).unwrap();
        p.workflow.output_dir = Some(tmp.to_path_buf());
        p.state.current_goal = Some("validate".to_string());
        p.state.mode = AppMode::ErrorRecovery {
            error_message: "validate is not supported on the Cursor backend".to_string(),
        };

        // When
        p.handle_intent(UserIntent::ContinueWithAgent);

        // Then
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
    }

    /// `start_workflow` passes `output_dir` as `.` while `session_dir` points at the session folder
    /// (`~/.tddy/sessions/...`). Continue with agent must read `changeset.yaml` from `session_dir`.
    #[test]
    fn continue_with_agent_reads_changeset_from_workflow_session_dir() {
        // Given
        let mut p = make_presenter();
        let tmp_plan_guard = tempfile::tempdir().unwrap();
        let tmp_plan = tmp_plan_guard.path();
        let tmp_wrong_guard = tempfile::tempdir().unwrap();
        let tmp_wrong = tmp_wrong_guard.path();
        let resume_id = "resume-from-plan-dir";
        let mut cs = crate::changeset::Changeset::default();
        cs.state.session_id = Some(resume_id.to_string());
        crate::changeset::write_changeset(tmp_plan, &cs).unwrap();
        p.workflow.output_dir = Some(tmp_wrong.to_path_buf());
        p.workflow.session_dir = Some(tmp_plan.to_path_buf());
        p.state.mode = AppMode::ErrorRecovery {
            error_message: "read refactoring-plan.md: No such file or directory (os error 2)"
                .to_string(),
        };

        // When
        p.handle_intent(UserIntent::ContinueWithAgent);

        // Then
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
    }

    #[test]
    fn connect_view_returns_none_without_broadcast() {
        // Given
        let p = make_presenter();

        // Then
        assert!(p.connect_view().is_none());
    }

    #[test]
    fn connect_view_returns_none_without_intent_tx() {
        // Given
        let (tx, _) = tokio::sync::broadcast::channel(16);
        let p = make_presenter().with_broadcast(tx);

        // Then
        assert!(p.connect_view().is_none());
    }

    #[test]
    fn connect_view_returns_connection_with_matching_snapshot() {
        // Given
        let (event_tx, _) = tokio::sync::broadcast::channel(16);
        let (intent_tx, _) = mpsc::channel();
        let p = make_presenter()
            .with_broadcast(event_tx)
            .with_intent_sender(intent_tx);

        // When
        let conn = p.connect_view().expect("connect_view should return Some");

        // Then
        assert_eq!(conn.state_snapshot.agent, "agent");
        assert_eq!(conn.state_snapshot.model, "model");
        assert!(matches!(conn.state_snapshot.mode, AppMode::FeatureInput));
    }

    #[test]
    fn connect_view_event_rx_receives_broadcast_events() {
        // Given
        let (event_tx, _) = tokio::sync::broadcast::channel(16);
        let (intent_tx, _) = mpsc::channel();
        let p = make_presenter()
            .with_broadcast(event_tx.clone())
            .with_intent_sender(intent_tx);
        let mut conn = p.connect_view().expect("connect_view should return Some");

        // When
        let _ = event_tx.send(PresenterEvent::GoalStarted("plan".to_string()));
        let ev = conn.event_rx.try_recv();

        // Then
        assert!(
            matches!(ev, Ok(PresenterEvent::GoalStarted(ref g)) if g == "plan"),
            "Expected GoalStarted event, got {:?}",
            ev
        );
    }

    #[test]
    fn show_backend_selection_transitions_to_select_mode() {
        // Given
        let mut p = make_presenter();
        let q = crate::backend::backend_selection_question();

        // When
        p.show_backend_selection(q, 0);

        // Then
        assert!(matches!(p.state().mode, AppMode::Select { .. }));
        assert!(p.is_backend_selection_pending());
    }

    /// Regression: workflow may show plan review first, then `tddy-tools ask` clarification.
    /// Presenter must leave DocumentReview and enter Select when clarification arrives.
    #[test]
    fn clarification_needed_after_document_review_enters_select_mode() {
        // Given
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

        // When
        p.poll_workflow();

        // Then
        assert!(
            matches!(p.state().mode, AppMode::Select { .. }),
            "expected Select after ClarificationNeeded; got {:?}",
            p.state().mode
        );
    }

    /// Telegram alignment: Choose none only applies when `allow_other` indicates multi-select channel semantics permit empty submissions.
    #[test]
    fn presenter_empty_multi_select_semantics() {
        // Given
        let mut p = make_presenter();
        inject_workflow_event(
            &mut p,
            WorkflowEvent::ClarificationNeeded {
                questions: vec![ClarificationQuestion {
                    header: "Scope".into(),
                    question: "Pick stacks".into(),
                    options: vec![
                        QuestionOption {
                            label: "Tokio".into(),
                            description: String::new(),
                        },
                        QuestionOption {
                            label: "async-std".into(),
                            description: String::new(),
                        },
                    ],
                    multi_select: true,
                    allow_other: false,
                }],
            },
        );
        p.poll_workflow();
        assert!(
            matches!(p.state().mode, AppMode::MultiSelect { .. }),
            "fixture must enter MultiSelect mode"
        );

        // When
        p.handle_intent(UserIntent::AnswerMultiSelect(vec![], None));

        // Then
        assert!(
            matches!(p.state().mode, AppMode::MultiSelect { .. }),
            "when allow_other is false the presenter must reject empty multi-select (no silent Choose-none semantics)"
        );
    }

    #[test]
    fn backend_selection_answer_transitions_to_feature_input() {
        // Given
        let mut p = make_presenter();
        let q = crate::backend::backend_selection_question();
        p.show_backend_selection(q, 0);

        // When
        p.handle_intent(UserIntent::AnswerSelect(2));

        // Then
        assert!(matches!(p.state().mode, AppMode::FeatureInput));
        assert!(!p.is_backend_selection_pending());
        assert_eq!(p.state().agent, "cursor");
        assert_eq!(p.state().model, "composer-2.5");
    }

    #[test]
    fn backend_selection_answer_claude_acp() {
        // Given
        let mut p = make_presenter();
        let q = crate::backend::backend_selection_question();
        p.show_backend_selection(q, 0);

        // When
        p.handle_intent(UserIntent::AnswerSelect(1));

        // Then
        assert_eq!(p.state().agent, "claude-acp");
        assert_eq!(p.state().model, "opus");
    }

    #[test]
    fn quit_broadcasts_intent_received_for_tui_should_quit_sync() {
        // Given
        let (event_tx, _) = tokio::sync::broadcast::channel(16);
        let (intent_tx, _) = mpsc::channel();
        let mut p = make_presenter()
            .with_broadcast(event_tx.clone())
            .with_intent_sender(intent_tx);
        let mut conn = p.connect_view().expect("connect_view should return Some");
        p.state.mode = AppMode::ErrorRecovery {
            error_message: "workflow failed".to_string(),
        };

        // When
        p.handle_intent(UserIntent::Quit);

        // Then
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
        // Given
        let tmp_guard = tempfile::tempdir().unwrap();
        let tmp = tmp_guard.path();
        std::fs::create_dir_all(tmp.join("artifacts")).unwrap();
        let on_disk = "# Plan\n\nDOC_FROM_DISK_UNIQUE_42\n";
        std::fs::write(tmp.join("artifacts").join("SessionDoc.md"), on_disk).unwrap();

        let mut p = make_presenter();
        p.workflow.session_dir = Some(tmp.to_path_buf());
        p.state.mode = AppMode::DocumentReview {
            content: "STALE_SNAPSHOT_NOT_ON_DISK".to_string(),
        };

        // When
        p.handle_intent(UserIntent::ViewSessionDocument);

        // Then
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
    }

    #[test]
    fn view_session_document_markdown_viewer_shows_uuid_root_when_workflow_dir_nested() {
        // Given
        let root_guard = tempfile::tempdir().unwrap();
        let root = root_guard.path();
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
        p.workflow.session_dir = Some(nested.clone());
        p.state.mode = AppMode::DocumentReview {
            content: "STALE".to_string(),
        };

        // When
        p.handle_intent(UserIntent::ViewSessionDocument);

        // Then
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
        // Given
        let mut p = make_presenter();
        inject_workflow_event(
            &mut p,
            WorkflowEvent::AgentOutput("partial_without_newline".to_string()),
        );

        // When
        p.poll_workflow();

        // Then
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
        // Given
        let (mut p, mut rx) = make_presenter_with_broadcast();
        inject_workflow_event(
            &mut p,
            WorkflowEvent::AgentOutput("single_line\n".to_string()),
        );

        // When
        p.poll_workflow();

        // Then
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

    #[test]
    fn submit_start_slash_without_recipe_resolver_logs_as_normal_feature_input() {
        // Given
        let mut p = make_presenter();
        assert!(matches!(p.state().mode, AppMode::FeatureInput));

        // When
        p.handle_intent(UserIntent::SubmitFeatureInput(
            "/start-tdd a todo app".to_string(),
        ));

        // Then
        let user_prompts: Vec<_> = p
            .state()
            .activity_log
            .iter()
            .filter(|e| e.kind == ActivityKind::UserPrompt)
            .map(|e| e.text.as_str())
            .collect();
        assert_eq!(
            user_prompts,
            vec!["/start-tdd a todo app"],
            "without recipe_resolver, /start-* must fall through so the user sees a normal submit"
        );
    }

    /// `/start-*` calls `restart_workflow` while the first run may still be blocked on the first
    /// `answer_rx.recv()` (no `session_dir`, no `initial_prompt`). The old workflow thread must
    /// exit when the answer channel closes so `join()` cannot deadlock.
    #[test]
    fn start_slash_restart_unblocks_workflow_waiting_for_first_feature_input() {
        use crate::presenter::presenter_test_recipe::EmptyPresenterTestRecipe;
        use crate::{AnyBackend, StubBackend};

        // Given
        let tmp_guard = tempfile::tempdir().unwrap();
        let tmp = tmp_guard.path().to_path_buf();

        let resolver =
            Arc::new(|_: &str| Ok(Arc::new(EmptyPresenterTestRecipe) as Arc<dyn WorkflowRecipe>));
        let mut p = Presenter::new(
            "stub",
            "opus",
            Arc::new(EmptyPresenterTestRecipe),
            tmp.clone(),
        )
        .with_recipe_resolver(resolver);
        let backend = SharedBackend::from_any(AnyBackend::Stub(StubBackend::new()));
        p.start_workflow(
            backend,
            tmp.clone(),
            None,
            None,
            None,
            None,
            false,
            None,
            None,
            None,
        );
        std::thread::sleep(std::time::Duration::from_millis(150));

        // When / Then — handle_intent must return without deadlocking
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let _ = std::thread::spawn(move || {
            p.handle_intent(UserIntent::SubmitFeatureInput(
                "/start-tdd hello world".to_string(),
            ));
            let _ = done_tx.send(());
        });
        done_rx
            .recv_timeout(std::time::Duration::from_secs(3))
            .expect("handle_intent must not deadlock waiting on workflow join");
    }
}
