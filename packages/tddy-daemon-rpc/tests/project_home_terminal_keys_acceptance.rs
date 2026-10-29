//! Acceptance: a project-home session takes control of another session's terminal and types into it,
//! for as long as it holds control and not a moment longer (`#home-inbox` 5/8).
//!
//! A session's TUI is stood in for by a shell script that puts its terminal in raw mode, announces
//! itself and then records every byte it is typed, byte for byte, into a file — so a test reads
//! exactly what reached the child's PTY. A jailed child has no PTY the daemon owns: its terminal is a
//! registered jail whose stdin is a channel the test reads.
//!
//! The browser is the existing lease holder: a person watching a session holds its control lease
//! under a screen id of their own and takes it back by stealing it.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools, docs/ft/daemon/terminal-sessions.md.
//! Changeset: docs/dev/changesets/2026-10-28-home-inbox-terminal-keys.md

mod support;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use support::{a_host, a_workspace_in, Host, InProcessPeers, HOST_A, HOST_B, THIS_PROJECT};
use tddy_rpc::Request;
use tddy_service::proto::exec_tools::{ExecToolService, ExecuteToolRequest};
use tddy_session_lifecycle::sandbox_session::{SandboxSessionState, SandboxSessionStateInit};
use tddy_session_lifecycle::test_util::TEST_TOKEN;
use tddy_task::TerminalCapture;
use tokio::sync::{broadcast, mpsc};

/// How long a stand-in child is given to start, and a typed byte to reach it.
const WAIT_TIMEOUT: Duration = Duration::from_secs(15);
/// How long a test waits for something that must NOT happen to show up.
const QUIET_PERIOD: Duration = Duration::from_millis(300);
/// What the stand-in TUI prints once its terminal is raw and it is recording.
const READY_MARKER: &str = "READY";
/// The screen a person watching the session in the browser holds control under.
const BROWSER_SCREEN: &str = "browser-screen-1";
/// The bytes of `C-a` and `C-c`.
const CTRL_A: u8 = 0x01;
const CTRL_C: u8 = 0x03;

struct World {
    a: Host,
    b: Host,
    home: String,
    other_home: String,
    on_a: String,
    on_b: String,
}

async fn a_world() -> World {
    let peers = Arc::new(InProcessPeers::default());
    let a = a_host(HOST_A, &peers);
    let b = a_host(HOST_B, &peers);
    let home = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    a.make_project_home(&home);
    let other_home = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    a.make_project_home(&other_home);
    let on_a = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    let on_b = a_workspace_in(&b.daemon, THIS_PROJECT).await;
    World {
        a,
        b,
        home,
        other_home,
        on_a,
        on_b,
    }
}

impl World {
    /// The screen id the home session holds control under.
    fn home_screen(&self) -> String {
        format!("project-home:{}", self.home)
    }
}

// ── the stand-in TUI, which records what it is typed ─────────────────────────────────────────

/// A running stand-in TUI on some host, and the file holding every byte typed into it.
struct RecordingTui {
    _dir: tempfile::TempDir,
    record: PathBuf,
}

