//! Regression: headless `tddy-coder --daemon` must poll the tddy-tools Unix relay like the TUI.
//!
//! When the presenter loop only calls `poll_workflow()` and never `poll_tool_calls()`,
//! [`ToolCallRequest::Submit`](tddy_core::toolcall::ToolCallRequest) from the relay is never
//! answered, so `tddy-tools submit` blocks on the socket
//! (`listener.rs` logs `[wait] waiting for presenter response...` with no matching `[send]`).

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
         run_full_workflow_tui. Otherwise the tddy-tools relay never completes Submit and the \
         agent hangs after calling tddy-tools submit (e.g. acceptance-tests)."
    );
}
