//! Integration tests: Presenter with broadcast event collection and StubBackend.
//!
//! Scenario-based tests that drive the full workflow without a terminal.

mod common;

use serial_test::serial;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tddy_coder::{ActivityEntry, AppMode, Presenter, UserIntent};
use tddy_core::{
    backend::{CodingBackend, InvokeRequest, InvokeResponse},
    changeset::read_changeset,
    output::{SESSIONS_SUBDIR, TDDY_SESSIONS_DIR_ENV},
    BackendError, PresenterEvent, SharedBackend, StubBackend, WorkflowCompletePayload,
};
use tddy_workflow_recipes::{resolve_workflow_recipe_from_cli_name, BugfixRecipe, TddRecipe};
use tokio::sync::broadcast;

/// Events collected from broadcast for assertions.
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

fn first_session_dir_under(sessions_path: &Path) -> Option<PathBuf> {
    std::fs::read_dir(sessions_path)
        .ok()?
        .flatten()
        .find_map(|e| {
            let p = e.path();
            if p.is_dir() {
                Some(p)
            } else {
                None
            }
        })
}

fn presenter_event_to_test_event(ev: PresenterEvent) -> Option<TestEvent> {
    match ev {
        PresenterEvent::ModeChanged(details) => Some(TestEvent::ModeChanged(details.mode)),
        PresenterEvent::ActivityLogged(entry) => Some(TestEvent::ActivityLogged(entry)),
        PresenterEvent::GoalStarted(goal) => Some(TestEvent::GoalStarted(goal)),
        PresenterEvent::StateChanged { from, to } => Some(TestEvent::StateChanged { from, to }),
        PresenterEvent::WorkflowComplete(result) => Some(TestEvent::WorkflowComplete(result)),
        PresenterEvent::AgentOutput(text) => Some(TestEvent::AgentOutput(text)),
        PresenterEvent::InboxChanged(inbox) => Some(TestEvent::InboxChanged(inbox)),
        PresenterEvent::IntentReceived(_) => None,
        PresenterEvent::BackendSelected { .. } => None,
        PresenterEvent::ShouldQuit => None,
    }
}

/// Collects events from broadcast receiver. Call drain() during the loop to accumulate.
struct EventCollector {
    rx: broadcast::Receiver<PresenterEvent>,
    events: Vec<TestEvent>,
}

impl EventCollector {
    fn new(rx: broadcast::Receiver<PresenterEvent>) -> Self {
        Self {
            rx,
            events: Vec::new(),
        }
    }

    fn drain(&mut self) {
        while let Ok(ev) = self.rx.try_recv() {
            if let Some(te) = presenter_event_to_test_event(ev) {
                self.events.push(te);
            }
        }
    }

    fn events(&self) -> &[TestEvent] {
        &self.events
    }
}

/// Creates a Presenter with broadcast and an EventCollector for assertions.
fn presenter_with_events() -> (Presenter, EventCollector) {
    let (event_tx, event_rx) = broadcast::channel(256);
    let presenter = Presenter::new("stub", "default", Arc::new(TddRecipe)).with_broadcast(event_tx);
    let collector = EventCollector::new(event_rx);
    (presenter, collector)
}

fn bugfix_presenter_with_events() -> (Presenter, EventCollector) {
    let (event_tx, event_rx) = broadcast::channel(256);
    let presenter =
        Presenter::new("stub", "default", Arc::new(BugfixRecipe)).with_broadcast(event_tx);
    let collector = EventCollector::new(event_rx);
    (presenter, collector)
}

fn create_stub_backend() -> SharedBackend {
    SharedBackend::from_arc(Arc::new(StubBackend::new()))
}

