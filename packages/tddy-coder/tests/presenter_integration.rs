//! Integration tests: Presenter with TestView and StubBackend.
//!
//! Scenario-based tests that drive the full workflow without a terminal.

mod common;

use serial_test::serial;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tddy_coder::{ActivityEntry, AppMode, Presenter, PresenterView, UserIntent};
use tddy_core::{
    backend::{CodingBackend, Goal, InvokeRequest, InvokeResponse},
    output::{SESSIONS_SUBDIR, TDDY_SESSIONS_DIR_ENV},
    BackendError, SharedBackend, StubBackend, WorkflowCompletePayload,
};

/// Events collected by TestView for assertions.
#[derive(Debug, Clone)]
pub enum TestEvent {
    ModeChanged(AppMode),
    ActivityLogged(ActivityEntry),
    GoalStarted(String),
    StateChanged { from: String, to: String },
    WorkflowComplete(Result<WorkflowCompletePayload, String>),
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

    fn on_activity_logged(&mut self, entry: &ActivityEntry, _activity_log_len: usize) {
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

    fn on_workflow_complete(&mut self, result: &Result<WorkflowCompletePayload, String>) {
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
    SharedBackend::from_arc(Arc::new(StubBackend::new()))
}

/// Backend that fails plan invocations when working_dir is not a git repo.
/// Used to enforce that plan refinement uses repo_path (not plan_dir.parent()) when plan_dir is under sessions.
struct AssertingRepoBackend {
    inner: StubBackend,
}

impl AssertingRepoBackend {
    fn new() -> Self {
        Self {
            inner: StubBackend::new(),
        }
    }
}

#[async_trait]
impl CodingBackend for AssertingRepoBackend {
    fn submit_channel(&self) -> Option<&tddy_core::toolcall::SubmitResultChannel> {
        self.inner.submit_channel()
    }

    async fn invoke(&self, request: InvokeRequest) -> Result<InvokeResponse, BackendError> {
        if request.goal == Goal::Plan {
            if let Some(ref wd) = request.working_dir {
                let git_dir = wd.join(".git");
                if !git_dir.exists() {
                    return Err(BackendError::InvocationFailed(format!(
                        "plan working_dir must be a git repo (expected .git at {:?})",
                        git_dir
                    )));
                }
            }
        }
        self.inner.invoke(request).await
    }

    fn name(&self) -> &str {
        "asserting-repo"
    }
}

fn create_asserting_repo_backend() -> SharedBackend {
    SharedBackend::from_arc(Arc::new(AssertingRepoBackend::new()))
}

/// Full workflow scenario: SubmitFeatureInput → run to completion → assert WorkflowComplete(Ok).
#[test]
fn full_workflow_completes_with_stub_backend() {
    let view = TestView::new();
    let mut presenter = Presenter::new(view, "stub", "default");
    let backend = create_stub_backend();
    let (output_dir, _) = common::temp_dir_with_git_repo("presenter-full");

    presenter.handle_intent(UserIntent::SubmitFeatureInput("Build auth".to_string()));
    presenter.start_workflow(
        backend,
        output_dir,
        None,
        Some("Build auth".to_string()),
        None,
        None,
        false,
        None,
        None,
        None,
    );

    let mut iterations = 0;
    let max_iterations = 4000;
    while !presenter.is_done() && iterations < max_iterations {
        presenter.poll_workflow();
        if matches!(presenter.state().mode, AppMode::PlanReview { .. }) {
            presenter.handle_intent(UserIntent::ApprovePlan);
        } else if matches!(presenter.state().mode, AppMode::Select { .. }) {
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
        "workflow should complete within {} iterations; last mode: {:?}",
        max_iterations,
        presenter.state().mode
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

/// When output_dir is "." (TUI default), plan_dir must be under sessions_base_path (~/.tddy/sessions),
/// not under the resolved current_dir. MDs (PRD.md, progress.md, etc.) go to plan_dir.
#[test]
#[serial]
fn plan_dir_under_sessions_base_when_output_dir_is_dot() {
    let sessions_base = std::env::temp_dir().join("tddy-plan-dir-test-sessions");
    let _ = std::fs::remove_dir_all(&sessions_base);
    std::fs::create_dir_all(&sessions_base).expect("create sessions base");
    let sessions_base_str = sessions_base.to_str().expect("path");
    std::env::set_var(TDDY_SESSIONS_DIR_ENV, sessions_base_str);

    let (repo_dir, _) = common::temp_dir_with_git_repo("plan-dir-test");

    let view = TestView::new();
    let mut presenter = Presenter::new(view, "stub", "default");
    let backend = create_stub_backend();

    let original_cwd = std::env::current_dir().expect("cwd");
    std::env::set_current_dir(&repo_dir).expect("chdir to repo");

    presenter.handle_intent(UserIntent::SubmitFeatureInput("Auth feature".to_string()));
    presenter.start_workflow(
        backend,
        std::path::PathBuf::from("."),
        None,
        Some("Auth feature".to_string()),
        None,
        None,
        false,
        Some(uuid::Uuid::now_v7().to_string()),
        None,
        None,
    );

    let mut iterations = 0;
    let max_iterations = 4000;
    while !presenter.is_done() && iterations < max_iterations {
        presenter.poll_workflow();
        if matches!(presenter.state().mode, AppMode::PlanReview { .. }) {
            presenter.handle_intent(UserIntent::ApprovePlan);
        } else if matches!(presenter.state().mode, AppMode::Select { .. }) {
            presenter.handle_intent(UserIntent::AnswerSelect(0));
        } else if matches!(presenter.state().mode, AppMode::MultiSelect { .. }) {
            presenter.handle_intent(UserIntent::AnswerMultiSelect(vec![0], None));
        } else if matches!(presenter.state().mode, AppMode::TextInput { .. }) {
            presenter.handle_intent(UserIntent::AnswerText("test".to_string()));
        }
        iterations += 1;
        std::thread::sleep(Duration::from_millis(10));
    }

    let _ = std::env::set_current_dir(&original_cwd);

    assert!(
        presenter.is_done(),
        "workflow should complete within {} iterations; last mode: {:?}",
        max_iterations,
        presenter.state().mode
    );

    let payload = presenter
        .view_mut()
        .events()
        .iter()
        .find_map(|e| match e {
            TestEvent::WorkflowComplete(Ok(p)) => Some(p.clone()),
            _ => None,
        })
        .expect("WorkflowComplete(Ok) with payload");

    let plan_dir = payload
        .plan_dir
        .as_ref()
        .expect("plan_dir must be set in payload");

    let expected_sessions_base = Path::new(sessions_base_str);
    let expected_plan_parent = expected_sessions_base.join(SESSIONS_SUBDIR);
    assert!(
        plan_dir.starts_with(&expected_plan_parent),
        "plan_dir {:?} must be under {}/sessions/ (sessions_base_path), not under repo {:?}",
        plan_dir,
        sessions_base_str,
        repo_dir
    );
    assert!(
        !plan_dir.starts_with(&repo_dir),
        "plan_dir {:?} must NOT be under repo {:?}",
        plan_dir,
        repo_dir
    );

    let _ = std::fs::remove_dir_all(&sessions_base);
    let _ = std::fs::remove_dir_all(repo_dir.parent().unwrap_or(&repo_dir));
}

/// When plan_dir is under sessions (output_dir "."), RefinePlan must use repo_path from changeset
/// for output_dir, not plan_dir.parent(). AssertingRepoBackend fails if plan working_dir lacks .git.
#[test]
#[serial]
fn plan_dir_under_sessions_refine_uses_repo_as_working_dir() {
    let sessions_base = std::env::temp_dir().join("tddy-plan-refine-sessions");
    let _ = std::fs::remove_dir_all(&sessions_base);
    std::fs::create_dir_all(&sessions_base).expect("create sessions base");
    let sessions_base_str = sessions_base.to_str().expect("path");
    std::env::set_var(TDDY_SESSIONS_DIR_ENV, sessions_base_str);

    let (repo_dir, _) = common::temp_dir_with_git_repo("plan-refine-repo");

    let view = TestView::new();
    let mut presenter = Presenter::new(view, "stub", "default");
    let backend = create_asserting_repo_backend();

    let original_cwd = std::env::current_dir().expect("cwd");
    std::env::set_current_dir(&repo_dir).expect("chdir to repo");

    presenter.handle_intent(UserIntent::SubmitFeatureInput("Auth feature".to_string()));
    presenter.start_workflow(
        backend,
        std::path::PathBuf::from("."),
        None,
        Some("Auth feature".to_string()),
        None,
        None,
        false,
        Some(uuid::Uuid::now_v7().to_string()),
        None,
        None,
    );

    let mut iterations = 0;
    let max_iterations = 4000;
    let mut plan_review_count = 0;
    let mut saw_error_recovery = false;
    while !presenter.is_done() && !saw_error_recovery && iterations < max_iterations {
        presenter.poll_workflow();
        if matches!(presenter.state().mode, AppMode::PlanReview { .. }) {
            plan_review_count += 1;
            if plan_review_count == 1 {
                presenter.handle_intent(UserIntent::RefinePlan);
            } else {
                presenter.handle_intent(UserIntent::ApprovePlan);
            }
        } else if matches!(presenter.state().mode, AppMode::Select { .. }) {
            presenter.handle_intent(UserIntent::AnswerSelect(0));
        } else if matches!(presenter.state().mode, AppMode::MultiSelect { .. }) {
            presenter.handle_intent(UserIntent::AnswerMultiSelect(vec![0], None));
        } else if matches!(presenter.state().mode, AppMode::TextInput { .. }) {
            presenter.handle_intent(UserIntent::AnswerText(
                "Add OAuth support to the plan".to_string(),
            ));
        } else if matches!(presenter.state().mode, AppMode::ErrorRecovery { .. }) {
            saw_error_recovery = true;
        }
        iterations += 1;
        std::thread::sleep(Duration::from_millis(10));
    }

    let _ = std::env::set_current_dir(&original_cwd);
    let _ = std::fs::remove_dir_all(&sessions_base);
    let _ = std::fs::remove_dir_all(repo_dir.parent().unwrap_or(&repo_dir));

    assert!(
        !saw_error_recovery,
        "refine must use repo_path for output_dir when plan_dir is under sessions; got ErrorRecovery: {:?}",
        presenter.state().mode
    );
    assert!(
        presenter.is_done(),
        "workflow should complete within {} iterations; last mode: {:?}",
        max_iterations,
        presenter.state().mode
    );
    assert!(
        plan_review_count >= 2,
        "expected PlanReview at least twice (initial + after refine)"
    );
    let events = presenter.view_mut().events();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, TestEvent::WorkflowComplete(Ok(_)))),
        "expected WorkflowComplete(Ok); refine must use repo_path for output_dir when plan_dir is under sessions. Events: {:?}",
        events
    );
}

/// Clarification scenario: StubBackend with CLARIFY → AnswerSelect → assert answers sent.
#[test]
fn clarification_roundtrip_sends_answers() {
    let view = TestView::new();
    let mut presenter = Presenter::new(view, "stub", "default");
    let backend = create_stub_backend();
    let (output_dir, _) = common::temp_dir_with_git_repo("presenter-clarify");

    presenter.start_workflow(
        backend,
        output_dir,
        None,
        Some("CLARIFY test".to_string()),
        None,
        None,
        false,
        None,
        None,
        None,
    );

    let mut iterations = 0;
    let max_iterations = 4000;
    while !presenter.is_done() && iterations < max_iterations {
        presenter.poll_workflow();
        if matches!(presenter.state().mode, AppMode::PlanReview { .. }) {
            presenter.handle_intent(UserIntent::ApprovePlan);
        } else if matches!(presenter.state().mode, AppMode::Select { .. }) {
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
        "workflow should complete within {} iterations; last mode: {:?}",
        max_iterations,
        presenter.state().mode
    );
    let events = presenter.view_mut().events();
    assert!(
        events.iter().any(
            |e| matches!(e, TestEvent::ModeChanged(AppMode::Select { .. }))
                || matches!(e, TestEvent::ModeChanged(AppMode::PlanReview { .. }))
        ),
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
    let (output_dir, _) = common::temp_dir_with_git_repo("presenter-inbox");

    presenter.handle_intent(UserIntent::SubmitFeatureInput("Build auth".to_string()));
    presenter.start_workflow(
        backend,
        output_dir,
        None,
        Some("Build auth".to_string()),
        None,
        None,
        false,
        None,
        None,
        None,
    );

    let mut iterations = 0;
    let max_iterations = 1500;
    let mut queued = false;
    while !presenter.is_done() && iterations < max_iterations {
        presenter.poll_workflow();
        // Handle intents; approve sets Running, so we can queue in same iteration.
        if matches!(presenter.state().mode, AppMode::PlanReview { .. }) {
            presenter.handle_intent(UserIntent::ApprovePlan);
        }
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
        "workflow should complete within {} iterations; last mode: {:?}",
        max_iterations,
        presenter.state().mode
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

/// Plan approval: After plan completes, PlanReview mode appears. ApprovePlan proceeds to next step.
#[test]
fn plan_approval_approve_proceeds_to_next_step() {
    let view = TestView::new();
    let mut presenter = Presenter::new(view, "stub", "default");
    let backend = create_stub_backend();
    let (output_dir, _) = common::temp_dir_with_git_repo("presenter-plan-approve");

    presenter.handle_intent(UserIntent::SubmitFeatureInput("Build auth".to_string()));
    presenter.start_workflow(
        backend,
        output_dir,
        None,
        Some("Build auth".to_string()),
        None,
        None,
        false,
        None,
        None,
        None,
    );

    let mut iterations = 0;
    let max_iterations = 4000;
    while !presenter.is_done() && iterations < max_iterations {
        presenter.poll_workflow();
        if matches!(presenter.state().mode, AppMode::PlanReview { .. }) {
            presenter.handle_intent(UserIntent::ApprovePlan);
        } else if matches!(presenter.state().mode, AppMode::Select { .. }) {
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
        "workflow should complete within {} iterations; last mode: {:?}",
        max_iterations,
        presenter.state().mode
    );
    let events = presenter.view_mut().events();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, TestEvent::ModeChanged(AppMode::PlanReview { .. }))),
        "expected PlanReview mode: {:?}",
        events
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, TestEvent::WorkflowComplete(Ok(_)))),
        "expected WorkflowComplete(Ok): {:?}",
        events
    );
}

/// Plan approval: ViewPlan opens MarkdownViewer, DismissViewer returns to PlanReview, ApprovePlan proceeds.
#[test]
fn plan_approval_view_then_approve() {
    let view = TestView::new();
    let mut presenter = Presenter::new(view, "stub", "default");
    let backend = create_stub_backend();
    let (output_dir, _) = common::temp_dir_with_git_repo("presenter-plan-view");

    presenter.handle_intent(UserIntent::SubmitFeatureInput("Build auth".to_string()));
    presenter.start_workflow(
        backend,
        output_dir,
        None,
        Some("Build auth".to_string()),
        None,
        None,
        false,
        None,
        None,
        None,
    );

    let mut iterations = 0;
    let max_iterations = 4000;
    let mut viewed = false;
    while !presenter.is_done() && iterations < max_iterations {
        presenter.poll_workflow();
        if matches!(presenter.state().mode, AppMode::MarkdownViewer { .. }) {
            if !viewed {
                presenter.handle_intent(UserIntent::DismissViewer);
                viewed = true;
            }
        } else if matches!(presenter.state().mode, AppMode::PlanReview { .. }) {
            if viewed {
                presenter.handle_intent(UserIntent::ApprovePlan);
            } else {
                presenter.handle_intent(UserIntent::ViewPlan);
            }
        } else if matches!(presenter.state().mode, AppMode::Select { .. }) {
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
        "workflow should complete within {} iterations; last mode: {:?}",
        max_iterations,
        presenter.state().mode
    );
    let events = presenter.view_mut().events();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, TestEvent::ModeChanged(AppMode::MarkdownViewer { .. }))),
        "expected MarkdownViewer mode: {:?}",
        events
    );
}

