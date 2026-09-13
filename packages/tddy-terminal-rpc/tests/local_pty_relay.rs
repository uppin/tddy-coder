//! Smoke test for the local PTY relay: a fast-exiting command runs to completion and the relay
//! returns once the child exits. (Full stdio assertion is covered by `tddy-pty`'s own pump tests;
//! here we only assert the relay's lifecycle over the shared runtime.)

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runs_a_fast_exiting_command_to_completion() {
    // Given a command that exits immediately with status 0
    let argv = vec![
        "/bin/sh".to_string(),
        "-c".to_string(),
        "exit 0".to_string(),
    ];
    let cwd = std::env::temp_dir();

    // When the local relay runs it
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        tddy_terminal_rpc::local_pty_relay::run(argv, cwd, Vec::new()),
    )
    .await;

    // Then the relay returns Ok once the child has exited
    let result = result.expect("local relay should finish within 30s when stdin is an open pipe");
    assert!(
        result.is_ok(),
        "local relay should return Ok after the child exits, got: {:?}",
        result.err()
    );
}
