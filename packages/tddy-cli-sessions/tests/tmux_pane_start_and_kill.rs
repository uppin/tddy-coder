//! The tmux helpers a delete and a failed start share (`#tab-hardening` 2/4): reading a pane's
//! process together with when it started, and killing a tmux session where a failure is logged,
//! never returned.
//!
//! A private tmux server per test (`TMUX_TMPDIR`), as the current OS user. The tests set the
//! process-wide `TMUX_TMPDIR`, so they hold one lock for their whole run.

use std::process::{Command, Output};

use tddy_cli_sessions::process_start::process_start;
use tddy_cli_sessions::tmux::{kill_tmux_session_logged, pane_pid, pane_pid_and_start};

const TMUX_SESSION: &str = "agent";

/// `TMUX_TMPDIR` is process-wide.
static TMUX_ENV: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

struct PrivateTmux {
    socket_dir: tempfile::TempDir,
}

impl PrivateTmux {
    /// A private tmux server with no session yet.
    fn started() -> Self {
        let tmux = Self {
            socket_dir: tempfile::tempdir().unwrap(),
        };
        std::env::set_var("TMUX_TMPDIR", tmux.socket_dir.path());
        std::env::remove_var("TMUX");
        std::env::remove_var("TDDY_TMUX_BINARY");
        tmux
    }

    /// [`Self::started`], running `sleep 300` in [`TMUX_SESSION`].
    fn with_a_running_agent() -> Self {
        let tmux = Self::started();
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

    fn has_session(&self, name: &str) -> bool {
        self.run(&["has-session", "-t", name]).status.success()
    }
}

impl Drop for PrivateTmux {
    fn drop(&mut self) {
        let _ = self.run(&["kill-server"]);
        std::env::remove_var("TMUX_TMPDIR");
    }
}

fn this_os_user() -> String {
    std::env::var("USER").expect("USER names the OS user running the test")
}

#[tokio::test]
async fn a_panes_process_is_read_with_when_it_started() {
    let _env = TMUX_ENV.lock().await;
    // Given
    let _tmux = PrivateTmux::with_a_running_agent();
    let user = this_os_user();

    // When
    let (pid, start) = pane_pid_and_start(&user, TMUX_SESSION).expect("the pane is read");

    // Then it is the pane's process, and the start is that process's
    assert_eq!(
        (Some(pid), Some(start)),
        (pane_pid(&user, TMUX_SESSION).ok(), process_start(pid))
    );
}

#[tokio::test]
async fn a_session_that_is_not_there_has_no_pane_to_read() {
    let _env = TMUX_ENV.lock().await;
    // Given
    let _tmux = PrivateTmux::with_a_running_agent();

    // When
    let read = pane_pid_and_start(&this_os_user(), "not-there");

    // Then
    assert!(read.is_err(), "{read:?}");
}

#[tokio::test]
async fn a_logged_kill_ends_the_tmux_session() {
    let _env = TMUX_ENV.lock().await;
    // Given
    let tmux = PrivateTmux::with_a_running_agent();

    // When
    kill_tmux_session_logged(&this_os_user(), TMUX_SESSION, "a start that failed").await;

    // Then
    assert!(!tmux.has_session(TMUX_SESSION));
}

#[tokio::test]
async fn a_logged_kill_of_a_session_that_is_not_there_returns_quietly() {
    let _env = TMUX_ENV.lock().await;
    // Given a tmux with no server running at all
    let tmux = PrivateTmux::started();

    // When
    kill_tmux_session_logged(&this_os_user(), TMUX_SESSION, "a tab that closed").await;

    // Then nothing was started in its place
    assert!(!tmux.has_session(TMUX_SESSION));
}
