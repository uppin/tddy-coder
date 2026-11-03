//! A tmux-run main's exit hook runs on the agent's exit, with the pane's final screen — and a
//! client ending never reads the pane (`#tmux-agent` 3/3).

use tddy_cli_sessions::cli_session_manager::{exit_hook_capture, MainExit};

#[test]
fn an_agent_exit_hands_the_hook_the_panes_final_screen() {
    // When the agent ended and the pane holds its banner
    let capture = exit_hook_capture(MainExit::AgentEnded, || {
        Ok("resume with: warp --resume tok-123\n".to_string())
    })
    .expect("an agent exit runs the hook");

    // Then the hook's output is that screen
    let output = String::from_utf8_lossy(capture.lock().unwrap().buffered_bytes()).into_owned();
    assert!(output.contains("warp --resume tok-123"), "{output}");
}

#[test]
fn a_client_exit_never_reads_the_pane() {
    // When only the tmux client ended
    let capture = exit_hook_capture(MainExit::ClientOnly, || {
        panic!("a client exit must not capture the pane")
    });

    // Then there is no hook input
    assert!(capture.is_none());
}
