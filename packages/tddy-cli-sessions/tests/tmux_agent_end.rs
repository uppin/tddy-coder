//! A tmux-run agent's end is announced when the agent ends, not when its tmux client does
//! (`#activity-wake` 1/3).
//!
//! A private tmux server per test (`TMUX_TMPDIR`), with the agent a `sleep` in a tmux session and a
//! main attached to it as its client. Both tests set the process-wide `TMUX_TMPDIR`, so they hold
//! one lock for their whole run.
//!
//! Feature: docs/ft/daemon/1-WIP/PRD-2026-11-16-session-ended-notice.md

use std::path::Path;
use std::process::{Command, Output};

use std::time::Duration;

use tddy_cli_sessions::cli_session_manager::{AgentEnd, CliSessionManager};
use tddy_testing_commons::wait::eventually;
use tokio::sync::broadcast;

const SESSION: &str = "019fb0c1-tmux-agent-end";
const TMUX_SESSION: &str = "agent";

/// A safety net for tmux and the watcher, not a prediction of how long they take.
const OBSERVE: Duration = Duration::from_secs(10);

/// How often the detach watcher checks for the tmux session — short, so the test is quick.
const WATCH_INTERVAL: Duration = Duration::from_millis(50);

/// Longer than several watcher intervals: had the watcher seen an end, it would have announced it.
const HELD: Duration = Duration::from_millis(500);

/// `TMUX_TMPDIR` is process-wide.
static TMUX_ENV: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

struct PrivateTmux {
    socket_dir: tempfile::TempDir,
}

impl PrivateTmux {
    /// A private tmux server running `sleep 300` in [`TMUX_SESSION`] — the agent.
    fn with_a_running_agent() -> Self {
        let tmux = Self {
            socket_dir: tempfile::tempdir().unwrap(),
        };
        std::env::set_var("TMUX_TMPDIR", tmux.socket_dir.path());
        std::env::remove_var("TMUX");
        assert!(tmux
            .run(&["new-session", "-d", "-s", TMUX_SESSION, "sleep", "300"])
            .status
            .success());
        tmux
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new("tmux")
            .env("TMUX_TMPDIR", self.socket_dir.path())
            .args(args)
            .output()
            .expect("tmux runs")
    }

    async fn has_a_client(&self) {
        eventually("the main to attach as a client", OBSERVE, || {
            (!self
                .run(&["list-clients", "-t", TMUX_SESSION])
                .stdout
                .is_empty())
            .then_some(())
            .ok_or_else(|| "no client attached yet".to_string())
        })
        .await;
    }
}

impl Drop for PrivateTmux {
    fn drop(&mut self) {
        let _ = self.run(&["kill-server"]);
    }
}

/// A manager whose detach watcher checks every [`WATCH_INTERVAL`], with `SESSION`'s main attached
/// to the agent's tmux session.
async fn a_main_attached_to(tmux: &PrivateTmux, worktree: &Path) -> CliSessionManager {
    let manager = CliSessionManager::new().with_detach_watch_interval(WATCH_INTERVAL);
    let user = std::env::var("USER").expect("USER names the OS user running the test");
    manager
        .attach_main_to_tmux(
            SESSION,
            worktree.to_path_buf(),
            &user,
            "agent-cli",
            TMUX_SESSION,
        )
        .await
        .expect("the main attaches");
    tmux.has_a_client().await;
    manager
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tmux_client_ending_while_its_agent_lives_announces_nothing() {
    let _env = TMUX_ENV.lock().await;
    // Given an agent in tmux and a main attached to it
    let tmux = PrivateTmux::with_a_running_agent();
    let worktree = tempfile::tempdir().unwrap();
    let manager = a_main_attached_to(&tmux, worktree.path()).await;
    let mut ends: broadcast::Receiver<AgentEnd> = manager.agent_ends();

    // When the client detaches and the agent lives on
    assert!(tmux
        .run(&["detach-client", "-s", TMUX_SESSION])
        .status
        .success());

    // Then nothing is announced
    let announced = tokio::time::timeout(HELD, ends.recv()).await;
    assert!(
        announced.is_err(),
        "a detach was announced as an end: {announced:?}"
    );
    assert!(tmux
        .run(&["has-session", "-t", TMUX_SESSION])
        .status
        .success());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_detached_agent_whose_tmux_session_goes_announces_its_end_as_unknown() {
    let _env = TMUX_ENV.lock().await;
    // Given an agent in tmux whose client has detached
    let tmux = PrivateTmux::with_a_running_agent();
    let worktree = tempfile::tempdir().unwrap();
    let manager = a_main_attached_to(&tmux, worktree.path()).await;
    let mut ends = manager.agent_ends();
    assert!(tmux
        .run(&["detach-client", "-s", TMUX_SESSION])
        .status
        .success());

    // When the agent's tmux session goes
    assert!(tmux
        .run(&["kill-session", "-t", TMUX_SESSION])
        .status
        .success());

    // Then the watcher announces its end — with no exit code, since nothing saw the agent exit
    let user = std::env::var("USER").unwrap();
    let end = tokio::time::timeout(OBSERVE, ends.recv())
        .await
        .expect("the detached agent's end was never announced")
        .expect("the announcement channel closed");
    assert_eq!(
        end,
        AgentEnd {
            session_id: SESSION.to_string(),
            os_user: Some(user),
            outcome: "exit code unknown".to_string(),
            exit_code: None,
        }
    );
}
