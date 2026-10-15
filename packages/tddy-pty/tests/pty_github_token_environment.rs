//! A PTY agent is started with none of the GitHub token variables the daemon inherited.
//!
//! It sets process-wide variables, so it is the only test in this binary.

use tddy_pty::{Bytes, PtyRegistry, PtyRuntime, PtySpawnSpec};
use tddy_task::TaskRegistry;
use tokio::sync::oneshot;

/// A PTY agent is started with none of the GitHub token variables the daemon inherited: `gh`
/// prefers them over its own login, so one exported where the daemon runs would act as whoever
/// exported it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pty_agent_inherits_none_of_the_daemons_github_tokens() {
    // Given a daemon environment carrying every GitHub token variable gh honours
    for variable in [
        "GH_TOKEN",
        "GITHUB_TOKEN",
        "GH_ENTERPRISE_TOKEN",
        "GITHUB_ENTERPRISE_TOKEN",
    ] {
        std::env::set_var(variable, "from_the_daemons_environment");
    }
    let registry = TaskRegistry::new();
    let pty_registry = PtyRegistry::new();
    let spec = PtySpawnSpec {
        argv: vec![
            "/bin/sh".to_string(),
            "-c".to_string(),
            "read _go; env; echo tddy-env-printed".to_string(),
        ],
        worktree_path: std::env::temp_dir(),
        session_id: "github-token-session".to_string(),
        terminal_id: "github-token-term".to_string(),
        kind: "sh".to_string(),
        env: Vec::new(),
    };
    let (ready_tx, ready_rx) = oneshot::channel();

    // When an agent that prints its environment is spawned in a PTY
    let task = PtyRuntime::spawn(&registry, &pty_registry, spec, ready_tx).await;
    ready_rx.await.expect("ready").expect("spawned");
    let channel = task.channel("0").expect("channel 0");
    let mut rx = channel.subscribe();
    channel
        .stdin_sender()
        .expect("stdin")
        .send(Bytes::from_static(b"go\n"))
        .expect("send stdin");
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut seen = String::new();
    while !seen.contains("tddy-env-printed") {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        assert!(!remaining.is_zero(), "timed out; saw: {seen:?}");
        let bytes = tokio::time::timeout(remaining, rx.recv())
            .await
            .expect("output before the deadline")
            .expect("output channel open");
        seen.push_str(&String::from_utf8_lossy(&bytes));
    }

    // Then it was started with none of them
    assert!(
        !seen.contains("from_the_daemons_environment"),
        "a GitHub token variable reached the agent: {seen}"
    );
    registry.cancel_task(&task.id).await;
}
