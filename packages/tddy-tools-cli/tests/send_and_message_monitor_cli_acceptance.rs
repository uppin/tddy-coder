//! `tddy-tools send` and `tddy-tools monitor --kinds message`: one session sends another a message,
//! and the recipient's monitor — streaming its inbox watch, the messages addressed to it — prints it
//! as one `message` line, run as Claude Code's `Monitor` runs it: a process whose stdout is read
//! line by line (`#activity-wake` 5/5).
//!
//! The daemon is a stand-in mailbox: each session's tool socket (`TDDY_SANDBOX_TOOL_IPC`) is served
//! for that session, and all of them share one mailbox. It answers `PostSessionMessage`,
//! `CreateSessionWatch` and `WaitSessionWatchEvents` as the daemon does — a watch event of kind
//! `message` per message addressed to the caller, handed until a later call acknowledges it — and
//! keeps every call's arguments. With nothing left to hand, it refuses, so a monitor that has
//! printed everything stops with an `error` line instead of waiting forever. The command's stdout,
//! exit code and the calls it made are its whole interface, so this suite runs the real binary and
//! asserts on exactly those.
//!
//! The monitor's loop, acknowledgement and line protocol are #934's: until #934 is green these
//! fail on its `TODO(watch-monitor)`.
//!
//! Feature: docs/ft/daemon/1-WIP/PRD-2026-11-17-session-messages.md

#![cfg(unix)]

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;
use prost::Message as _;
use serde_json::{json, Value};
use tddy_rpc::{RpcMessage, RpcResult, RpcService, Status};
use tddy_service::proto::exec_tools::{ExecuteToolRequest, ExecuteToolResponse};

const POST_SESSION_MESSAGE: &str = "PostSessionMessage";
const CREATE_SESSION_WATCH: &str = "CreateSessionWatch";
const WAIT_SESSION_WATCH_EVENTS: &str = "WaitSessionWatchEvents";
/// The project's main session: an orchestrator.
const MAIN: &str = "01900000-0000-7000-8000-AABB00000900";
/// A session the orchestrator started.
const CHILD: &str = "01900000-0000-7000-8000-AABB00000901";
/// What the stand-in refuses with once it has nothing left to hand.
const NOTHING_LEFT: &str = "the stand-in has no more messages";

// ---------------------------------------------------------------------------
// The mailbox daemon on the sessions' tool sockets
// ---------------------------------------------------------------------------

/// One message as the mailbox keeps it.
#[derive(Clone)]
struct Kept {
    seq: u64,
    message_id: String,
    from: String,
    to: String,
    text: String,
}

/// What the mailbox holds, and every call it was asked.
#[derive(Default)]
struct Mailbox {
    messages: Vec<Kept>,
    /// Per recipient, the `seq` its inbox watch's stream acknowledged.
    acknowledged: std::collections::HashMap<String, u64>,
    /// `(caller, tool, args)` of every call, in order.
    calls: Vec<(String, String, Value)>,
}

/// A session's display label, as the daemon names it.
fn label_of(session_id: &str) -> &'static str {
    if session_id == MAIN {
        "main"
    } else {
        "fix-login"
    }
}

impl Mailbox {
    fn post(&mut self, from: &str, to: &str, text: &str) -> Kept {
        let seq = 5_120 + self.messages.len() as u64 + 1;
        let kept = Kept {
            seq,
            message_id: format!("m-{}", self.messages.len() + 1),
            from: from.to_string(),
            to: to.to_string(),
            text: text.to_string(),
        };
        self.messages.push(kept.clone());
        kept
    }