/// Plan approval: RefinePlan enters TextInput, AnswerText sends feedback, plan re-runs, approval re-appears.
#[test]
fn plan_approval_refine_re_shows_approval() {
    let view = TestView::new();
    let mut presenter = Presenter::new(view, "stub", "default");
    let backend = create_stub_backend();
    let (output_dir, _) = common::temp_dir_with_git_repo("presenter-plan-refine");

    presenter.handle_intent(UserIntent::SubmitFeatureInput("Build auth".to_string()));
    presenter.start_workflow(
        backend,
        output_dir,
        None,
        Some("Build auth".to_string()),
        None,
        None,
        false,
        None,
        None,
        None,
    );

    let mut iterations = 0;
    let max_iterations = 4000;
    let mut plan_review_count = 0;
    while !presenter.is_done() && iterations < max_iterations {
        presenter.poll_workflow();
        if matches!(presenter.state().mode, AppMode::PlanReview { .. }) {
            plan_review_count += 1;
            if plan_review_count == 1 {
                presenter.handle_intent(UserIntent::RefinePlan);
            } else {
                presenter.handle_intent(UserIntent::ApprovePlan);
            }
        } else if matches!(presenter.state().mode, AppMode::Select { .. }) {
            presenter.handle_intent(UserIntent::AnswerSelect(0));
        } else if matches!(presenter.state().mode, AppMode::MultiSelect { .. }) {
            presenter.handle_intent(UserIntent::AnswerMultiSelect(vec![0], None));
        } else if matches!(presenter.state().mode, AppMode::TextInput { .. }) {
            presenter.handle_intent(UserIntent::AnswerText(
                "Add OAuth support to the plan".to_string(),
            ));
        }
        iterations += 1;
        std::thread::sleep(Duration::from_millis(10));
    }

    assert!(
        presenter.is_done(),
        "workflow should complete within {} iterations; last mode: {:?}",
        max_iterations,
        presenter.state().mode
    );
    assert!(
        plan_review_count >= 2,
        "expected PlanReview at least twice (initial + after refine)"
    );
}