/// Start a raw-mode TUI as the main terminal of `session_id` on `host`, and wait until it is ready
/// to be typed into.
async fn start_recording_terminal(host: &Host, session_id: &str) -> RecordingTui {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let record = dir.path().join("typed.bin");
    let script = dir.path().join("stand_in_tui.sh");
    // Raw, so no line discipline rewrites a byte; echo on, so the screen shows what was typed.
    let script_text = format!(
        "#!/bin/sh\nstty raw echo -echoctl\nprintf '{READY_MARKER}\\r\\n'\nexec cat > '{}'\n",
        record.display()
    );
    std::fs::write(&script, script_text).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let handle = host
        .terminals
        .start(
            session_id,
            host.worktree_of(session_id),
            "claude-opus-4-8",
            script.to_str().unwrap(),
            None,
            None,
        )
        .await
        .expect("the main terminal starts");
    let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
    loop {
        let ready = handle
            .capture
            .lock()
            .unwrap()
            .buffered_bytes()
            .windows(READY_MARKER.len())
            .any(|window| window == READY_MARKER.as_bytes());
        if ready {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the stand-in TUI never became ready"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    RecordingTui { _dir: dir, record }
}

impl RecordingTui {
    fn typed_so_far(&self) -> Vec<u8> {
        std::fs::read(&self.record).unwrap_or_default()
    }

    /// Wait until the child has been typed `expected`, and return what it was typed.
    async fn typed_after_waiting_for(&self, expected: &[u8]) -> Vec<u8> {
        let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
        loop {
            let typed = self.typed_so_far();
            if typed.len() >= expected.len() || tokio::time::Instant::now() >= deadline {
                return typed;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

// ── the stand-in jail, whose stdin the test reads ────────────────────────────────────────────

/// A jailed child registered on a host: nothing the daemon owns is a PTY, so what it was typed is
/// what arrives on the jail's stdin channel.
struct JailedChild {
    stdin: mpsc::UnboundedReceiver<Vec<u8>>,
}

async fn a_jailed_child_of(host: &Host, session_id: &str) -> JailedChild {
    let (stdout_tx, _) = broadcast::channel(64);
    let (stdin_tx, mut jail_stdin) = mpsc::unbounded_channel();
    let (typed_tx, stdin) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        while let Some(bytes) = jail_stdin.recv().await {
            let bytes: Vec<u8> = AsRef::<[u8]>::as_ref(&bytes).to_vec();
            if typed_tx.send(bytes).is_err() {
                return;
            }
        }
    });
    let state = Arc::new(SandboxSessionState::new(SandboxSessionStateInit {
        pid: 4242,
        worktree_path: host.worktree_of(session_id),
        stdout_tx,
        capture: Arc::new(Mutex::new(TerminalCapture::new())),
        stdin_tx,
        ready_marker: PathBuf::from("/unused-by-the-terminal-surface"),
        handle: an_exited_sandbox_process(),
        managed_workflow: None,
    }));
    host.daemon
        .connection()
        .sandbox_sessions()
        .insert(session_id.to_string(), state)
        .await;
    JailedChild { stdin }
}

/// A process standing in for the jail: the state keeps the handle only so a delete can kill the
/// tree, and nothing on the terminal surface reads it.
fn an_exited_sandbox_process() -> tddy_sandbox::SandboxHandle {
    let child = std::process::Command::new("true")
        .spawn()
        .expect("a trivial child process");
    let unused = || PathBuf::from("/unused-by-the-terminal-surface");
    tddy_sandbox::SandboxHandle::new(child, unused(), unused(), unused())
}

impl JailedChild {
    /// Everything that has reached the jail's stdin so far.
    async fn typed_so_far(&mut self) -> Vec<u8> {
        tokio::time::sleep(QUIET_PERIOD).await;
        let mut typed = Vec::new();
        while let Ok(bytes) = self.stdin.try_recv() {
            typed.extend(bytes);
        }
        typed
    }
}

// ── the lease, as a person at the browser holds it ───────────────────────────────────────────

impl Host {
    /// A person watching `session_id` in the browser takes control, as the browser's claim does.
    async fn a_browser_takes_control_of(&self, session_id: &str) {
        self.daemon
            .connection()
            .terminal_control()
            .claim(session_id, BROWSER_SCREEN, true)
            .await;
    }

    async fn control_of(&self, session_id: &str) -> Option<String> {
        self.daemon
            .connection()
            .terminal_control()
            .holder_screen_id(session_id)
            .await
    }
}

// ── what a tool call answered ────────────────────────────────────────────────────────────────

/// What came back from a tool call as the agent sees it: an answer, or why there is none. A call
/// the daemon refuses outright is a refusal here too, so a missing tool reads as one.
struct Answer {
    refused: bool,
    text: String,
}

impl Answer {
    fn assert_answered(&self) -> serde_json::Value {
        assert!(
            !self.refused,
            "expected an answer, was refused: {}",
            self.text
        );
        serde_json::from_str(&self.text).expect("the answer is JSON")
    }

    fn assert_refused_saying(&self, reason: &str) -> &Self {
        assert!(self.refused, "expected a refusal, got {}", self.text);
        assert!(
            self.text.contains(reason),
            "refused, but not for {reason:?}: {}",
            self.text
        );
        self
    }

    /// Refused because control is held by `screen`, telling the agent to claim it again.
    fn assert_refused_because_held_by(&self, screen: &str) -> &Self {
        self.assert_refused_saying(screen)
            .assert_refused_saying("claim")
    }
}

impl World {
    async fn call(&self, host: &Host, caller: &str, tool: &str, args: serde_json::Value) -> Answer {
        let result = host
            .daemon
            .execute_tool(Request::direct(ExecuteToolRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: caller.to_string(),
                tool_name: tool.to_string(),
                args_json: args.to_string(),
                daemon_instance_id: String::new(),
                conversation_id: String::new(),
            }))
            .await;
        match result {
            Ok(response) => {
                let response = response.into_inner();
                Answer {
                    refused: response.is_error,
                    text: if response.is_error {
                        response.error_message
                    } else {
                        response.result_json
                    },
                }
            }
            Err(status) => Answer {
                refused: true,
                text: format!("{:?}: {}", status.code(), status.message()),
            },
        }
    }

    /// `ClaimSessionTerminal`, called by `caller` on host-a.
    async fn claims(&self, caller: &str, args: serde_json::Value) -> Answer {
        self.call(&self.a, caller, "ClaimSessionTerminal", args)
            .await
    }

    /// `SendSessionKeys`, called by `caller` on host-a.
    async fn sends(&self, caller: &str, args: serde_json::Value) -> Answer {
        self.call(&self.a, caller, "SendSessionKeys", args).await
    }

    async fn home_claims(&self, session_id: &str) -> Answer {
        self.claims(&self.home, serde_json::json!({ "session_id": session_id }))
            .await
    }

    async fn home_sends_keys(&self, session_id: &str, keys: &[&str]) -> Answer {
        self.sends(
            &self.home,
            serde_json::json!({ "session_id": session_id, "keys": keys }),
        )
        .await
    }
}

/// A `ReadSessionTerminal` answer's text lines, polled until `wanted` is among them: the screen
/// follows the PTY's output a moment after the keystrokes reach it.
async fn screen_lines_eventually_include(world: &World, session_id: &str, wanted: &str) {
    let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
    loop {
        let read = world
            .call(
                &world.a,
                &world.home,
                "ReadSessionTerminal",
                serde_json::json!({ "session_id": session_id }),
            )
            .await;
        let lines = if read.refused {
            Vec::new()
        } else {
            read.assert_answered()["lines"]
                .as_array()
                .expect("a lines array")
                .iter()
                .map(|line| line.as_str().unwrap_or_default().to_string())
                .collect()
        };
        if lines.iter().any(|line| line.starts_with(wanted)) {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the screen never showed {wanted:?}; the read answered {read_text:?} and its lines are {lines:?}",
            read_text = read.text
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

// ── the tests ────────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn after_a_claim_send_session_keys_writes_the_encoded_bytes_and_the_screen_reflects_them() {
    // Given a session whose TUI is ready, and a home session that has taken control of it
    let world = a_world().await;
    let tui = start_recording_terminal(&world.a, &world.on_a).await;
    world.home_claims(&world.on_a).await.assert_answered();

    // When it types a command and presses Enter
    let sent = world
        .sends(
            &world.home,
            serde_json::json!({
                "session_id": world.on_a,
                "text": "echo hello",
                "keys": ["Enter"],
            }),
        )
        .await;

    // Then exactly the encoded bytes reached the child's PTY ...
    sent.assert_answered();
    assert_eq!(
        tui.typed_after_waiting_for(b"echo hello\r").await,
        b"echo hello\r".to_vec(),
        "the bytes the child was typed"
    );
    // ... and the child's screen, read back through ReadSessionTerminal, shows them
    screen_lines_eventually_include(&world, &world.on_a, "echo hello").await;
}

#[tokio::test]
async fn send_session_keys_without_a_claim_is_refused_and_writes_nothing() {
    // Given a session whose TUI is ready, which this home session never claimed
    let world = a_world().await;
    let tui = start_recording_terminal(&world.a, &world.on_a).await;

    // When it tries to type
    let refused = world.home_sends_keys(&world.on_a, &["C-b"]).await;

    // Then it is refused, told to claim the terminal first
    refused.assert_refused_saying("ClaimSessionTerminal");
    // And nothing was written: after a claim, the first byte the child holds is the next one sent
    world.home_claims(&world.on_a).await.assert_answered();
    world
        .home_sends_keys(&world.on_a, &["C-a"])
        .await
        .assert_answered();
    assert_eq!(
        tui.typed_after_waiting_for(&[CTRL_A]).await,
        vec![CTRL_A],
        "the bytes the child was typed"
    );
}

#[tokio::test]
async fn a_browser_steal_after_the_claim_refuses_the_next_send_naming_the_holder_and_a_new_claim_succeeds(
) {
    // Given a home session that claimed a session's terminal and typed into it, and a person who
    // then takes control back in the browser
    let world = a_world().await;
    let tui = start_recording_terminal(&world.a, &world.on_a).await;
    world.home_claims(&world.on_a).await.assert_answered();
    world
        .home_sends_keys(&world.on_a, &["C-a"])
        .await
        .assert_answered();
    world.a.a_browser_takes_control_of(&world.on_a).await;

    // When the home session types again
    let refused = world.home_sends_keys(&world.on_a, &["C-b"]).await;

    // Then it is refused, naming the person's screen
    refused.assert_refused_because_held_by(BROWSER_SCREEN);

    // And after it claims again, its next send goes through
    world.home_claims(&world.on_a).await.assert_answered();
    world
        .home_sends_keys(&world.on_a, &["C-c"])
        .await
        .assert_answered();
    assert_eq!(
        tui.typed_after_waiting_for(&[CTRL_A, CTRL_C]).await,
        vec![CTRL_A, CTRL_C],
        "the bytes the child was typed: the refused C-b is not among them"
    );
}

#[tokio::test]
async fn an_unknown_key_name_is_refused_and_writes_nothing() {
    // Given a home session that holds control of a session's terminal
    let world = a_world().await;
    let tui = start_recording_terminal(&world.a, &world.on_a).await;
    world.home_claims(&world.on_a).await.assert_answered();

    // When it presses Enter and then a key that does not exist
    let refused = world
        .home_sends_keys(&world.on_a, &["Enter", "Hyper"])
        .await;

    // Then it is refused, naming the key
    refused.assert_refused_saying("Hyper");
    // And not even the Enter before it was written
    world
        .home_sends_keys(&world.on_a, &["C-a"])
        .await
        .assert_answered();
    assert_eq!(
        tui.typed_after_waiting_for(&[CTRL_A]).await,
        vec![CTRL_A],
        "the bytes the child was typed"
    );
}

#[tokio::test]
async fn the_claim_broadcasts_a_control_change_naming_the_project_home_screen() {
    // Given a session a person is watching, who holds its control, and a subscriber to lease changes
    let world = a_world().await;
    start_recording_terminal(&world.a, &world.on_a).await;
    world.a.a_browser_takes_control_of(&world.on_a).await;
    let mut changes = world.a.daemon.connection().terminal_control().subscribe();

    // When the home session claims the terminal
    let claimed = world.home_claims(&world.on_a).await.assert_answered();

    // Then the lease change names the home session's screen, as a browser is told, and so does the
    // claim's own answer
    let change = tokio::time::timeout(WAIT_TIMEOUT, changes.recv())
        .await
        .expect("a control change is broadcast")
        .expect("the broadcast is open");
    assert_eq!(
        (change.session_id, change.holder_screen_id),
        (world.on_a.clone(), world.home_screen()),
        "the control change"
    );
    assert_eq!(claimed["holder"], world.home_screen(), "the claim's answer");
}

#[tokio::test]
async fn a_jailed_childs_terminal_follows_the_same_rules() {
    // Given a jailed session, whose terminal does not itself check the control lease, held by a
    // person in the browser
    let world = a_world().await;
    let mut jail = a_jailed_child_of(&world.a, &world.on_a).await;
    world.a.a_browser_takes_control_of(&world.on_a).await;

    // When the home session types without claiming, then claims and types, then a person takes
    // control back and it types again
    let before_claim = world.home_sends_keys(&world.on_a, &["C-b"]).await;
    world.home_claims(&world.on_a).await.assert_answered();
    let while_holding = world.home_sends_keys(&world.on_a, &["Enter"]).await;
    world.a.a_browser_takes_control_of(&world.on_a).await;
    let after_steal = world.home_sends_keys(&world.on_a, &["C-b"]).await;

    // Then the lease is enforced for the jail all the same: refused without it and after losing it,
    // typed while holding it
    before_claim.assert_refused_because_held_by(BROWSER_SCREEN);
    while_holding.assert_answered();
    after_steal.assert_refused_because_held_by(BROWSER_SCREEN);
    assert_eq!(
        jail.typed_so_far().await,
        b"\r".to_vec(),
        "the bytes the jail was typed"
    );
}

// The two below are not in the changeset's list: they pin what its 2026-10-26 replan adds, which
// none of the six above reaches — a call across hosts, and a lease that belongs to one home.

#[tokio::test]
async fn a_session_on_another_host_is_claimed_and_typed_into_on_that_host() {
    // Given a session of the project on host-b whose TUI is ready; the home session is on host-a
    let world = a_world().await;
    let tui = start_recording_terminal(&world.b, &world.on_b).await;
    let on_b = |keys: &[&str]| {
        serde_json::json!({
            "session_id": world.on_b,
            "daemon_instance_id": HOST_B,
            "keys": keys,
        })
    };

    // When the home session claims it and types, naming host-b
    world
        .claims(
            &world.home,
            serde_json::json!({ "session_id": world.on_b, "daemon_instance_id": HOST_B }),
        )
        .await
        .assert_answered();
    world
        .sends(&world.home, on_b(&["C-a"]))
        .await
        .assert_answered();

    // Then host-b's child was typed, and it is host-b that records the home session as the holder
    assert_eq!(
        tui.typed_after_waiting_for(&[CTRL_A]).await,
        vec![CTRL_A],
        "the bytes the child was typed"
    );
    assert_eq!(
        world.b.control_of(&world.on_b).await,
        Some(world.home_screen()),
        "who holds control on host-b"
    );
}

#[tokio::test]
async fn another_home_that_never_claimed_cannot_type_with_the_first_homes_claim() {
    // Given two home sessions of the project, the first of which holds control of a terminal
    let world = a_world().await;
    let tui = start_recording_terminal(&world.a, &world.on_a).await;
    world.home_claims(&world.on_a).await.assert_answered();

    // When the second types into it
    let refused = world
        .sends(
            &world.other_home,
            serde_json::json!({ "session_id": world.on_a, "keys": ["C-b"] }),
        )
        .await;

    // Then it is refused, naming the first home's screen as the holder, and nothing is written
    refused.assert_refused_because_held_by(&world.home_screen());
    world
        .home_sends_keys(&world.on_a, &["C-a"])
        .await
        .assert_answered();
    assert_eq!(
        tui.typed_after_waiting_for(&[CTRL_A]).await,
        vec![CTRL_A],
        "the bytes the child was typed"
    );
}
