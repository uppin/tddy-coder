//! Contract: newly spawned sessions must receive a UUID v7 identifier (time-ordered, sortable).
//! The daemon spawner must use `Uuid::now_v7()`, not `Uuid::new_v4()`.

#[test]
fn spawner_generates_uuid_v7_session_id() {
    let spawner_rs = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/spawner.rs"));

    assert!(
        !spawner_rs.contains("Uuid::new_v4"),
        "spawner must not use Uuid::new_v4 for session ID generation — use Uuid::now_v7 instead"
    );
    assert!(
        spawner_rs.contains("Uuid::now_v7"),
        "spawner must use Uuid::now_v7() for new session IDs (time-ordered, sortable)"
    );
}

/// The `run_daemon` code path in tddy-coder must pass `args.session_id` to the presenter's
/// `start_workflow` call so the TUI status bar can display the session segment immediately.
/// Currently the 8th argument (session_id) is hardcoded to `None`.
#[test]
fn daemon_passes_session_id_to_presenter_start_workflow() {
    let run_rs = include_str!("../../tddy-coder/src/run.rs");

    let in_daemon_fn = run_rs
        .find("fn run_daemon(")
        .expect("run_daemon function must exist in run.rs");
    let daemon_body = &run_rs[in_daemon_fn..];

    let start_workflow_call = daemon_body
        .find("presenter.start_workflow(")
        .expect("run_daemon must call presenter.start_workflow");
    let call_region = &daemon_body[start_workflow_call..];
    let closing_paren = call_region
        .find(");")
        .expect("start_workflow call must have closing );");
    let call_text = &call_region[..closing_paren];

    assert!(
        call_text.contains("args.session_id"),
        "run_daemon must pass args.session_id to presenter.start_workflow \
         so the TUI status bar shows the session segment; found call:\n{}",
        call_text
    );
}