    /// The next batch of `caller`'s inbox watch `w-1`, once `ack_seq` is acknowledged.
    fn stream(&mut self, caller: &str, args: &Value) -> Result<Value, String> {
        if let Some(ack) = args["ack_seq"].as_u64() {
            let position = self.acknowledged.entry(caller.to_string()).or_default();
            *position = (*position).max(ack);
        }
        let position = self.acknowledged.get(caller).copied().unwrap_or_default();
        let waiting: Vec<&Kept> = self
            .messages
            .iter()
            .filter(|m| m.to == caller && m.seq > position)
            .collect();
        if waiting.is_empty() {
            return Err(format!("{WAIT_SESSION_WATCH_EVENTS}: {NOTHING_LEFT}"));
        }
        Ok(json!({
            "stream_id": "s-1",
            "events": waiting.iter().map(|m| a_message_event(m)).collect::<Vec<_>>(),
            "lost": 0,
            "ended": [],
            "next_ack_seq": waiting.last().map_or(position, |m| m.seq),
        }))
    }
}

/// A watch event of kind `message`, as `WaitSessionWatchEvents` answers it.
fn a_message_event(m: &Kept) -> Value {
    json!({
        "seq": m.seq,
        "watch_ids": ["w-1"],
        "kind": "message",
        "session_id": m.from,
        "session": label_of(&m.from),
        "summary": m.text,
        "record": {
            "seq": m.seq,
            "session_id": m.from,
            "source": "message",
            "text": m.text,
            "message": {
                "to_session_id": m.to,
                "message_id": m.message_id,
                "text": m.text,
            },
        },
    })
}

/// The mailbox serving one session's socket: every call it answers is that session's.
#[derive(Clone)]
struct AMailboxDaemon {
    session_id: String,
    mailbox: Arc<Mutex<Mailbox>>,
}

#[async_trait::async_trait]
impl RpcService for AMailboxDaemon {
    async fn handle_rpc(&self, service: &str, method: &str, message: &RpcMessage) -> RpcResult {
        if (service, method) != ("exec_tools.ExecToolService", "ExecuteTool") {
            return RpcResult::Unary(Err(Status::not_found(format!(
                "this stand-in answers only ExecuteTool, not {service}/{method}"
            ))));
        }
        let request = ExecuteToolRequest::decode(message.payload.as_slice())
            .expect("the request body decodes");
        let args: Value = serde_json::from_str(&request.args_json).expect("the args are JSON");
        let mut mailbox = self.mailbox.lock().expect("the mailbox");
        mailbox.calls.push((
            self.session_id.clone(),
            request.tool_name.clone(),
            args.clone(),
        ));
        let answer = match request.tool_name.as_str() {
            POST_SESSION_MESSAGE => {
                let kept = mailbox.post(
                    &self.session_id,
                    args["to_session_id"].as_str().unwrap_or_default(),
                    args["text"].as_str().unwrap_or_default(),
                );
                Ok(json!({
                    "message_id": kept.message_id,
                    "to_session_id": kept.to,
                    "sent_at_unix_ms": 1_793_000_000_000_u64,
                }))
            }
            CREATE_SESSION_WATCH => Ok(json!({
                "watch_id": "w-1",
                "session_ids": [self.session_id],
                "started_by_me": false,
                "kinds": ["message"],
                "created_at_unix_ms": 1_793_000_000_000_u64,
                "expires_at_unix_ms": 1_793_000_600_000_u64,
            })),
            WAIT_SESSION_WATCH_EVENTS => mailbox.stream(&self.session_id, &args),
            other => Err(format!("this stand-in does not serve {other}")),
        };
        let response = match answer {
            Ok(result) => ExecuteToolResponse {
                result_json: result.to_string(),
                ..Default::default()
            },
            Err(reason) => ExecuteToolResponse {
                is_error: true,
                error_message: reason,
                ..Default::default()
            },
        };
        RpcResult::Unary(Ok(response.encode_to_vec()))
    }
}

/// A session's tool socket, served by the mailbox for that session. Dropping it stops serving.
struct ATooledSession {
    socket: PathBuf,
    serving: tokio::task::JoinHandle<()>,
    _dir: tempfile::TempDir,
}