/// Plan approval from viewer: ViewPlan → ApprovePlan directly in MarkdownViewer (no DismissViewer) → workflow completes.
/// Asserts presenter is_done() and PlanReview appears exactly once (no return to PlanReview after viewer approval).
#[test]
fn plan_approval_from_markdown_viewer() {
    let view = TestView::new();
    let mut presenter = Presenter::new(view, "stub", "default");
    let backend = create_stub_backend();
    let (output_dir, _) = common::temp_dir_with_git_repo("presenter-viewer-approve");

    presenter.handle_intent(UserIntent::SubmitFeatureInput("Build auth".to_string()));
    presenter.start_workflow(
        backend,
        output_dir,
        None,
        Some("Build auth".to_string()),
        None,
        None,
        false,
        None,
        None,
        None,
    );

    let mut iterations = 0;
    let max_iterations = 4000;
    let mut approved_from_viewer = false;
    while !presenter.is_done() && iterations < max_iterations {
        presenter.poll_workflow();
        if matches!(presenter.state().mode, AppMode::PlanReview { .. }) && !approved_from_viewer {
            presenter.handle_intent(UserIntent::ViewPlan);
        } else if matches!(presenter.state().mode, AppMode::MarkdownViewer { .. })
            && !approved_from_viewer
        {
            presenter.handle_intent(UserIntent::ApprovePlan);
            approved_from_viewer = true;
        } else if matches!(presenter.state().mode, AppMode::Select { .. }) {
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
        "workflow should complete after approving from viewer; last mode: {:?}",
        presenter.state().mode
    );

    let events = presenter.view_mut().events();
    let plan_review_count = events
        .iter()
        .filter(|e| matches!(e, TestEvent::ModeChanged(AppMode::PlanReview { .. })))
        .count();
    assert_eq!(
        plan_review_count, 1,
        "PlanReview should appear exactly once when approving directly from viewer: {:?}",
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
    presenter.start_workflow(
        backend,
        output_dir,
        None,
        Some("FAIL_INVOKE test".to_string()),
        None,
        None,
        false,
        None,
        None,
        None,
    );

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
