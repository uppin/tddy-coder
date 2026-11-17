//! `tddy-tools monitor`: an orchestrator's watch events streamed as one JSON object per stdout line,
//! run as Claude Code's `Monitor` runs it — a process whose stdout is read line by line, its exit
//! the end of the watch (`#activity-wake` 4/5).
//!
//! The daemon is a stand-in on the session's tool socket (`TDDY_SANDBOX_TOOL_IPC`) that answers
//! `CreateSessionWatch` and `WaitSessionWatchEvents` as the daemon does — the stream's batches it
//! is scripted with, one per call — and keeps every call's arguments. The command's stdout, exit
//! code and the calls it made are its whole interface, so this suite runs the real binary and
//! asserts on exactly those.
//!
//! Feature: docs/ft/daemon/1-WIP/PRD-2026-11-17-session-watch-monitor.md

#![cfg(unix)]

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;
use prost::Message as _;
use serde_json::{json, Value};
use tddy_rpc::{RpcMessage, RpcResult, RpcService, Status};
use tddy_service::proto::exec_tools::{ExecuteToolRequest, ExecuteToolResponse};

const CREATE_SESSION_WATCH: &str = "CreateSessionWatch";
const WAIT_SESSION_WATCH_EVENTS: &str = "WaitSessionWatchEvents";
const CHILD: &str = "01900000-0000-7000-8000-AABB00000801";

// ---------------------------------------------------------------------------
// The daemon on the session's tool socket
// ---------------------------------------------------------------------------

/// How the stand-in answers one `WaitSessionWatchEvents` call.
#[derive(Clone)]
enum StreamAnswer {
    /// The batch, as the daemon answers it.
    Batch(Value),
    /// The refusal, as the tool's error message.
    Refused(String),
}

/// What the stand-in daemon was asked, and what it answers next.
#[derive(Default)]
struct DaemonState {
    calls: Vec<(String, Value)>,
    stream: VecDeque<StreamAnswer>,
}

/// An `ExecToolService` that creates watch `w-1` and answers each stream call with the next of its
/// scripted answers — a refusal once they run out, so a monitor that keeps calling stops.
#[derive(Clone)]
struct AWatchingDaemon {
    state: Arc<Mutex<DaemonState>>,
}