impl ATooledSession {
    fn served_by(mailbox: &Arc<Mutex<Mailbox>>, session_id: &str) -> Self {
        let dir = tempfile::tempdir().expect("the socket directory");
        let socket = dir.path().join("tool.sock");
        let listener = tokio::net::UnixListener::bind(&socket).expect("bind the tool socket");
        let daemon = AMailboxDaemon {
            session_id: session_id.to_string(),
            mailbox: Arc::clone(mailbox),
        };
        let serving = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let daemon = daemon.clone();
                tokio::spawn(async move {
                    let (read_half, write_half) = tokio::io::split(stream);
                    let (_client, endpoint) = tddy_stdio::StdioEndpoint::from_duplex(
                        read_half,
                        write_half,
                        daemon,
                        tddy_rpc::RequestTransport::UnixSocket,
                    );
                    endpoint.run().await;
                });
            }
        });
        Self {
            socket,
            serving,
            _dir: dir,
        }
    }
}

impl Drop for ATooledSession {
    fn drop(&mut self) {
        self.serving.abort();
    }
}

/// The main session and the child, each on its own socket of one mailbox.
struct AProject {
    mailbox: Arc<Mutex<Mailbox>>,
    main: ATooledSession,
    child: ATooledSession,
}

impl AProject {
    fn new() -> Self {
        let mailbox = Arc::new(Mutex::new(Mailbox::default()));
        Self {
            main: ATooledSession::served_by(&mailbox, MAIN),
            child: ATooledSession::served_by(&mailbox, CHILD),
            mailbox,
        }
    }

    /// The main session has sent the child `text`, as an earlier `send` left it in the mailbox.
    fn the_main_session_has_sent_the_child(&self, text: &str) -> u64 {
        self.mailbox
            .lock()
            .expect("the mailbox")
            .post(MAIN, CHILD, text)
            .seq
    }

    /// The arguments of every `CreateSessionWatch` `caller` made, in order.
    fn watches_created_by(&self, caller: &str) -> Vec<Value> {
        self.mailbox
            .lock()
            .expect("the mailbox")
            .calls
            .iter()
            .filter(|(by, tool, _)| by == caller && tool == CREATE_SESSION_WATCH)
            .map(|(_, _, args)| args.clone())
            .collect()
    }
}

// ---------------------------------------------------------------------------
// The sessions' processes
// ---------------------------------------------------------------------------

/// The routes to a daemon a test's own environment may carry, which the session must not inherit.
const INHERITED_ROUTES: [&str; 7] = [
    "TDDY_SOCKET",
    "TDDY_REMOTE_SESSION_ID",
    "TDDY_REMOTE_DAEMON_URL",
    "TDDY_REMOTE_DAEMON_SOCKET",
    "TDDY_REMOTE_LIVEKIT_URL",
    "TDDY_REMOTE_LIVEKIT_ROOM",
    "TDDY_REMOTE_LIVEKIT_TOKEN",
];

/// `tddy-tools` as a jailed session runs it: its tool socket in its environment, and no other
/// route to a daemon.
fn tddy_tools_in(session: &ATooledSession) -> Command {
    let mut cmd = cargo_bin_cmd!("tddy-tools");
    for inherited in INHERITED_ROUTES {
        cmd.env_remove(inherited);
    }
    cmd.env("TDDY_SANDBOX_TOOL_IPC", &session.socket);
    cmd
}

/// What one run left behind.
struct Ran {
    succeeded: bool,
    /// stdout, split on newlines as `Monitor` splits it.
    lines: Vec<String>,
    stderr: String,
}

impl Ran {
    /// Each stdout line, parsed; a line that is not JSON fails the test naming it.
    fn json_lines(&self) -> Vec<Value> {
        self.lines
            .iter()
            .map(|line| {
                serde_json::from_str(line)
                    .unwrap_or_else(|e| panic!("stdout line {line:?} is not JSON: {e}"))
            })
            .collect()
    }

    /// The `message` lines alone.
    fn message_lines(&self) -> Vec<Value> {
        self.json_lines()
            .into_iter()
            .filter(|line| line["type"] == "message")
            .collect()
    }
}

