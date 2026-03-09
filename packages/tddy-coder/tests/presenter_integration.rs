//! Integration tests: Presenter with TestView and StubBackend.
//!
//! Scenario-based tests that drive the full workflow without a terminal.

use std::time::Duration;

use tddy_coder::{ActivityEntry, AppMode, Presenter, PresenterView, UserIntent};
use tddy_core::{AnyBackend, SharedBackend, StubBackend};

/// Events collected by TestView for assertions.
#[derive(Debug, Clone)]
pub enum TestEvent {
    ModeChanged(AppMode),
    ActivityLogged(ActivityEntry),
    GoalStarted(String),
    StateChanged { from: String, to: String },
    WorkflowComplete(Result<String, String>),
    AgentOutput(String),
    InboxChanged(Vec<String>),
}

/// TestView: implements PresenterView and collects all events.
pub struct TestView {
    pub events: Vec<TestEvent>,
}

impl TestView {
    pub fn new() -> Self {
        TestView { events: Vec::new() }
    }

    pub fn events(&self) -> &[TestEvent] {
        &self.events
    }
}

impl Default for TestView {
    fn default() -> Self {
        Self::new()
    }
}

impl PresenterView for TestView {
    fn on_mode_changed(&mut self, mode: &AppMode) {
        self.events.push(TestEvent::ModeChanged(mode.clone()));
    }

    fn on_activity_logged(&mut self, entry: &ActivityEntry) {
        self.events.push(TestEvent::ActivityLogged(entry.clone()));
    }

    fn on_goal_started(&mut self, goal: &str) {
        self.events.push(TestEvent::GoalStarted(goal.to_string()));
    }

    fn on_state_changed(&mut self, from: &str, to: &str) {
        self.events.push(TestEvent::StateChanged {
            from: from.to_string(),
            to: to.to_string(),
        });
    }

    fn on_workflow_complete(&mut self, result: &Result<String, String>) {
        self.events
            .push(TestEvent::WorkflowComplete(result.clone()));
    }

    fn on_agent_output(&mut self, text: &str) {
        self.events.push(TestEvent::AgentOutput(text.to_string()));
    }

    fn on_inbox_changed(&mut self, inbox: &[String]) {
        self.events.push(TestEvent::InboxChanged(inbox.to_vec()));
    }
}

fn create_stub_backend() -> SharedBackend {
    SharedBackend::from_any(AnyBackend::Stub(StubBackend::new()))
}

/// Full workflow scenario: SubmitFeatureInput → run to completion → assert WorkflowComplete(Ok).
#[test]
fn full_workflow_completes_with_stub_backend() {
    let view = TestView::new();
    let mut presenter = Presenter::new(view, "stub", "default");
    let backend = create_stub_backend();
    let output_dir = std::env::temp_dir().join("tddy-presenter-test-full");

    presenter.handle_intent(UserIntent::SubmitFeatureInput("Build auth".to_string()));
    presenter.start_workflow(backend, output_dir, Some("Build auth".to_string()));

    let mut iterations = 0;
    let max_iterations = 500;
    while !presenter.is_done() && iterations < max_iterations {
        presenter.poll_workflow();
        if matches!(presenter.state().mode, AppMode::Select { .. }) {
            presenter.handle_intent(UserIntent::AnswerSelect(0));
        } else if matches!(presenter.state().mode, AppMode::MultiSelect { .. }) {
            presenter.handle_intent(UserIntent::AnswerMultiSelect(vec![0], None));
        } else if matches!(presenter.state().mode, AppMode::TextInput { .. }) {
            presenter.handle_intent(UserIntent::AnswerText("test".to_string()));
        }
        iterations += 1;
        std::thread::sleep(Duration::from_millis(10));
    }

    assert!(
        presenter.is_done(),
        "workflow should complete within {} iterations",
        max_iterations
    );

    let events = presenter.view_mut().events();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, TestEvent::GoalStarted(g) if g == "plan")),
        "expected GoalStarted(plan) in events: {:?}",
        events
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, TestEvent::WorkflowComplete(Ok(_)))),
        "expected WorkflowComplete(Ok) in events: {:?}",
        events
    );
}

