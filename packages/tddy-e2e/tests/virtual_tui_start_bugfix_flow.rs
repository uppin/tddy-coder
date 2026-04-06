//! E2E: Virtual TUI + [`tddy_tui_testkit::TuiTestkit`] — submit `/start-bugfix …` and reach bugfix elicitation.
//!
//! Mirrors production: default TDD recipe, then `/start-<cli>` switches recipe via [`Presenter`]
//! recipe resolver (same as `tddy-coder` full TUI).

use std::sync::atomic::Ordering;
use std::time::Duration;

use tddy_e2e::spawn_presenter_with_view_factory;
use tddy_service::start_virtual_tui_session;
use tddy_tui_testkit::TuiTestkit;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn virtual_tui_start_bugfix_slash_reaches_bugfix_questions() {
    let _ = env_logger::builder().is_test(true).try_init();

    let (_presenter_join, factory, shutdown) = spawn_presenter_with_view_factory(None);
    let session = start_virtual_tui_session(&*factory, false).expect("VirtualTui session");
    let tk = TuiTestkit::new(session, 80, 24);

    tk.wait_for_text("feature description", Duration::from_secs(5))
        .await
        .expect("feature input prompt");

    tk.type_text("/start-bugfix Login page crashes on submit")
        .await
        .expect("type start-bugfix line");
    tk.press_enter().await.expect("submit feature line");

    tk.wait_for_text("Question 1", Duration::from_secs(15))
        .await
        .expect("bugfix recipe should elicit after /start-bugfix + remainder");

    let screen = tk.screen_contents().await;
    assert!(
        screen.contains("Question 1"),
        "expected bugfix clarification on screen; got:\n{screen}"
    );

    tk.shutdown();
    shutdown.store(true, Ordering::Relaxed);
}