/// Run `tddy-tools <args>` in `session` to completion off the reactor: the socket it talks to is
/// served by this runtime.
async fn run_in(session: &ATooledSession, args: &[&str]) -> Ran {
    let mut cmd = tddy_tools_in(session);
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    let output = tokio::task::spawn_blocking(move || {
        cmd.args(&args)
            .timeout(Duration::from_secs(20))
            .output()
            .expect("tddy-tools ran")
    })
    .await
    .expect("tddy-tools ran to completion");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    Ran {
        succeeded: output.status.success(),
        lines: stdout.lines().map(str::to_string).collect(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// `tddy-tools monitor` streaming `session`'s inbox watch: the messages addressed to it.
async fn monitors_its_messages(session: &ATooledSession) -> Ran {
    run_in(session, &["monitor", "--kinds", "message", "--ttl", "600"]).await
}

/// The `message` line the monitor prints for `from`'s message `message_id` to `to` at `seq`.
fn the_line_for(seq: u64, message_id: &str, from: &str, to: &str, text: &str) -> Value {
    json!({
        "type": "message",
        "seq": seq,
        "watch_ids": ["w-1"],
        "message_id": message_id,
        "from_session_id": from,
        "from": label_of(from),
        "to_session_id": to,
        "text": text,
    })
}

// ---------------------------------------------------------------------------
// The tests
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn send_prints_the_message_id_and_the_recipient_receives_it() {
    // Given a project whose child session reports to the main session
    let project = AProject::new();

    // When the child sends the main session a message, and the main session monitors its messages
    let sent = run_in(
        &project.child,
        &["send", "--to", MAIN, "Tests pass; pushed 3f2a1c."],
    )
    .await;
    let monitored = monitors_its_messages(&project.main).await;

    // Then send printed the message's id as one JSON line, and the main session's monitor printed
    // that message
    assert_eq!(
        (
            sent.succeeded,
            sent.json_lines()
                .first()
                .map(|answer| answer["message_id"].clone())
        ),
        (true, Some(json!("m-1"))),
        "send's stderr: {}",
        sent.stderr
    );
    assert_eq!(
        monitored.message_lines(),
        vec![the_line_for(
            5_121,
            "m-1",
            CHILD,
            MAIN,
            "Tests pass; pushed 3f2a1c."
        )],
        "the monitor's stdout: {:?}",
        monitored.lines
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn monitor_kinds_message_streams_a_sessions_own_messages_one_line_each() {
    // Given two messages the main session sent the child
    let project = AProject::new();
    let first = project.the_main_session_has_sent_the_child("Rebase on main.");
    let second = project.the_main_session_has_sent_the_child("Then open the PR.");

    // When the child monitors its messages
    let ran = monitors_its_messages(&project.child).await;

    // Then it asked for its inbox watch — the kind `message` alone, selecting no session — and
    // printed one message line per message, oldest first
    assert_eq!(
        project
            .watches_created_by(CHILD)
            .iter()
            .map(|args| (
                args["kinds"].clone(),
                args["session_ids"]
                    .as_array()
                    .is_some_and(|ids| !ids.is_empty()),
                args["started_by_me"] == json!(true),
            ))
            .collect::<Vec<_>>(),
        vec![(json!(["message"]), false, false)],
        "stderr: {}",
        ran.stderr
    );
    assert_eq!(
        ran.message_lines(),
        vec![
            the_line_for(first, "m-1", MAIN, CHILD, "Rebase on main."),
            the_line_for(second, "m-2", MAIN, CHILD, "Then open the PR."),
        ],
        "stdout: {:?}",
        ran.lines
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_message_text_with_newlines_stays_on_one_line() {
    // Given a message whose text spans three lines
    let project = AProject::new();
    let seq = project.the_main_session_has_sent_the_child("Rebase on main.\nThen test.\nThen PR.");

    // When the child monitors its messages
    let ran = monitors_its_messages(&project.child).await;

    // Then the message is one stdout line carrying the text whole
    assert_eq!(
        ran.lines
            .iter()
            .filter(|line| line.contains("\"type\":\"message\""))
            .count(),
        1,
        "stdout: {:?}",
        ran.lines
    );
    assert_eq!(
        ran.message_lines(),
        vec![the_line_for(
            seq,
            "m-1",
            MAIN,
            CHILD,
            "Rebase on main.\nThen test.\nThen PR."
        )]
    );
}
