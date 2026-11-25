//! An agent session's clipboard: a program in tddy's tmux session copies to the viewer's
//! clipboard — a plain OSC 52 write reaches the attached client, a tmux-wrapped (DCS) one passes
//! through unwrapped, and a copy-mode copy (what a mouse selection becomes) is emitted to the
//! client. Starting the session enables the forwarding on it; an explicitly disabled server
//! clipboard is left alone.
//!
//! A private tmux server per test (`TMUX_TMPDIR`), as the current OS user. The tests set the
//! process-wide `TMUX_TMPDIR`, so they hold one lock for their whole run.

use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;

use tddy_cli_sessions::cli_session_manager::CliSessionManager;
use tddy_cli_sessions::tmux::start_agent_tmux_session;
use tddy_testing_commons::wait::eventually;
use tokio::sync::broadcast;

/// How long an emission may take to reach the client: a safety net, not a prediction.
const OBSERVE: Duration = Duration::from_secs(15);

const SESSION_ID: &str = "clipboard-session";
const TMUX_SESSION: &str = "tddy-clipboard";

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

    fn run(&self, args: &[&str]) -> Output {
        Command::new("tmux")
            .env("TMUX_TMPDIR", self.socket_dir.path())
            .env_remove("TMUX")
            .args(args)
            .output()
            .expect("tmux runs")
    }

    /// What `name`'s pane shows.
    fn pane_text(&self, name: &str) -> String {
        let target = format!("={name}:");
        String::from_utf8_lossy(&self.run(&["capture-pane", "-p", "-t", &target]).stdout)
            .into_owned()
    }

    /// Runs `command` in `name`'s shell, once its prompt is up.
    fn send_command(&self, name: &str, command: &str) {
        let target = format!("={name}:");
        assert!(
            self.run(&["send-keys", "-t", &target, "-l", command])
                .status
                .success(),
            "send-keys types the command"
        );
        assert!(
            self.run(&["send-keys", "-t", &target, "Enter"])
                .status
                .success(),
            "send-keys runs it"
        );
    }

    async fn has_a_client_in(&self, name: &str) {
        eventually("the client to attach", OBSERVE, || {
            (!self.run(&["list-clients", "-t", name]).stdout.is_empty())
                .then_some(())
                .ok_or_else(|| "no client attached yet".to_string())
        })
        .await;
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

/// A shell running in `name` as an agent session would: what [`start_agent_tmux_session`] starts.
fn an_agent_session_in_tmux(worktree: &Path, session_dir: &Path, name: &str) {
    start_agent_tmux_session(
        &this_os_user(),
        name,
        worktree,
        session_dir,
        &[],
        &["/bin/sh".to_string()],
        false,
    )
    .expect("the agent session starts");
}

/// A client attached to `name`, and everything it has shown since the attach.
struct AttachedClient {
    _manager: CliSessionManager,
    seen: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
}

async fn a_client_attached_to(worktree: &Path, name: &str) -> AttachedClient {
    let manager = CliSessionManager::new();
    let user = this_os_user();
    let handle = manager
        .attach_main_to_tmux(SESSION_ID, worktree.to_path_buf(), &user, "agent-cli", name)
        .await
        .expect("the client attaches");
    // Drained from the attach on: the broadcast ring is bounded, and an emission read only after
    // the pane acted may already have been evicted by the attach redraw.
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut output = handle.stdout_tx.subscribe();
    tokio::spawn({
        let seen = std::sync::Arc::clone(&seen);
        async move {
            loop {
                match output.recv().await {
                    Ok(chunk) => seen.lock().unwrap().extend_from_slice(&chunk),
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    });
    AttachedClient {
        _manager: manager,
        seen,
    }
}

/// Passes once `needle` is among what the client has shown; fails naming `what` never arrived.
async fn output_reaches_the_client_within(client: &AttachedClient, needle: &[u8], what: &str) {
    let needle = needle.to_vec();
    eventually(what, OBSERVE, || {
        let seen = client.seen.lock().unwrap();
        seen.windows(needle.len())
            .any(|w| w == needle.as_slice())
            .then_some(())
            .ok_or_else(|| format!("no {needle:?} in {} bytes yet", seen.len()))
    })
    .await;
}

#[tokio::test]
async fn a_plain_osc52_write_from_a_program_reaches_the_attached_client() {
    let _env = TMUX_ENV.lock().await;
    // Given an agent session with a client attached to it
    let tmux = PrivateTmux::started();
    let worktree = tempfile::tempdir().unwrap();
    let session_dir = tempfile::tempdir().unwrap();
    an_agent_session_in_tmux(worktree.path(), session_dir.path(), TMUX_SESSION);
    let client = a_client_attached_to(worktree.path(), TMUX_SESSION).await;
    tmux.has_a_client_in(TMUX_SESSION).await;

    // When a program prints a plain OSC 52 write (`copied-by-a-program`)
    tmux.send_command(
        TMUX_SESSION,
        r"printf '\033]52;c;Y29waWVkLWJ5LWEtcHJvZ3JhbQ==\a'",
    );

    // Then the client sees the write
    output_reaches_the_client_within(
        &client,
        b"\x1b]52;c;Y29waWVkLWJ5LWEtcHJvZ3JhbQ==",
        "the plain OSC 52 write",
    )
    .await;
}

#[tokio::test]
async fn a_tmux_wrapped_osc52_write_from_a_program_reaches_the_attached_client() {
    let _env = TMUX_ENV.lock().await;
    // Given an agent session with a client attached to it
    let tmux = PrivateTmux::started();
    let worktree = tempfile::tempdir().unwrap();
    let session_dir = tempfile::tempdir().unwrap();
    an_agent_session_in_tmux(worktree.path(), session_dir.path(), TMUX_SESSION);
    let client = a_client_attached_to(worktree.path(), TMUX_SESSION).await;
    tmux.has_a_client_in(TMUX_SESSION).await;

    // When a tmux-aware program wraps its OSC 52 write (`copied-wrapped`) in a DCS passthrough
    tmux.send_command(
        TMUX_SESSION,
        r"printf '\033Ptmux;\033\033]52;c;Y29waWVkLXdyYXBwZWQ=\a\033\\'",
    );

    // Then the client sees the write, unwrapped
    output_reaches_the_client_within(
        &client,
        b"\x1b]52;c;Y29waWVkLXdyYXBwZWQ=",
        "the tmux-wrapped OSC 52 write",
    )
    .await;
}

#[tokio::test]
async fn a_copy_mode_copy_reaches_the_attached_client() {
    let _env = TMUX_ENV.lock().await;
    // Given an agent session with a client attached to it, and a line on its pane
    let tmux = PrivateTmux::started();
    let worktree = tempfile::tempdir().unwrap();
    let session_dir = tempfile::tempdir().unwrap();
    an_agent_session_in_tmux(worktree.path(), session_dir.path(), TMUX_SESSION);
    let client = a_client_attached_to(worktree.path(), TMUX_SESSION).await;
    tmux.has_a_client_in(TMUX_SESSION).await;
    tmux.send_command(TMUX_SESSION, "echo COPYME-LINE-1");
    eventually("the line to print", OBSERVE, || {
        tmux.pane_text(TMUX_SESSION)
            .contains("COPYME-LINE-1")
            .then_some(())
            .ok_or_else(|| "the line is not on the pane yet".to_string())
    })
    .await;

    // When the line above the fresh prompt is copied in copy-mode (what a mouse selection
    // becomes): copy-mode enters on the prompt's line, one row up is the echoed line
    let target = format!("={TMUX_SESSION}:");
    assert!(tmux.run(&["copy-mode", "-t", &target]).status.success());
    for key in [
        "cursor-up",
        "start-of-line",
        "begin-selection",
        "end-of-line",
        "copy-selection-and-cancel",
    ] {
        assert!(
            tmux.run(&["send-keys", "-X", "-t", &target, key])
                .status
                .success(),
            "copy-mode takes {key}"
        );
    }

    // Then it is the line that was copied
    let copied = tmux.run(&["show-buffer"]);
    assert_eq!(
        String::from_utf8_lossy(&copied.stdout).trim_end(),
        "COPYME-LINE-1"
    );
    // And the client sees tmux's OSC 52 for it. The first 16 base64 characters cover the first
    // 12 bytes, identical whether tmux copied the line with its newline or without.
    output_reaches_the_client_within(&client, b"\x1b]52;;Q09QWU1FLUxJTkUt", "the copy-mode copy")
        .await;
}

#[tokio::test]
async fn starting_an_agent_session_enables_clipboard_forwarding_on_it() {
    let _env = TMUX_ENV.lock().await;
    // Given a tmux server holding nothing but its defaults
    let tmux = PrivateTmux::started();
    let worktree = tempfile::tempdir().unwrap();
    let session_dir = tempfile::tempdir().unwrap();

    // When an agent session starts in it
    an_agent_session_in_tmux(worktree.path(), session_dir.path(), TMUX_SESSION);

    // Then its window passes programs' writes through, and the server forwards plain ones
    let target = format!("={TMUX_SESSION}:");
    let passthrough = tmux.run(&["show-options", "-w", "-t", &target, "allow-passthrough"]);
    assert_eq!(
        String::from_utf8_lossy(&passthrough.stdout),
        "allow-passthrough on\n"
    );
    let clipboard = tmux.run(&["show-options", "-s", "set-clipboard"]);
    assert_eq!(
        String::from_utf8_lossy(&clipboard.stdout),
        "set-clipboard on\n"
    );
}

#[tokio::test]
async fn starting_an_agent_session_leaves_an_explicitly_disabled_server_clipboard_off() {
    let _env = TMUX_ENV.lock().await;
    // Given a tmux server whose clipboard a user hardened off (a dummy session raises the
    // server a server option needs)
    let tmux = PrivateTmux::started();
    assert!(tmux
        .run(&["new-session", "-d", "-s", "dummy", "sleep", "300"])
        .status
        .success());
    assert!(tmux
        .run(&["set-option", "-s", "set-clipboard", "off"])
        .status
        .success());
    let worktree = tempfile::tempdir().unwrap();
    let session_dir = tempfile::tempdir().unwrap();

    // When an agent session starts in it
    an_agent_session_in_tmux(worktree.path(), session_dir.path(), TMUX_SESSION);

    // Then the server's choice stands, and only the session's own window is enabled
    let clipboard = tmux.run(&["show-options", "-s", "set-clipboard"]);
    assert_eq!(
        String::from_utf8_lossy(&clipboard.stdout),
        "set-clipboard off\n"
    );
    let target = format!("={TMUX_SESSION}:");
    let passthrough = tmux.run(&["show-options", "-w", "-t", &target, "allow-passthrough"]);
    assert_eq!(
        String::from_utf8_lossy(&passthrough.stdout),
        "allow-passthrough on\n"
    );
}
