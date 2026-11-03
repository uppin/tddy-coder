//! A tmux-run main's exit is classified by whether its tmux session lives, and a daemon shutdown
//! ends the tmux *client* only (`#tmux-agent` 2/3).

use std::cell::Cell;
use std::process::Command;

use tddy_cli_sessions::cli_session_manager::{classify_main_exit, CliSessionManager, MainExit};
use tddy_cli_sessions::tmux::tmux_attach_argv;
use tddy_testing_commons::wait::eventually;

#[test]
fn a_tmux_run_main_whose_tmux_session_is_alive_ended_as_a_client_only() {
    // Given a probe that finds the session alive
    let probed = Cell::new(None);

    // When the main's process ends
    let exit = classify_main_exit(Some("tddy-1234abcd"), |name| {
        probed.set(Some(name.to_string()));
        true
    });

    // Then only the client ended, and the probe was asked about the recorded session
    assert_eq!(exit, MainExit::ClientOnly);
    assert_eq!(probed.take().as_deref(), Some("tddy-1234abcd"));
}

#[test]
fn a_tmux_run_main_whose_tmux_session_is_gone_ended_with_its_agent() {
    // When the main's process ends and tmux no longer has its session
    let exit = classify_main_exit(Some("tddy-1234abcd"), |_| false);

    // Then the agent ended
    assert_eq!(exit, MainExit::AgentEnded);
}

#[test]
fn a_pty_run_main_ended_with_its_agent_without_asking_tmux() {
    // When a main with no tmux session ends
    let exit = classify_main_exit(None, |_| {
        panic!("a PTY-run main has no tmux session to probe")
    });

    // Then the agent ended
    assert_eq!(exit, MainExit::AgentEnded);
}

#[tokio::test]
async fn kill_all_ends_a_tmux_run_mains_client_but_not_its_tmux_session() {
    // Given a private tmux server with an agent session, and a main attached to it as a client
    let socket_dir = tempfile::tempdir().unwrap();
    std::env::set_var("TMUX_TMPDIR", socket_dir.path());
    std::env::remove_var("TMUX");
    let tmux = |args: &[&str]| {
        Command::new("tmux")
            .env("TMUX_TMPDIR", socket_dir.path())
            .args(args)
            .output()
            .expect("tmux runs")
    };
    assert!(tmux(&["new-session", "-d", "-s", "agent", "sleep", "300"])
        .status
        .success());
    let worktree = tempfile::tempdir().unwrap();
    let manager = CliSessionManager::new();
    let user = std::env::var("USER").expect("USER names the OS user running the test");
    manager
        .start_tmux_terminal(
            "session-1",
            worktree.path().to_path_buf(),
            &user,
            tmux_attach_argv("tmux", "agent"),
            "agent",
        )
        .await
        .expect("the client attaches");
    eventually(
        "the client to attach",
        std::time::Duration::from_secs(10),
        || {
            let clients = tmux(&["list-clients", "-t", "agent"]).stdout.len();
            (clients > 0)
                .then_some(())
                .ok_or_else(|| "no client attached yet".to_string())
        },
    )
    .await;

    // When the daemon shuts down
    manager.kill_all().await;

    // Then the client is gone and the tmux session — the agent — is still running
    assert!(tmux(&["has-session", "-t", "agent"]).status.success());
    eventually(
        "the client to detach",
        std::time::Duration::from_secs(10),
        || {
            let clients = tmux(&["list-clients", "-t", "agent"]).stdout.len();
            (clients == 0)
                .then_some(())
                .ok_or_else(|| "the client is still attached".to_string())
        },
    )
    .await;
    let _ = tmux(&["kill-server"]);
}