/// Backend that fails plan invocations when working_dir is not a git repo.
/// Used to enforce that plan refinement uses repo_path (not session_dir.parent()) when session_dir is under sessions.
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
        if request.goal_id.as_str() == "plan" {
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
#[serial]
fn full_workflow_completes_with_stub_backend() {
    let (mut presenter, mut events) = presenter_with_events();
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
        events.drain();
        if matches!(presenter.state().mode, AppMode::DocumentReview { .. }) {
            presenter.handle_intent(UserIntent::ApproveSessionDocument);
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

    events.drain();
    let evs = events.events();
    assert!(
        evs.iter()
            .any(|e| matches!(e, TestEvent::GoalStarted(g) if g == "plan")),
        "expected GoalStarted(plan) in events: {:?}",
        evs
    );
    assert!(
        evs.iter()
            .any(|e| matches!(e, TestEvent::WorkflowComplete(Ok(_)))),
        "expected WorkflowComplete(Ok) in events: {:?}",
        evs
    );

    let activity_texts: Vec<&str> = evs
        .iter()
        .filter_map(|e| {
            if let TestEvent::ActivityLogged(entry) = e {
                Some(entry.text.as_str())
            } else {
                None
            }
        })
        .collect();
    assert!(
        activity_texts
            .iter()
            .any(|t| t.to_lowercase().contains("exited")),
        "expected activity log to contain an entry that the agent exited, got: {:?}",
        activity_texts
    );

    // Acceptance: success completion transitions to FeatureInput (not Done)
    assert!(
        matches!(presenter.state().mode, AppMode::FeatureInput),
        "after workflow completion, mode should be FeatureInput (ready for new workflow), got {:?}",
        presenter.state().mode
    );
}

/// Baseline: TDD recipe reaches `plan` after the user submits feature text with no initial prompt.
#[test]
#[serial]
fn tdd_workflow_starts_plan_after_feature_submit() {
    let sessions_base = std::env::temp_dir().join("tddy-presenter-tdd-feature-submit");
    let _ = std::fs::remove_dir_all(&sessions_base);
    std::fs::create_dir_all(&sessions_base).expect("sessions base");
    std::env::set_var(TDDY_SESSIONS_DIR_ENV, sessions_base.to_str().unwrap());

    let (mut presenter, mut events) = presenter_with_events();
    let backend = create_stub_backend();
    let (output_dir, _) = common::temp_dir_with_git_repo("presenter-tdd-start");

    presenter.start_workflow(
        backend, output_dir, None, None, None, None, false, None, None, None,
    );

    presenter.handle_intent(UserIntent::SubmitFeatureInput("Build auth".to_string()));

    let mut iterations = 0;
    const MAX: usize = 4000;
    let mut saw_plan = false;
    while iterations < MAX {
        presenter.poll_workflow();
        events.drain();
        for e in events.events() {
            if let TestEvent::WorkflowComplete(Err(msg)) = e {
                std::env::remove_var(TDDY_SESSIONS_DIR_ENV);
                panic!("tdd workflow failed before plan: {}", msg);
            }
        }
        if events
            .events()
            .iter()
            .any(|e| matches!(e, TestEvent::GoalStarted(g) if g.as_str() == "plan"))
        {
            saw_plan = true;
            break;
        }
        if matches!(presenter.state().mode, AppMode::DocumentReview { .. }) {
            presenter.handle_intent(UserIntent::ApproveSessionDocument);
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

    std::env::remove_var(TDDY_SESSIONS_DIR_ENV);
    assert!(
        saw_plan,
        "expected GoalStarted(plan) within {} polls; last mode {:?}, events: {:?}",
        MAX,
        presenter.state().mode,
        events.events()
    );
}

/// PRD acceptance: bugfix session bootstrap completes and `changeset.yaml` records the recipe so
/// resume/daemon tooling stay consistent with recipe-driven policy.
#[test]
#[serial]
fn bugfix_start_session_reaches_reproduce_goal_after_bootstrap_fix() {
    let sessions_base = std::env::temp_dir().join("tddy-presenter-bugfix-bootstrap-recipe");
    let _ = std::fs::remove_dir_all(&sessions_base);
    std::fs::create_dir_all(&sessions_base).expect("sessions base");
    std::env::set_var(TDDY_SESSIONS_DIR_ENV, sessions_base.to_str().unwrap());

    let (mut presenter, mut events) = bugfix_presenter_with_events();
    let backend = create_stub_backend();
    let (output_dir, _) = common::temp_dir_with_git_repo("presenter-bugfix-bootstrap");

    presenter.start_workflow(
        backend, output_dir, None, None, None, None, false, None, None, None,
    );

    presenter.handle_intent(UserIntent::SubmitFeatureInput(
        "repro the crash".to_string(),
    ));

    let mut iterations = 0;
    const MAX: usize = 4000;
    let mut saw_reproduce = false;
    let sessions_path = sessions_base.join(SESSIONS_SUBDIR);
    while iterations < MAX {
        presenter.poll_workflow();
        events.drain();
        for e in events.events() {
            if let TestEvent::WorkflowComplete(Err(msg)) = e {
                std::env::remove_var(TDDY_SESSIONS_DIR_ENV);
                panic!("bugfix workflow failed before reproduce goal: {}", msg);
            }
        }
        if events
            .events()
            .iter()
            .any(|e| matches!(e, TestEvent::GoalStarted(g) if g.as_str() == "reproduce"))
        {
            saw_reproduce = true;
            break;
        }
        if matches!(presenter.state().mode, AppMode::DocumentReview { .. }) {
            presenter.handle_intent(UserIntent::ApproveSessionDocument);
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

    std::env::remove_var(TDDY_SESSIONS_DIR_ENV);
    assert!(
        saw_reproduce,
        "expected GoalStarted(reproduce) within {} polls; last mode {:?}, events: {:?}",
        MAX,
        presenter.state().mode,
        events.events()
    );
    let session_dir =
        first_session_dir_under(&sessions_path).expect("session directory under TDDY_SESSIONS_DIR");
    let cs = read_changeset(&session_dir).expect("changeset.yaml after bugfix bootstrap");
    assert_eq!(
        cs.recipe.as_deref(),
        Some("bugfix"),
        "changeset.yaml must record recipe \"bugfix\" for presenter/daemon parity (recipe policy / F3)"
    );
}

fn free_prompting_presenter_with_events() -> (Presenter, EventCollector) {
    let (event_tx, event_rx) = broadcast::channel(256);
    let recipe = resolve_workflow_recipe_from_cli_name("free-prompting")
        .expect("free-prompting must resolve for presenter smoke");
    let presenter = Presenter::new("stub", "default", recipe).with_broadcast(event_tx);
    let collector = EventCollector::new(event_rx);
    (presenter, collector)
}

/// PRD acceptance: TDD still pauses for session document approval after planning, and the on-disk
/// changeset records the active recipe (same contract as daemon `StartSession`).
#[test]
#[serial]
fn tdd_plan_still_triggers_session_document_approval_via_recipe_policy() {
    let sessions_base = std::env::temp_dir().join("tddy-presenter-tdd-recipe-policy");
    let _ = std::fs::remove_dir_all(&sessions_base);
    std::fs::create_dir_all(&sessions_base).expect("sessions base");
    std::env::set_var(TDDY_SESSIONS_DIR_ENV, sessions_base.to_str().unwrap());

    let (mut presenter, mut events) = presenter_with_events();
    let backend = create_stub_backend();
    let (output_dir, _) = common::temp_dir_with_git_repo("presenter-tdd-recipe-policy");

    presenter.start_workflow(
        backend, output_dir, None, None, None, None, false, None, None, None,
    );

    presenter.handle_intent(UserIntent::SubmitFeatureInput("Build auth".to_string()));

    let mut iterations = 0;
    const MAX: usize = 4000;
    let mut saw_document_review = false;
    let sessions_path = sessions_base.join(SESSIONS_SUBDIR);
    while iterations < MAX {
        presenter.poll_workflow();
        events.drain();
        for e in events.events() {
            if let TestEvent::WorkflowComplete(Err(msg)) = e {
                std::env::remove_var(TDDY_SESSIONS_DIR_ENV);
                panic!("tdd workflow failed before document approval: {}", msg);
            }
        }
        if events
            .events()
            .iter()
            .any(|e| matches!(e, TestEvent::ModeChanged(AppMode::DocumentReview { .. })))
        {
            saw_document_review = true;
        }
        if matches!(presenter.state().mode, AppMode::DocumentReview { .. }) {
            presenter.handle_intent(UserIntent::ApproveSessionDocument);
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

    std::env::remove_var(TDDY_SESSIONS_DIR_ENV);
    assert!(
        saw_document_review,
        "expected session document approval (DocumentReview) after plan per TDD recipe policy; last mode {:?}, events: {:?}",
        presenter.state().mode,
        events.events()
    );
    let session_dir =
        first_session_dir_under(&sessions_path).expect("session directory under TDDY_SESSIONS_DIR");
    let cs = read_changeset(&session_dir).expect("changeset.yaml when document approval ran");
    assert_eq!(
        cs.recipe.as_deref(),
        Some("tdd"),
        "changeset.yaml must record recipe \"tdd\" so approval/resume paths use recipe policy (F2/F3)"
    );
}

/// PRD acceptance: free-prompting disables primary session document approval — no DocumentReview gate
/// when the recipe policy says approval is skipped.
#[test]
#[serial]
fn free_prompting_session_has_no_document_approval_when_disabled_by_recipe() {
    let sessions_base = std::env::temp_dir().join("tddy-presenter-free-prompting-no-approval");
    let _ = std::fs::remove_dir_all(&sessions_base);
    std::fs::create_dir_all(&sessions_base).expect("sessions base");
    std::env::set_var(TDDY_SESSIONS_DIR_ENV, sessions_base.to_str().unwrap());

    let recipe = resolve_workflow_recipe_from_cli_name("free-prompting").expect("resolve");
    assert!(
        !recipe.uses_primary_session_document(),
        "free-prompting must not use a primary session document when approval is disabled by recipe policy"
    );

    let (mut presenter, mut events) = free_prompting_presenter_with_events();
    let backend = create_stub_backend();
    let (output_dir, _) = common::temp_dir_with_git_repo("presenter-free-prompting");

    presenter.start_workflow(
        backend, output_dir, None, None, None, None, false, None, None, None,
    );

    presenter.handle_intent(UserIntent::SubmitFeatureInput(
        "open-ended task".to_string(),
    ));

    let start_goal = recipe.start_goal().as_str().to_string();
    let mut iterations = 0;
    const MAX: usize = 4000;
    let mut saw_start_goal = false;
    while iterations < MAX {
        presenter.poll_workflow();
        events.drain();
        assert!(
            !events.events().iter().any(|e| {
                matches!(
                    e,
                    TestEvent::ModeChanged(AppMode::DocumentReview { .. })
                )
            }),
            "free-prompting must not enter DocumentReview when recipe disables session document approval; events: {:?}",
            events.events()
        );
        for e in events.events() {
            if let TestEvent::WorkflowComplete(Err(msg)) = e {
                std::env::remove_var(TDDY_SESSIONS_DIR_ENV);
                panic!("free-prompting workflow failed: {}", msg);
            }
        }
        if events
            .events()
            .iter()
            .any(|e| matches!(e, TestEvent::GoalStarted(g) if g.as_str() == start_goal.as_str()))
        {
            saw_start_goal = true;
            break;
        }
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

    std::env::remove_var(TDDY_SESSIONS_DIR_ENV);
    assert!(
        saw_start_goal,
        "expected GoalStarted({}) for free-prompting; last mode {:?}, events: {:?}",
        start_goal,
        presenter.state().mode,
        events.events()
    );
}

/// `tddy-demo --recipe free-prompting`: after the user submits the initial feature line, the TUI must
/// stay in [`AppMode::Running`] with the `prompting` goal active so Enter routes input to the stub agent.
/// If the workflow completes immediately (e.g. echo task advances to end), [`WorkflowEvent::WorkflowComplete`]
/// returns the UI to [`AppMode::FeatureInput`] and Enter submits another feature instead of agent input.
#[test]
#[serial]
fn free_prompting_stub_keeps_running_for_agent_input_after_feature_submit() {
    let sessions_base = std::env::temp_dir().join("tddy-presenter-free-prompting-running-mode");
    let _ = std::fs::remove_dir_all(&sessions_base);
    std::fs::create_dir_all(&sessions_base).expect("sessions base");
    std::env::set_var(TDDY_SESSIONS_DIR_ENV, sessions_base.to_str().unwrap());

    let (mut presenter, mut events) = free_prompting_presenter_with_events();
    let backend = create_stub_backend();
    let (output_dir, _) = common::temp_dir_with_git_repo("presenter-free-prompting-running");

    presenter.start_workflow(
        backend, output_dir, None, None, None, None, false, None, None, None,
    );

    presenter.handle_intent(UserIntent::SubmitFeatureInput(
        "first user line to stub".to_string(),
    ));

    let mut iterations = 0;
    const MAX: usize = 500;
    while iterations < MAX {
        presenter.poll_workflow();
        events.drain();
        for e in events.events() {
            if let TestEvent::WorkflowComplete(Err(msg)) = e {
                std::env::remove_var(TDDY_SESSIONS_DIR_ENV);
                panic!("free-prompting workflow failed: {}", msg);
            }
        }
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

    std::env::remove_var(TDDY_SESSIONS_DIR_ENV);
    assert!(
        !events
            .events()
            .iter()
            .any(|e| matches!(e, TestEvent::WorkflowComplete(Ok(_)))),
        "free-prompting must not finish with WorkflowComplete(Ok) while the prompting goal should still accept agent input; events: {:?}",
        events.events()
    );
    assert_eq!(
        presenter.state().mode,
        AppMode::Running,
        "expected AppMode::Running so the prompt bar talks to the agent; got {:?}, events: {:?}",
        presenter.state().mode,
        events.events()
    );
}

/// Simulates UI lag where `WorkflowComplete` is not polled before the user sends their first typed
/// feature: a preloaded `--prompt` lets the worker thread finish and drop `answer_rx` while
/// `workflow_result` is still unset. The next `SubmitFeatureInput` gets `SendError` on `answer_tx`
/// and is misrouted through `restart_workflow` (must reuse `workflow_session_dir` so no extra folder).
#[test]
#[serial]
fn bugfix_preloaded_then_first_typed_submit_without_poll_spawns_extra_session_dir() {
    let sessions_base =
        std::env::temp_dir().join("tddy-presenter-bugfix-preload-first-submit-race");
    let _ = std::fs::remove_dir_all(&sessions_base);
    std::fs::create_dir_all(&sessions_base).expect("sessions base");
    std::env::set_var(TDDY_SESSIONS_DIR_ENV, sessions_base.to_str().unwrap());

    let fixed_sid = "019d38cf-b74f-7d40-93ae-dcc2bf3f6936";
    let session_path = sessions_base.join(SESSIONS_SUBDIR).join(fixed_sid);
    std::fs::create_dir_all(&session_path).expect("pre-created session dir");

    let (mut presenter, mut events) = bugfix_presenter_with_events();
    let backend = create_stub_backend();
    let (output_dir, _) = common::temp_dir_with_git_repo("presenter-bugfix-preload-race");

    presenter.start_workflow(
        backend,
        output_dir,
        Some(session_path.clone()),
        Some("preloaded SKIP_QUESTIONS".to_string()),
        None,
        None,
        false,
        None,
        None,
        None,
    );

    std::thread::sleep(Duration::from_millis(400));
    assert!(
        !presenter.is_done(),
        "precondition: WorkflowComplete must not be processed yet (no poll_workflow)"
    );

    presenter.handle_intent(UserIntent::SubmitFeatureInput(
        "user first typed feature".to_string(),
    ));

    let mut iterations = 0;
    const MAX: usize = 12000;
    while iterations < MAX {
        presenter.poll_workflow();
        events.drain();
        if presenter.is_done() {
            break;
        }
        iterations += 1;
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        presenter.is_done(),
        "expected workflow to settle; mode {:?}, iterations {}",
        presenter.state().mode,
        iterations
    );

    let sessions_subdir = sessions_base.join(SESSIONS_SUBDIR);
    let dir_count = std::fs::read_dir(&sessions_subdir)
        .expect("read sessions subdir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .count();

    std::env::remove_var(TDDY_SESSIONS_DIR_ENV);
    assert_eq!(
        dir_count, 1,
        "only the bound session directory may exist under sessions/; extra directories indicate \
         the first typed submit was misrouted after a fast preloaded run (disconnected answer_tx). \
         count={}",
        dir_count
    );
    let _ = std::fs::remove_dir_all(&sessions_base);
}

/// After a bugfix run completes, the user often submits another feature from the same web or CLI
/// session folder. `restart_workflow` must pass that session directory into `run_workflow` so the
/// second run does not silently allocate a new UUID directory under `TDDY_SESSIONS_DIR` (which makes
/// the original path look abandoned and resembles a failed start).
#[test]
#[serial]
fn bugfix_second_run_reuses_presenter_session_dir_after_workflow_complete() {
    let sessions_base = std::env::temp_dir().join("tddy-presenter-bugfix-reuse-fixed-session-dir");
    let _ = std::fs::remove_dir_all(&sessions_base);
    std::fs::create_dir_all(&sessions_base).expect("sessions base");
    std::env::set_var(TDDY_SESSIONS_DIR_ENV, sessions_base.to_str().unwrap());

    let fixed_sid = "019d38cf-b74f-7d40-93ae-dcc2bf3f6936";
    let session_path = sessions_base.join(SESSIONS_SUBDIR).join(fixed_sid);
    std::fs::create_dir_all(&session_path).expect("pre-created session dir");

    let (mut presenter, mut events) = bugfix_presenter_with_events();
    let backend = create_stub_backend();
    let (output_dir, _) = common::temp_dir_with_git_repo("presenter-bugfix-reuse");

    presenter.start_workflow(
        backend.clone(),
        output_dir,
        Some(session_path.clone()),
        Some("first run SKIP_QUESTIONS".to_string()),
        None,
        None,
        false,
        None,
        None,
        None,
    );

    let mut iterations = 0;
    const MAX: usize = 8000;
    while !presenter.is_done() && iterations < MAX {
        presenter.poll_workflow();
        events.drain();
        iterations += 1;
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        presenter.is_done(),
        "first bugfix run should complete; mode {:?}",
        presenter.state().mode
    );
    events.drain();
    let first_ok_count = events
        .events()
        .iter()
        .filter(|e| matches!(e, TestEvent::WorkflowComplete(Ok(_))))
        .count();
    assert_eq!(first_ok_count, 1, "expected one WorkflowComplete(Ok)");

    presenter.handle_intent(UserIntent::SubmitFeatureInput(
        "second run SKIP_QUESTIONS".to_string(),
    ));

    iterations = 0;
    while !presenter.is_done() && iterations < MAX {
        presenter.poll_workflow();
        events.drain();
        for e in events.events() {
            if let TestEvent::WorkflowComplete(Err(msg)) = e {
                std::env::remove_var(TDDY_SESSIONS_DIR_ENV);
                panic!("second bugfix workflow failed: {}", msg);
            }
        }
        iterations += 1;
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        presenter.is_done(),
        "second bugfix run should complete; mode {:?}",
        presenter.state().mode
    );

    events.drain();
    let last_session_dir: Option<PathBuf> = events.events().iter().rev().find_map(|e| {
        if let TestEvent::WorkflowComplete(Ok(p)) = e {
            p.session_dir.clone()
        } else {
            None
        }
    });

    std::env::remove_var(TDDY_SESSIONS_DIR_ENV);
    assert_eq!(
        last_session_dir,
        Some(session_path.clone()),
        "second workflow must reuse the same session_dir as the first run; got {:?}. \
         Otherwise the folder the user opened (daemon/web session path) never receives the new run.",
        last_session_dir
    );
}

/// Acceptance: SubmitFeatureInput after completion spawns new workflow.
/// Workflow completes -> FeatureInput -> user submits new feature -> new workflow runs to completion.
#[test]
#[serial]
fn submit_feature_input_after_completion_restarts_workflow() {
    let (mut presenter, mut events) = presenter_with_events();
    let backend = create_stub_backend();
    let (output_dir, _) = common::temp_dir_with_git_repo("presenter-restart");

    presenter.handle_intent(UserIntent::SubmitFeatureInput("Build auth".to_string()));
    presenter.start_workflow(
        backend.clone(),
        output_dir.clone(),
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
        events.drain();
        if matches!(presenter.state().mode, AppMode::DocumentReview { .. }) {
            presenter.handle_intent(UserIntent::ApproveSessionDocument);
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
        "first workflow should complete within {} iterations; last mode: {:?}",
        max_iterations,
        presenter.state().mode
    );
    assert!(
        matches!(presenter.state().mode, AppMode::FeatureInput),
        "after first completion, mode should be FeatureInput"
    );

    events.drain();
    let first_complete_count = events
        .events()
        .iter()
        .filter(|e| matches!(e, TestEvent::WorkflowComplete(Ok(_))))
        .count();
    assert_eq!(
        first_complete_count, 1,
        "expected exactly one WorkflowComplete(Ok) so far"
    );

    presenter.handle_intent(UserIntent::SubmitFeatureInput("Build auth 2".to_string()));

    iterations = 0;
    while !presenter.is_done() && iterations < max_iterations {
        presenter.poll_workflow();
        events.drain();
        if matches!(presenter.state().mode, AppMode::DocumentReview { .. }) {
            presenter.handle_intent(UserIntent::ApproveSessionDocument);
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
        "second workflow should complete within {} iterations; last mode: {:?}",
        max_iterations,
        presenter.state().mode
    );

    events.drain();
    let total_complete_count = events
        .events()
        .iter()
        .filter(|e| matches!(e, TestEvent::WorkflowComplete(Ok(_))))
        .count();
    assert_eq!(
        total_complete_count, 2,
        "expected two WorkflowComplete(Ok) events (one per workflow)"
    );
}

/// When output_dir is "." (TUI default), session_dir must be under tddy_data_dir_path (~/.tddy/sessions),
/// not under the resolved current_dir. MDs (PRD.md, progress.md, etc.) go to session_dir.
#[test]
#[serial]
fn session_dir_under_sessions_base_when_output_dir_is_dot() {
    let sessions_base = std::env::temp_dir().join("tddy-session-dir-test-sessions");
    let _ = std::fs::remove_dir_all(&sessions_base);
    std::fs::create_dir_all(&sessions_base).expect("create sessions base");
    let sessions_base_str = sessions_base.to_str().expect("path");
    std::env::set_var(TDDY_SESSIONS_DIR_ENV, sessions_base_str);

    let (repo_dir, _) = common::temp_dir_with_git_repo("session-dir-test");

    let (mut presenter, mut events) = presenter_with_events();
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
        events.drain();
        if matches!(presenter.state().mode, AppMode::DocumentReview { .. }) {
            presenter.handle_intent(UserIntent::ApproveSessionDocument);
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

    events.drain();
    let payload = events
        .events()
        .iter()
        .find_map(|e| match e {
            TestEvent::WorkflowComplete(Ok(p)) => Some(p.clone()),
            _ => None,
        })
        .expect("WorkflowComplete(Ok) with payload");

    let session_dir = payload
        .session_dir
        .as_ref()
        .expect("session_dir must be set in payload");

    let expected_sessions_base = Path::new(sessions_base_str);
    let expected_plan_parent = expected_sessions_base.join(SESSIONS_SUBDIR);
    assert!(
        session_dir.starts_with(&expected_plan_parent),
        "session_dir {:?} must be under {}/sessions/ (tddy_data_dir_path), not under repo {:?}",
        session_dir,
        sessions_base_str,
        repo_dir
    );
    assert!(
        !session_dir.starts_with(&repo_dir),
        "session_dir {:?} must NOT be under repo {:?}",
        session_dir,
        repo_dir
    );

    let _ = std::fs::remove_dir_all(&sessions_base);
    let _ = std::fs::remove_dir_all(repo_dir.parent().unwrap_or(&repo_dir));
}

/// When session_dir is under sessions (output_dir "."), RefinePlan must use repo_path from changeset
/// for output_dir, not session_dir.parent(). AssertingRepoBackend fails if plan working_dir lacks .git.
#[test]
#[serial]
fn session_dir_under_sessions_refine_uses_repo_as_working_dir() {
    let sessions_base = std::env::temp_dir().join("tddy-plan-refine-sessions");
    let _ = std::fs::remove_dir_all(&sessions_base);
    std::fs::create_dir_all(&sessions_base).expect("create sessions base");
    let sessions_base_str = sessions_base.to_str().expect("path");
    std::env::set_var(TDDY_SESSIONS_DIR_ENV, sessions_base_str);

    let (repo_dir, _) = common::temp_dir_with_git_repo("plan-refine-repo");

    let (mut presenter, mut events) = presenter_with_events();
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
        events.drain();
        if matches!(presenter.state().mode, AppMode::DocumentReview { .. }) {
            plan_review_count += 1;
            if plan_review_count == 1 {
                presenter.handle_intent(UserIntent::RefineSessionDocument);
            } else {
                presenter.handle_intent(UserIntent::ApproveSessionDocument);
            }
        } else if matches!(presenter.state().mode, AppMode::Select { .. }) {
            presenter.handle_intent(UserIntent::AnswerSelect(0));
        } else if matches!(presenter.state().mode, AppMode::MultiSelect { .. }) {
            presenter.handle_intent(UserIntent::AnswerMultiSelect(vec![0], None));
        } else if (matches!(presenter.state().mode, AppMode::MarkdownViewer { .. })
            && presenter.state().plan_refinement_pending)
            || matches!(presenter.state().mode, AppMode::TextInput { .. })
        {
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
        "refine must use repo_path for output_dir when session_dir is under sessions; got ErrorRecovery: {:?}",
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
        "expected DocumentReview at least twice (initial + after refine)"
    );
    events.drain();
    let evs = events.events();
    assert!(
        evs.iter()
            .any(|e| matches!(e, TestEvent::WorkflowComplete(Ok(_)))),
        "expected WorkflowComplete(Ok); refine must use repo_path for output_dir when session_dir is under sessions. Events: {:?}",
        evs
    );
}

/// Clarification scenario: StubBackend with CLARIFY → AnswerSelect → assert answers sent.
#[test]
#[serial]
fn clarification_roundtrip_sends_answers() {
    let (mut presenter, mut events) = presenter_with_events();
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
        events.drain();
        if matches!(presenter.state().mode, AppMode::DocumentReview { .. }) {
            presenter.handle_intent(UserIntent::ApproveSessionDocument);
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
    events.drain();
    let evs = events.events();
    assert!(
        evs.iter().any(
            |e| matches!(e, TestEvent::ModeChanged(AppMode::Select { .. }))
                || matches!(e, TestEvent::ModeChanged(AppMode::DocumentReview { .. }))
        ),
        "expected Select mode during clarification: {:?}",
        evs
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
#[serial]
fn inbox_queue_and_dequeue() {
    let (mut presenter, mut events) = presenter_with_events();
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
        events.drain();
        // Handle intents; approve sets Running, so we can queue in same iteration.
        if matches!(presenter.state().mode, AppMode::DocumentReview { .. }) {
            presenter.handle_intent(UserIntent::ApproveSessionDocument);
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
    events.drain();
    let evs = events.events();
    assert!(
        evs.iter()
            .any(|e| matches!(e, TestEvent::InboxChanged(inbox) if inbox.len() == 1)),
        "expected InboxChanged with 1 item: {:?}",
        evs
    );
}

/// Plan approval: After plan completes, DocumentReview mode appears. ApprovePlan proceeds to next step.
#[test]
#[serial]
fn plan_approval_approve_proceeds_to_next_step() {
    let (mut presenter, mut events) = presenter_with_events();
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
        events.drain();
        if matches!(presenter.state().mode, AppMode::DocumentReview { .. }) {
            presenter.handle_intent(UserIntent::ApproveSessionDocument);
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
    events.drain();
    let evs = events.events();
    assert!(
        evs.iter()
            .any(|e| matches!(e, TestEvent::ModeChanged(AppMode::DocumentReview { .. }))),
        "expected DocumentReview mode: {:?}",
        evs
    );
    assert!(
        evs.iter()
            .any(|e| matches!(e, TestEvent::WorkflowComplete(Ok(_)))),
        "expected WorkflowComplete(Ok): {:?}",
        evs
    );
}

/// Plan approval: ViewPlan opens MarkdownViewer, DismissViewer returns to DocumentReview, ApprovePlan proceeds.
#[test]
#[serial]
fn plan_approval_view_then_approve() {
    let (mut presenter, mut events) = presenter_with_events();
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
        events.drain();
        if matches!(presenter.state().mode, AppMode::MarkdownViewer { .. }) {
            if !viewed {
                presenter.handle_intent(UserIntent::DismissViewer);
                viewed = true;
            }
        } else if matches!(presenter.state().mode, AppMode::DocumentReview { .. }) {
            if viewed {
                presenter.handle_intent(UserIntent::ApproveSessionDocument);
            } else {
                presenter.handle_intent(UserIntent::ViewSessionDocument);
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
    events.drain();
    let evs = events.events();
    assert!(
        evs.iter()
            .any(|e| matches!(e, TestEvent::ModeChanged(AppMode::MarkdownViewer { .. }))),
        "expected MarkdownViewer mode: {:?}",
        evs
    );
}

/// PRD (activity pane): Refine from the markdown plan view must not replace the PRD with a
/// full-screen TextInput — refinement is entered via the prompt bar while PRD stays visible.
#[test]
#[serial]
fn plan_view_refinement_submits_without_dismissing_markdown() {
    let (mut presenter, mut events) = presenter_with_events();
    let backend = create_stub_backend();
    let (output_dir, _) = common::temp_dir_with_git_repo("presenter-refine-without-textinput");

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
    let mut triggered_refine = false;
    while !presenter.is_done() && iterations < max_iterations {
        presenter.poll_workflow();
        events.drain();
        if matches!(presenter.state().mode, AppMode::DocumentReview { .. }) {
            presenter.handle_intent(UserIntent::ViewSessionDocument);
        } else if matches!(presenter.state().mode, AppMode::MarkdownViewer { .. })
            && !triggered_refine
        {
            presenter.handle_intent(UserIntent::RefineSessionDocument);
            triggered_refine = true;
            assert!(
                !matches!(presenter.state().mode, AppMode::TextInput { .. }),
                "RefineSessionDocument with PRD visible must not switch to TextInput; got {:?}",
                presenter.state().mode
            );
        } else if matches!(presenter.state().mode, AppMode::Select { .. }) {
            presenter.handle_intent(UserIntent::AnswerSelect(0));
        } else if matches!(presenter.state().mode, AppMode::MultiSelect { .. }) {
            presenter.handle_intent(UserIntent::AnswerMultiSelect(vec![0], None));
        } else if matches!(presenter.state().mode, AppMode::TextInput { .. }) {
            presenter.handle_intent(UserIntent::AnswerText("fallback".to_string()));
        }
        iterations += 1;
        std::thread::sleep(Duration::from_millis(10));
    }

    assert!(
        triggered_refine,
        "expected to reach MarkdownViewer and issue RefineSessionDocument; last mode: {:?}",
        presenter.state().mode
    );
}

/// Plan approval: RefinePlan opens MarkdownViewer + prompt refinement; AnswerText sends feedback; plan re-runs, approval re-appears.
#[test]
#[serial]
fn plan_approval_refine_re_shows_approval() {
    let (mut presenter, mut events) = presenter_with_events();
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
        events.drain();
        if matches!(presenter.state().mode, AppMode::DocumentReview { .. }) {
            plan_review_count += 1;
            if plan_review_count == 1 {
                presenter.handle_intent(UserIntent::RefineSessionDocument);
            } else {
                presenter.handle_intent(UserIntent::ApproveSessionDocument);
            }
        } else if matches!(presenter.state().mode, AppMode::Select { .. }) {
            presenter.handle_intent(UserIntent::AnswerSelect(0));
        } else if matches!(presenter.state().mode, AppMode::MultiSelect { .. }) {
            presenter.handle_intent(UserIntent::AnswerMultiSelect(vec![0], None));
        } else if (matches!(presenter.state().mode, AppMode::MarkdownViewer { .. })
            && presenter.state().plan_refinement_pending)
            || matches!(presenter.state().mode, AppMode::TextInput { .. })
        {
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
        "expected DocumentReview at least twice (initial + after refine)"
    );
}

/// Plan approval from viewer: ViewPlan → ApprovePlan directly in MarkdownViewer (no DismissViewer) → workflow completes.
/// Asserts presenter is_done() and DocumentReview appears exactly once (no return to DocumentReview after viewer approval).
#[test]
#[serial]
fn plan_approval_from_markdown_viewer() {
    let (mut presenter, mut events) = presenter_with_events();
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
        events.drain();
        if matches!(presenter.state().mode, AppMode::DocumentReview { .. }) && !approved_from_viewer
        {
            presenter.handle_intent(UserIntent::ViewSessionDocument);
        } else if matches!(presenter.state().mode, AppMode::MarkdownViewer { .. })
            && !approved_from_viewer
        {
            presenter.handle_intent(UserIntent::ApproveSessionDocument);
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

    events.drain();
    let evs = events.events();
    let plan_review_count = evs
        .iter()
        .filter(|e| matches!(e, TestEvent::ModeChanged(AppMode::DocumentReview { .. })))
        .count();
    assert_eq!(
        plan_review_count, 1,
        "DocumentReview should appear exactly once when approving directly from viewer: {:?}",
        evs
    );
}

/// Error scenario: StubBackend with FAIL_INVOKE → assert WorkflowComplete(Err).
#[test]
#[serial]
fn workflow_error_propagates() {
    let (mut presenter, mut events) = presenter_with_events();
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
        events.drain();
        iterations += 1;
        std::thread::sleep(Duration::from_millis(10));
    }

    events.drain();
    let evs = events.events();
    assert!(
        evs.iter()
            .any(|e| matches!(e, TestEvent::WorkflowComplete(Err(_)))),
        "expected WorkflowComplete(Err) for FAIL_INVOKE: {:?}",
        evs
    );

    let activity_texts: Vec<&str> = evs
        .iter()
        .filter_map(|e| {
            if let TestEvent::ActivityLogged(entry) = e {
                Some(entry.text.as_str())
            } else {
                None
            }
        })
        .collect();
    assert!(
        activity_texts
            .iter()
            .any(|t| t.to_lowercase().contains("fail") || t.to_lowercase().contains("error")),
        "expected activity log to contain an entry indicating the workflow failure, got: {:?}",
        activity_texts
    );
}
