//! Regression: headless `tddy-coder --daemon` must poll the tddy-tools Unix relay like the TUI.
//!
//! Submit is acknowledged on the wire immediately; the presenter still receives
//! [`ToolCallRequest::SubmitActivity`](tddy_core::toolcall::ToolCallRequest) for activity log
//! lines. If `poll_tool_calls()` is never called, those log lines are skipped but submit does not
//! block. This test keeps the daemon loop calling `poll_tool_calls()` alongside `poll_workflow()`.

fn run_daemon_source() -> &'static str {
    let src = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/run.rs"));
    let start = src
        .find("fn run_daemon(args: &Args")
        .expect("run_daemon must exist");
    let tail = &src[start..];
    let end = tail
        .find("\n/// Print session id and plan dir")
        .expect("expected print_session_info_on_exit doc comment after run_daemon");
    &tail[..end]
}

#[test]
fn run_daemon_presenter_loop_polls_tool_calls() {
    let body = run_daemon_source();
    assert!(
        body.contains("p.poll_tool_calls()"),
        "run_daemon's presenter thread must call Presenter::poll_tool_calls() the same way as \
         run_full_workflow_tui so SubmitActivity notifications are processed for the activity log."
    );
}