/// Clarification scenario: StubBackend with CLARIFY → AnswerSelect → assert answers sent.
#[test]
fn clarification_roundtrip_sends_answers() {
    let view = TestView::new();
    let mut presenter = Presenter::new(view, "stub", "default");
    let backend = create_stub_backend();
    let output_dir = std::env::temp_dir().join("tddy-presenter-test-clarify");

    presenter.start_workflow(backend, output_dir, Some("CLARIFY test".to_string()));

    let mut iterations = 0;
    let max_iterations = 500;
    while !presenter.is_done() && iterations < max_iterations {
        presenter.poll_workflow();
        if matches!(presenter.state().mode, AppMode::Select { .. }) {
            presenter.handle_intent(UserIntent::AnswerSelect(0));
        } else if matches!(presenter.state().mode, AppMode::MultiSelect { .. }) {
            presenter.handle_intent(UserIntent::AnswerMultiSelect(vec![0], None));
        } else if matches!(presenter.state().mode, AppMode::TextInput { .. }) {
            presenter.handle_intent(UserIntent::AnswerText("test".to_string()));
        }
        iterations += 1;
        std::thread::sleep(Duration::from_millis(10));
    }

    assert!(presenter.is_done());
    let events = presenter.view_mut().events();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, TestEvent::ModeChanged(AppMode::Select { .. }))),
        "expected Select mode during clarification: {:?}",
        events
    );
    let result = presenter
        .take_workflow_result()
        .expect("should have result");
    assert!(
        result.is_ok(),
        "expected workflow to complete successfully, got: {:?}",
        result
    );
}

/// Inbox scenario: QueuePrompt during Running → WorkflowComplete → assert dequeued.
#[test]
fn inbox_queue_and_dequeue() {
    let view = TestView::new();
    let mut presenter = Presenter::new(view, "stub", "default");
    let backend = create_stub_backend();
    let output_dir = std::env::temp_dir().join("tddy-presenter-test-inbox");

    presenter.handle_intent(UserIntent::SubmitFeatureInput("Build auth".to_string()));
    presenter.start_workflow(backend, output_dir, Some("Build auth".to_string()));

    let mut iterations = 0;
    let max_iterations = 500;
    let mut queued = false;
    while !presenter.is_done() && iterations < max_iterations {
        presenter.poll_workflow();
        if matches!(presenter.state().mode, AppMode::Running) && !queued {
            presenter.handle_intent(UserIntent::QueuePrompt("fix the login bug".to_string()));
            queued = true;
        }
        if matches!(presenter.state().mode, AppMode::Select { .. }) {
            presenter.handle_intent(UserIntent::AnswerSelect(0));
        } else if matches!(presenter.state().mode, AppMode::MultiSelect { .. }) {
            presenter.handle_intent(UserIntent::AnswerMultiSelect(vec![0], None));
        }
        iterations += 1;
        std::thread::sleep(Duration::from_millis(10));
    }

    assert!(
        presenter.is_done(),
        "workflow should complete within {} iterations",
        max_iterations
    );
    let events = presenter.view_mut().events();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, TestEvent::InboxChanged(inbox) if inbox.len() == 1)),
        "expected InboxChanged with 1 item: {:?}",
        events
    );
}

/// Error scenario: StubBackend with FAIL_INVOKE → assert WorkflowComplete(Err).
#[test]
fn workflow_error_propagates() {
    let view = TestView::new();
    let mut presenter = Presenter::new(view, "stub", "default");
    let backend = create_stub_backend();
    let output_dir = std::env::temp_dir().join("tddy-presenter-test-error");

    presenter.handle_intent(UserIntent::SubmitFeatureInput(
        "FAIL_INVOKE test".to_string(),
    ));
    presenter.start_workflow(backend, output_dir, Some("FAIL_INVOKE test".to_string()));

    let mut iterations = 0;
    let max_iterations = 200;
    while !presenter.is_done() && iterations < max_iterations {
        presenter.poll_workflow();
        iterations += 1;
        std::thread::sleep(Duration::from_millis(10));
    }

    let events = presenter.view_mut().events();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, TestEvent::WorkflowComplete(Err(_)))),
        "expected WorkflowComplete(Err) for FAIL_INVOKE: {:?}",
        events
    );
}
