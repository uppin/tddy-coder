//! A PTY child starts from what its spec's `env_base` says (`#agent-tools 28/28`): an agent's from
//! nothing, a user's terminal from the spawning process's environment.
//!
//! It sets a process-wide variable, which every test here plants with the same value.

use tddy_pty::{Bytes, PtyEnvBase, PtyRegistry, PtyRuntime, PtySpawnSpec};
use tddy_task::TaskRegistry;
use tokio::sync::oneshot;

const OUTPUT_DEADLINE: std::time::Duration = std::time::Duration::from_secs(10);
const DAEMON_ONLY: &str = "PTY_ENV_BASE_DAEMON_ONLY";

fn a_daemon_holding_a_variable_of_its_own() {
    std::env::set_var(DAEMON_ONLY, "from-the-daemon");
}

/// The environment a PTY child started on `env_base` with `env` prints.
async fn environment_printed_by_a_child_on(
    env_base: PtyEnvBase,
    env: Vec<(String, String)>,
) -> String {
    let registry = TaskRegistry::new();
    let pty_registry = PtyRegistry::new();
    let spec = PtySpawnSpec {
        argv: vec![
            "/bin/sh".to_string(),
            "-c".to_string(),
            "read _go; /usr/bin/env; echo tddy-env-printed".to_string(),
        ],
        worktree_path: std::env::temp_dir(),
        session_id: "env-base-session".to_string(),
        terminal_id: "env-base-term".to_string(),
        kind: "sh".to_string(),
        env,
        env_base,
    };
    let (ready_tx, ready_rx) = oneshot::channel();
    let task = PtyRuntime::spawn(&registry, &pty_registry, spec, ready_tx).await;
    ready_rx.await.expect("ready").expect("spawned");
    let channel = task.channel("0").expect("channel 0");
    let mut rx = channel.subscribe();
    channel
        .stdin_sender()
        .expect("stdin")
        .send(Bytes::from_static(b"go\n"))
        .expect("send stdin");
    let deadline = tokio::time::Instant::now() + OUTPUT_DEADLINE;
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
    registry.cancel_task(&task.id).await;
    seen
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_child_on_a_cleared_base_sees_only_its_spec_env_and_the_terminal_variables() {
    // Given
    a_daemon_holding_a_variable_of_its_own();

    // When
    let printed = environment_printed_by_a_child_on(
        PtyEnvBase::Cleared,
        vec![("PATH".to_string(), "/usr/bin:/bin".to_string())],
    )
    .await;

    // Then
    assert!(
        !printed.contains(DAEMON_ONLY),
        "a daemon variable reached a cleared child: {printed}"
    );
    assert!(printed.contains("PATH=/usr/bin:/bin"), "{printed}");
    assert!(printed.contains("TERM=xterm-256color"), "{printed}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_child_on_a_cleared_base_is_told_the_terminal_it_runs_in_whatever_its_env_says() {
    // Given — an agent env carrying the daemon's own terminal description
    let env = vec![
        ("PATH".to_string(), "/usr/bin:/bin".to_string()),
        ("TERM".to_string(), "xterm-ghostty".to_string()),
        ("COLORTERM".to_string(), "24bit".to_string()),
    ];

    // When
    let printed = environment_printed_by_a_child_on(PtyEnvBase::Cleared, env).await;

    // Then — the PTY's own terminal stands
    assert!(printed.contains("TERM=xterm-256color"), "{printed}");
    assert!(printed.contains("COLORTERM=truecolor"), "{printed}");
    assert!(!printed.contains("xterm-ghostty"), "{printed}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_child_on_an_inherited_base_still_sees_the_daemons_variables() {
    // Given
    a_daemon_holding_a_variable_of_its_own();

    // When
    let printed = environment_printed_by_a_child_on(PtyEnvBase::Inherited, Vec::new()).await;

    // Then — guard: a user's terminal keeps the environment it always had
    assert!(
        printed.contains("PTY_ENV_BASE_DAEMON_ONLY=from-the-daemon"),
        "{printed}"
    );
}