#[async_trait::async_trait]
impl RpcService for AWatchingDaemon {
    async fn handle_rpc(&self, service: &str, method: &str, message: &RpcMessage) -> RpcResult {
        if (service, method) != ("exec_tools.ExecToolService", "ExecuteTool") {
            return RpcResult::Unary(Err(Status::not_found(format!(
                "this stand-in answers only ExecuteTool, not {service}/{method}"
            ))));
        }
        let request = ExecuteToolRequest::decode(message.payload.as_slice())
            .expect("the request body decodes");
        let args: Value = serde_json::from_str(&request.args_json).expect("the args are JSON");
        let mut state = self.state.lock().expect("the stand-in's state");
        state.calls.push((request.tool_name.clone(), args));
        let answer = match request.tool_name.as_str() {
            CREATE_SESSION_WATCH => Ok(json!({
                "watch_id": "w-1",
                "session_ids": [],
                "started_by_me": true,
                "kinds": ["permission"],
                "created_at_unix_ms": 1_793_000_000_000_u64,
                "expires_at_unix_ms": 1_793_000_600_000_u64,
            })),
            WAIT_SESSION_WATCH_EVENTS => match state.stream.pop_front() {
                Some(StreamAnswer::Batch(batch)) => Ok(batch),
                Some(StreamAnswer::Refused(reason)) => Err(reason),
                None => Err(format!(
                    "{WAIT_SESSION_WATCH_EVENTS}: the stand-in's script ran out"
                )),
            },
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

/// The stand-in daemon serving a session's tool socket. Dropping it stops serving.
struct ATooledSession {
    socket: PathBuf,
    state: Arc<Mutex<DaemonState>>,
    serving: tokio::task::JoinHandle<()>,
    _dir: tempfile::TempDir,
}

impl ATooledSession {
    /// A session whose daemon answers its stream calls with `script`, in order.
    fn streaming(script: Vec<StreamAnswer>) -> Self {
        let dir = tempfile::tempdir().expect("the socket directory");
        let socket = dir.path().join("tool.sock");
        let state = Arc::new(Mutex::new(DaemonState {
            calls: Vec::new(),
            stream: script.into(),
        }));
        let listener = tokio::net::UnixListener::bind(&socket).expect("bind the tool socket");
        let daemon = AWatchingDaemon {
            state: Arc::clone(&state),
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
            state,
            serving,
            _dir: dir,
        }
    }

    /// Every tool the monitor called, in order.
    fn tools_called(&self) -> Vec<String> {
        self.state
            .lock()
            .expect("the stand-in's state")
            .calls
            .iter()
            .map(|(tool, _)| tool.clone())
            .collect()
    }

    /// The `(watch_ids, stream_id, ack_seq)` of every stream call, in order.
    fn stream_calls(&self) -> Vec<(Value, Value, Value)> {
        self.state
            .lock()
            .expect("the stand-in's state")
            .calls
            .iter()
            .filter(|(tool, _)| tool == WAIT_SESSION_WATCH_EVENTS)
            .map(|(_, args)| {
                (
                    args["watch_ids"].clone(),
                    args["stream_id"].clone(),
                    args["ack_seq"].clone(),
                )
            })
            .collect()
    }
}

impl Drop for ATooledSession {
    fn drop(&mut self) {
        self.serving.abort();
    }
}

// ---------------------------------------------------------------------------
// What the daemon answers
// ---------------------------------------------------------------------------

/// One event of a batch, as the daemon answers it: a permission prompt of the child, `summary`
/// worded as the wake words it.
fn a_prompt(seq: u64, request_id: &str, summary: &str) -> Value {
    json!({
        "seq": seq,
        "watch_ids": ["w-1"],
        "kind": "permission",
        "session_id": CHILD,
        "session": "fix-login",
        "summary": summary,
        "request_id": request_id,
        "record": { "seq": seq, "session_id": CHILD, "text": summary },
    })
}

/// A batch of stream `s-1` carrying `events` and the watches in `ended`.
fn a_batch(events: Vec<Value>, ended: &[&str], next_ack_seq: u64) -> StreamAnswer {
    a_batch_of_stream("s-1", events, ended, next_ack_seq)
}

/// [`a_batch`] of the stream `stream_id`.
fn a_batch_of_stream(
    stream_id: &str,
    events: Vec<Value>,
    ended: &[&str],
    next_ack_seq: u64,
) -> StreamAnswer {
    StreamAnswer::Batch(json!({
        "stream_id": stream_id,
        "events": events,
        "lost": 0,
        "ended": ended
            .iter()
            .map(|watch_id| json!({ "watch_id": watch_id, "reason": "sessions_ended" }))
            .collect::<Vec<_>>(),
        "next_ack_seq": next_ack_seq,
    }))
}

/// The event line the monitor prints for [`a_prompt`]: the batch's event without its record.
fn the_line_for(seq: u64, request_id: &str, summary: &str) -> Value {
    json!({
        "type": "event",
        "seq": seq,
        "watch_ids": ["w-1"],
        "kind": "permission",
        "session_id": CHILD,
        "session": "fix-login",
        "summary": summary,
        "request_id": request_id,
    })
}

fn an_ended_line(watch_id: &str) -> Value {
    json!({ "type": "ended", "watch_id": watch_id, "reason": "sessions_ended" })
}

// ---------------------------------------------------------------------------
// The orchestrator's process
// ---------------------------------------------------------------------------

/// `tddy-tools` as a jailed orchestrator runs it: its session's tool socket in its environment,
/// and no other route to a daemon.
fn tddy_tools_in(session: &ATooledSession) -> Command {
    let mut cmd = cargo_bin_cmd!("tddy-tools");
    for inherited in [
        "TDDY_SOCKET",
        "TDDY_REMOTE_SESSION_ID",
        "TDDY_REMOTE_DAEMON_URL",
        "TDDY_REMOTE_DAEMON_SOCKET",
        "TDDY_REMOTE_LIVEKIT_URL",
        "TDDY_REMOTE_LIVEKIT_ROOM",
        "TDDY_REMOTE_LIVEKIT_TOKEN",
    ] {
        cmd.env_remove(inherited);
    }
    cmd.env("TDDY_SANDBOX_TOOL_IPC", &session.socket);
    cmd
}

/// What one run of the monitor left behind.
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

    /// The `type` of each stdout line.
    fn line_types(&self) -> Vec<String> {
        self.json_lines()
            .iter()
            .map(|line| line["type"].as_str().unwrap_or("").to_string())
            .collect()
    }
}

/// Run `tddy-tools monitor <args>` to completion off the reactor: the socket it talks to is served
/// by this runtime.
async fn monitor(session: &ATooledSession, args: &[&str]) -> Ran {
    let mut cmd = tddy_tools_in(session);
    let args: Vec<String> = std::iter::once("monitor".to_string())
        .chain(args.iter().map(|a| a.to_string()))
        .collect();
    let output = tokio::task::spawn_blocking(move || {
        cmd.args(&args)
            .timeout(std::time::Duration::from_secs(20))
            .output()
            .expect("the monitor ran")
    })
    .await
    .expect("the monitor ran to completion");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    Ran {
        succeeded: output.status.success(),
        lines: stdout.lines().map(str::to_string).collect(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

// ---------------------------------------------------------------------------
// The tests
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn monitor_prints_a_watch_line_then_one_event_line_per_matched_record() {
    // Given a daemon whose stream hands two prompts, then reports the watch ended
    let session = ATooledSession::streaming(vec![
        a_batch(
            vec![
                a_prompt(4182, "req-1", "wants to run Bash: cargo test"),
                a_prompt(4190, "req-2", "wants to run Bash: cargo fmt"),
            ],
            &[],
            4190,
        ),
        a_batch(Vec::new(), &["w-1"], 4190),
    ]);

    // When the orchestrator monitors the permission prompts of the sessions it started
    let ran = monitor(
        &session,
        &["--started-by-me", "--kinds", "permission", "--ttl", "600"],
    )
    .await;

    // Then it created one watch and printed its line, then one line per prompt with its request id
    assert_eq!(
        session.tools_called().first().map(String::as_str),
        Some(CREATE_SESSION_WATCH),
        "stderr: {}",
        ran.stderr
    );
    let lines = ran.json_lines();
    assert_eq!(
        (
            lines
                .first()
                .map(|line| (line["type"].clone(), line["watch_id"].clone())),
            lines.get(1..3).map(<[Value]>::to_vec),
        ),
        (
            Some((json!("watch"), json!("w-1"))),
            Some(vec![
                the_line_for(4182, "req-1", "wants to run Bash: cargo test"),
                the_line_for(4190, "req-2", "wants to run Bash: cargo fmt"),
            ]),
        ),
        "stdout: {:?}",
        ran.lines
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_stdout_line_is_one_json_object() {
    // Given a daemon whose stream hands prompts whose wording spans several lines
    let session = ATooledSession::streaming(vec![
        a_batch(
            vec![
                a_prompt(7, "req-1", "wants to run Bash:\ncargo test\n--all"),
                a_prompt(8, "req-2", "wants to run Bash: cargo fmt"),
            ],
            &[],
            8,
        ),
        a_batch(Vec::new(), &["w-1"], 8),
    ]);

    // When the orchestrator monitors it
    let ran = monitor(&session, &["--watch", "w-1"]).await;

    // Then stdout is exactly one line per event and one for the end, each a JSON object
    assert_eq!(
        ran.line_types(),
        vec!["event", "event", "ended"],
        "stdout: {:?}",
        ran.lines
    );
    assert_eq!(
        ran.json_lines()
            .iter()
            .map(Value::is_object)
            .collect::<Vec<_>>(),
        vec![true, true, true]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn monitor_exits_zero_after_an_ended_line_for_each_watch() {
    // Given a daemon whose stream reports one watch ended, then the other
    let session = ATooledSession::streaming(vec![
        a_batch(Vec::new(), &["w-1"], 0),
        a_batch(Vec::new(), &["w-2"], 0),
    ]);

    // When the orchestrator monitors both
    let ran = monitor(&session, &["--watch", "w-1", "--watch", "w-2"]).await;

    // Then it printed an ended line for each and exited 0 only after the second
    assert_eq!(
        (ran.json_lines(), ran.succeeded),
        (vec![an_ended_line("w-1"), an_ended_line("w-2")], true),
        "stderr: {}",
        ran.stderr
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn monitor_prints_an_error_line_and_exits_non_zero_on_a_refusal() {
    // Given a daemon that refuses the stream because another monitor holds the watch
    let daemons_reason = "watch w-1 is already streamed by another monitor";
    let session = ATooledSession::streaming(vec![StreamAnswer::Refused(format!(
        "{WAIT_SESSION_WATCH_EVENTS}: {daemons_reason}"
    ))]);

    // When the orchestrator monitors it
    let ran = monitor(&session, &["--watch", "w-1"]).await;

    // Then its only line is an error carrying the daemon's reason, and it exits non-zero
    let lines = ran.json_lines();
    assert_eq!(
        (ran.line_types(), ran.succeeded),
        (vec!["error".to_string()], false),
        "stdout: {:?}",
        ran.lines
    );
    // The daemon's reason may be prefixed with the tool's name: only that it is carried is pinned.
    assert!(
        lines[0]["message"]
            .as_str()
            .is_some_and(|message| message.contains(daemons_reason)),
        "the error line {} does not carry the daemon's reason",
        lines[0]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn monitor_with_watch_continues_from_the_acknowledged_position() {
    // Given a watch whose acknowledged position the daemon keeps, two prompts waiting after it
    let session = ATooledSession::streaming(vec![
        a_batch(
            vec![
                a_prompt(7, "req-7", "wants to run Bash: cargo test"),
                a_prompt(8, "req-8", "wants to run Bash: cargo fmt"),
            ],
            &[],
            8,
        ),
        a_batch(Vec::new(), &["w-1"], 8),
    ]);

    // When the orchestrator re-attaches a monitor to it
    let ran = monitor(&session, &["--watch", "w-1"]).await;

    // Then no watch is created; the first call starts a stream acknowledging nothing, and the next
    // presents that stream and acknowledges what was printed
    assert_eq!(
        (
            session
                .tools_called()
                .contains(&CREATE_SESSION_WATCH.to_string()),
            session.stream_calls()
        ),
        (
            false,
            vec![
                (json!(["w-1"]), Value::Null, Value::Null),
                (json!(["w-1"]), json!("s-1"), json!(8)),
            ]
        ),
        "stderr: {}",
        ran.stderr
    );
    assert_eq!(
        ran.json_lines(),
        vec![
            the_line_for(7, "req-7", "wants to run Bash: cargo test"),
            the_line_for(8, "req-8", "wants to run Bash: cargo fmt"),
            an_ended_line("w-1"),
        ]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn monitor_with_watch_resumes_a_watch_another_monitor_was_streaming() {
    // Given a watch an earlier monitor was streaming and still leases: the daemon hands the
    // resuming stream its own id and the batch the earlier one never acknowledged
    let session = ATooledSession::streaming(vec![
        a_batch_of_stream(
            "s-2",
            vec![a_prompt(9, "req-9", "wants to run Bash: cargo test")],
            &[],
            9,
        ),
        a_batch_of_stream("s-2", Vec::new(), &["w-1"], 9),
    ]);

    // When the orchestrator resumes its monitor on the watch
    let ran = monitor(&session, &["--watch", "w-1"]).await;

    // Then it is not refused: it streams on under the new stream from the acknowledged position,
    // and exits 0 once the watch ends
    assert_eq!(
        (ran.json_lines(), ran.succeeded),
        (
            vec![
                the_line_for(9, "req-9", "wants to run Bash: cargo test"),
                an_ended_line("w-1"),
            ],
            true,
        ),
        "stderr: {}",
        ran.stderr
    );
    assert_eq!(
        session.stream_calls(),
        vec![
            (json!(["w-1"]), Value::Null, Value::Null),
            (json!(["w-1"]), json!("s-2"), json!(9)),
        ]
    );
}
