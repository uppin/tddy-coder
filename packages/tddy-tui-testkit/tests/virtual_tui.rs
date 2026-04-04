//! Integration tests: [`TuiTestkit`] against in-process VirtualTui and a stub presenter.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use tddy_core::workflow::recipe::WorkflowRecipe;
use tddy_core::{AnyBackend, Presenter, SharedBackend, StubBackend};
use tddy_service::start_virtual_tui_session;
use tddy_tui_testkit::TuiTestkit;
use tddy_workflow_recipes::{BugfixRecipe, FreePromptingRecipe, TddRecipe};
use tokio::sync::broadcast;

type ViewConnectionFactory = Arc<dyn Fn() -> Option<tddy_core::ViewConnection> + Send + Sync>;

fn spawn_stub_presenter(
    initial_prompt: Option<String>,
) -> (
    thread::JoinHandle<()>,
    ViewConnectionFactory,
    Arc<AtomicBool>,
) {
    spawn_stub_presenter_with_recipe(initial_prompt, Arc::new(TddRecipe))
}

fn spawn_stub_presenter_with_recipe(
    initial_prompt: Option<String>,
    recipe: Arc<dyn WorkflowRecipe>,
) -> (
    thread::JoinHandle<()>,
    ViewConnectionFactory,
    Arc<AtomicBool>,
) {
    let (event_tx, _) = broadcast::channel(256);
    let (intent_tx, intent_rx) = std::sync::mpsc::channel();

    let presenter = Presenter::new("stub", "opus", recipe)
        .with_broadcast(event_tx)
        .with_intent_sender(intent_tx);
    let output_dir =
        std::env::temp_dir().join(format!("tddy-tui-testkit-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&output_dir).unwrap();
    let backend = SharedBackend::from_any(AnyBackend::Stub(StubBackend::new()));
    let mut presenter = presenter.with_worktree_dir(output_dir.clone());
    presenter.start_workflow(
        backend,
        output_dir,
        None,
        initial_prompt,
        None,
        None,
        false,
        None,
        None,
        None,
    );

    let presenter = Arc::new(Mutex::new(presenter));
    let presenter_for_factory = presenter.clone();
    let factory: ViewConnectionFactory = Arc::new(move || {
        presenter_for_factory
            .lock()
            .ok()
            .and_then(|p| p.connect_view())
    });

    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = shutdown.clone();
    let presenter_for_thread = presenter.clone();
    let presenter_handle = thread::spawn(move || {
        for _ in 0..1000 {
            if shutdown_clone.load(Ordering::Relaxed) {
                break;
            }
            while let Ok(intent) = intent_rx.try_recv() {
                if let Ok(mut p) = presenter_for_thread.lock() {
                    p.handle_intent(intent);
                }
            }
            if let Ok(mut p) = presenter_for_thread.lock() {
                p.poll_workflow();
            }
            thread::sleep(Duration::from_millis(10));
        }
    });

    (presenter_handle, factory, shutdown)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tui_testkit_shows_feature_prompt() {
    let _ = env_logger::builder().is_test(true).try_init();
    let (_handle, factory, shutdown) = spawn_stub_presenter(None);
    let session = start_virtual_tui_session(&*factory, false).expect("session");
    let tk = TuiTestkit::new(session, 80, 24);
    tk.wait_for_text("feature description", Duration::from_secs(5))
        .await
        .expect("prompt visible");
    tk.shutdown();
    shutdown.store(true, Ordering::Relaxed);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tui_testkit_type_text_visible() {
    let _ = env_logger::builder().is_test(true).try_init();
    let (_handle, factory, shutdown) = spawn_stub_presenter(None);
    let session = start_virtual_tui_session(&*factory, false).expect("session");
    let tk = TuiTestkit::new(session, 80, 24);
    tk.wait_for_text("feature description", Duration::from_secs(5))
        .await
        .unwrap();
    tk.type_text("hello").await.unwrap();
    tk.wait_for_text("hello", Duration::from_secs(5))
        .await
        .unwrap();
    tk.shutdown();
    shutdown.store(true, Ordering::Relaxed);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tui_testkit_wait_for_text_times_out() {
    let _ = env_logger::builder().is_test(true).try_init();
    let (_handle, factory, shutdown) = spawn_stub_presenter(None);
    let session = start_virtual_tui_session(&*factory, false).expect("session");
    let tk = TuiTestkit::new(session, 80, 24);
    let err = tk
        .wait_for_text("___NOT_ON_SCREEN_ZZZ___", Duration::from_millis(300))
        .await;
    assert!(err.is_err(), "expected timeout");
    tk.shutdown();
    shutdown.store(true, Ordering::Relaxed);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tui_testkit_resize_and_mouse() {
    let _ = env_logger::builder().is_test(true).try_init();
    let (_handle, factory, shutdown) = spawn_stub_presenter(None);
    let session = start_virtual_tui_session(&*factory, true).expect("session");
    let tk = TuiTestkit::new(session, 80, 24);
    tk.wait_for_text("feature description", Duration::from_secs(5))
        .await
        .unwrap();
    tk.resize(100, 30).await.unwrap();
    tk.wait_for_render(Duration::from_secs(2)).await.unwrap();
    let _ = tk.screen_contents().await;
    tk.click(10, 10).await.unwrap();
    tk.scroll_up(5, 5).await.unwrap();
    tk.scroll_down(5, 5).await.unwrap();
    tk.shutdown();
    shutdown.store(true, Ordering::Relaxed);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tui_testkit_screen_line_matches_screen_contents() {
    let _ = env_logger::builder().is_test(true).try_init();
    let (_handle, factory, shutdown) = spawn_stub_presenter(None);
    let session = start_virtual_tui_session(&*factory, false).expect("session");
    let tk = TuiTestkit::new(session, 80, 24);
    tk.wait_for_text("feature description", Duration::from_secs(5))
        .await
        .unwrap();
    let full = tk.screen_contents().await;
    let line0 = tk.screen_line(0).await;
    assert!(
        full.contains(line0.trim_end()),
        "first line should be substring of full screen"
    );
    tk.shutdown();
    shutdown.store(true, Ordering::Relaxed);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tui_testkit_press_enter_sends_input() {
    let _ = env_logger::builder().is_test(true).try_init();
    let (_handle, factory, shutdown) = spawn_stub_presenter(None);
    let session = start_virtual_tui_session(&*factory, false).expect("session");
    let tk = TuiTestkit::new(session, 80, 24);
    tk.wait_for_text("feature description", Duration::from_secs(5))
        .await
        .unwrap();
    tk.press_enter().await.unwrap();
    tk.wait_for_render(Duration::from_secs(2)).await.unwrap();
    tk.shutdown();
    shutdown.store(true, Ordering::Relaxed);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn free_prompting_stub_responds_to_first_prompt() {
    let _ = env_logger::builder().is_test(true).try_init();
    let (_handle, factory, shutdown) =
        spawn_stub_presenter_with_recipe(None, Arc::new(FreePromptingRecipe));
    let session = start_virtual_tui_session(&*factory, false).expect("session");
    let tk = TuiTestkit::new(session, 120, 30);

    tk.wait_for_text("feature description", Duration::from_secs(5))
        .await
        .expect("prompt visible");

    tk.type_text("Hello world").await.unwrap();
    tk.press_enter().await.unwrap();

    tk.wait_for_text("[Stub]", Duration::from_secs(10))
        .await
        .expect("stub response should appear on screen after first prompt");

    let screen = tk.screen_contents().await;
    log::info!("Screen after 1st prompt:\n{}", screen);

    tk.shutdown();
    shutdown.store(true, Ordering::Relaxed);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn free_prompting_second_prompt_produces_new_response() {
    let _ = env_logger::builder().is_test(true).try_init();
    let (_handle, factory, shutdown) =
        spawn_stub_presenter_with_recipe(None, Arc::new(FreePromptingRecipe));
    let session = start_virtual_tui_session(&*factory, false).expect("session");
    let tk = TuiTestkit::new(session, 120, 30);

    tk.wait_for_text("feature description", Duration::from_secs(5))
        .await
        .expect("prompt visible");

    tk.type_text("First question").await.unwrap();
    tk.press_enter().await.unwrap();

    tk.wait_for_text("[Stub]", Duration::from_secs(10))
        .await
        .expect("stub response should appear after first prompt");

    let screen_after_first = tk.screen_contents().await;
    log::info!("Screen after 1st prompt:\n{}", screen_after_first);
    let stub_count_after_first = screen_after_first.matches("[Stub]").count();
    assert_eq!(
        stub_count_after_first, 1,
        "exactly one [Stub] response after first prompt, screen:\n{}",
        screen_after_first
    );

    assert!(
        !screen_after_first.contains("Ready"),
        "workflow should not be in Ready state after first prompt (should be waiting for next input), screen:\n{}",
        screen_after_first
    );

    tk.type_text("Second question").await.unwrap();
    tk.press_enter().await.unwrap();

    tokio::time::sleep(Duration::from_secs(2)).await;
    tk.wait_for_render(Duration::from_secs(5)).await.unwrap();
    let screen_after_second = tk.screen_contents().await;
    log::info!("Screen after 2nd prompt:\n{}", screen_after_second);

    let stub_count_after_second = screen_after_second.matches("[Stub]").count();
    assert!(
        stub_count_after_second >= 2,
        "expected at least 2 [Stub] responses after two prompts (got {}), screen:\n{}",
        stub_count_after_second,
        screen_after_second
    );

    tk.shutdown();
    shutdown.store(true, Ordering::Relaxed);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bugfix_demo_asks_two_questions_after_first_prompt() {
    let _ = env_logger::builder().is_test(true).try_init();
    let (_handle, factory, shutdown) =
        spawn_stub_presenter_with_recipe(None, Arc::new(BugfixRecipe));
    let session = start_virtual_tui_session(&*factory, false).expect("session");
    let tk = TuiTestkit::new(session, 120, 30);

    tk.wait_for_text("feature description", Duration::from_secs(5))
        .await
        .expect("prompt visible");

    tk.type_text("Login page crashes on submit").await.unwrap();
    tk.press_enter().await.unwrap();

    tk.wait_for_text("Question 1", Duration::from_secs(10))
        .await
        .expect("first question should appear after submitting bug description");

    let screen_q1 = tk.screen_contents().await;
    log::info!("Screen at Q1:\n{}", screen_q1);
    assert!(
        !screen_q1.contains("Ready"),
        "workflow should not show Ready while asking questions, screen:\n{}",
        screen_q1
    );

    tk.press_enter().await.unwrap();

    tk.wait_for_text("Question 2", Duration::from_secs(5))
        .await
        .expect("second question should appear after answering the first");

    let screen_q2 = tk.screen_contents().await;
    log::info!("Screen at Q2:\n{}", screen_q2);

    tk.press_enter().await.unwrap();

    tk.wait_for_text("[Stub]", Duration::from_secs(10))
        .await
        .expect("stub response should appear after answering both questions");

    let screen_after = tk.screen_contents().await;
    log::info!("Screen after answers:\n{}", screen_after);
    assert!(
        screen_after.contains("[Stub]"),
        "expected [Stub] response on screen after answering questions, screen:\n{}",
        screen_after
    );

    tk.shutdown();
    shutdown.store(true, Ordering::Relaxed);
}

/// Full TDD recipe: interview clarification → plan clarification → plan approval menu (stub).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tdd_full_workflow_interview_through_plan_approval() {
    let _ = env_logger::builder().is_test(true).try_init();
    let (_handle, factory, shutdown) = spawn_stub_presenter(None);
    let session = start_virtual_tui_session(&*factory, false).expect("session");
    let tk = TuiTestkit::new(session, 120, 30);

    tk.wait_for_text("feature description", Duration::from_secs(5))
        .await
        .expect("prompt visible");

    tk.type_text("Add user profile API with validation")
        .await
        .unwrap();
    tk.press_enter().await.unwrap();

    tk.wait_for_text("Feature scope", Duration::from_secs(10))
        .await
        .expect("first interview question should appear");
    tk.press_enter().await.unwrap();

    tk.wait_for_text("Constraints", Duration::from_secs(10))
        .await
        .expect("second interview question should appear");
    tk.press_enter().await.unwrap();

    // Interview handoff is merged into plan context as answers, so the first plan invoke skips
    // stub plan clarification and submits the PRD — user goes straight to plan approval.
    tk.wait_for_text("Plan generated", Duration::from_secs(30))
        .await
        .expect("plan approval menu should appear after interview completes and plan runs");

    let screen = tk.screen_contents().await;
    log::info!("Screen at plan approval:\n{}", screen);
    assert!(
        screen.contains("View") && screen.contains("Approve") && screen.contains("Refine"),
        "expected plan approval options on screen, got:\n{}",
        screen
    );

    tk.shutdown();
    shutdown.store(true, Ordering::Relaxed);
}
