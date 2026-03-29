//! Regression: headless `tddy-coder --daemon` (LiveKit path) must pass args.session_id to
//! `Presenter::start_workflow` so the TUI status bar shows the session segment instead of the
//! placeholder em-dash.
//!
//! When connecting from tddy-web, the spawner creates a session_id and passes it via
//! `--session-id`. The daemon's `run_daemon` function must thread that id into
//! `start_workflow(..., args.session_id.clone(), ...)` so `PresenterState::workflow_session_id`
//! is set immediately — not left as `None` until (and unless) a `SessionStarted` event fires.

mod common;

use std::sync::Arc;

use tddy_coder::Presenter;
use tddy_core::{SharedBackend, StubBackend};
use tddy_workflow_recipes::TddRecipe;

/// Extract the `presenter.start_workflow(...)` call block from `run_daemon`.
///
/// `run_daemon` contains exactly one `presenter.start_workflow(` call — the daemon/LiveKit path.
/// Returns the text from `start_workflow(` through the matching `);`.
fn run_daemon_start_workflow_call() -> String {
    let src = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/run.rs"));
    let fn_start = src
        .find("fn run_daemon(args: &Args")
        .expect("run_daemon must exist");
    let fn_body = &src[fn_start..];
    let fn_end = fn_body
        .find("\n/// Print session id and plan dir")
        .expect("expected print_session_info_on_exit doc comment after run_daemon");
    let fn_body = &fn_body[..fn_end];

    let call_offset = fn_body
        .find("start_workflow(")
        .expect("run_daemon must call start_workflow");
    let call_start = &fn_body[call_offset..];

    // Find the matching closing ");". The call spans multiple lines with trailing comma args.
    let mut depth = 0;
    let mut end = 0;
    for (i, c) in call_start.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    end = i + 1;
                    break;
                }
            }
            _ => {}
        }
    }
    assert!(end > 0, "could not find closing paren for start_workflow(");
    call_start[..end].to_string()
}

/// Source-level regression: verify `run_daemon`'s `start_workflow` call passes `args.session_id`.
///
/// Follows the same pattern as `daemon_toolcall_poll_regression.rs` — reads the source text of
/// `run_daemon` and asserts the session_id parameter is not hardcoded `None`.
#[test]
fn run_daemon_passes_session_id_to_start_workflow() {
    let call = run_daemon_start_workflow_call();

    // The start_workflow call in run_daemon must pass `args.session_id` (the spawner-assigned id),
    // not `None`. The 8th positional argument is session_id.
    //
    // start_workflow signature:
    //   (backend, output, session_dir, prompt, conv_out, debug_out, debug, session_id, socket, tool_rx)
    assert!(
        call.contains("args.session_id"),
        "run_daemon's start_workflow call must pass args.session_id (not None) \
         so the TUI status bar shows the session segment when connecting from tddy-web. \
         Without it, PresenterState::workflow_session_id stays None and the status bar \
         renders the placeholder '\u{2014}' instead of e.g. '019d38e2-31fe'.\n\n\
         Actual call:\n{}",
        call
    );
}

/// Presenter-level reproduction: when start_workflow receives a session_id,
/// workflow_session_id is available immediately (from the first frame).
#[test]
fn start_workflow_with_session_id_sets_workflow_session_id_immediately() {
    let (mut presenter, _events) = presenter_with_events();
    let backend = create_stub_backend();
    let (output_dir, _) = common::temp_dir_with_git_repo("daemon-session-id");

    let session_id = "019d38e2-31fe-7071-a58f-ae9a89b97532";

    presenter.start_workflow(
        backend,
        output_dir,
        None,
        None,
        None,
        None,
        false,
        Some(session_id.to_string()),
        None,
        None,
    );

    assert_eq!(
        presenter.state().workflow_session_id.as_deref(),
        Some(session_id),
        "start_workflow must set workflow_session_id immediately so the status bar \
         shows the session segment from the first frame — web clients connecting to \
         a daemon-spawned session should see e.g. '019d38e2-31fe' not '\u{2014}'"
    );
}

fn presenter_with_events() -> (Presenter, tokio::sync::broadcast::Receiver<tddy_core::PresenterEvent>) {
    let (event_tx, event_rx) = tokio::sync::broadcast::channel(256);
    let presenter = Presenter::new("stub", "default", Arc::new(TddRecipe)).with_broadcast(event_tx);
    (presenter, event_rx)
}

fn create_stub_backend() -> SharedBackend {
    SharedBackend::from_arc(Arc::new(StubBackend::new()))
}
