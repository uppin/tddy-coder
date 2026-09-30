//! Acceptance test: the session's agent roster is known before the first `tools/list` is answered.
//!
//! Feature: docs/ft/daemon/session-agent-roster.md § The roster stream
//!
//! A host-run agent is spawned with no roster seed at all: its `tddy-tools --mcp` learns the
//! attached agents only from `StreamSessionAgents`. Claude Code asks for `tools/list` the moment the
//! handshake completes, and it built its tool pool from that first answer — a later
//! `tools/list_changed` did not reach the turn already running. So a server that answered before
//! the first roster snapshot handed the agent a catalog with no way to reach the agent it was told
//! to use, and with none of that agent's tool takeovers applied (session 01a0f089, 2026-09-30:
//! "ask Gemma to find …" answered with "the tools for reaching it aren't available").
//!
//! The stub daemon here delays its snapshot on purpose, so the test fails deterministically when
//! the server answers first rather than depending on how fast a real daemon happens to be.

use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use prost::Message;
use serde_json::{json, Value};
use tddy_rpc::{RpcMessage, RpcResult, RpcService, Status};
use tddy_service::proto::session_agents_svc::{SessionAgentEntry, SessionAgentRoster};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};

/// Long enough that a server answering before the snapshot always loses the race; short against
/// the time the server waits for a roster before serving without one.
const SNAPSHOT_DELAY: Duration = Duration::from_millis(700);

const IO_TIMEOUT: Duration = Duration::from_secs(10);

const THE_CODEBASE_SESSION: &str = "01a0f089-6df2-77a3-83c8-32d6273b7640";

const THE_AGENT: &str = "Gemma Local Coder@a-daemon";

/// A stub of the facilitating daemon's `SessionAgentService`: `StreamSessionAgents` answers with
/// one snapshot attaching [`THE_AGENT`] after [`SNAPSHOT_DELAY`], then holds the stream open the
/// way the daemon does. Every other call is refused.
struct DaemonServingOneAgentLate;

#[async_trait]
impl RpcService for DaemonServingOneAgentLate {
    async fn handle_rpc(&self, service: &str, method: &str, _message: &RpcMessage) -> RpcResult {
        if method != "StreamSessionAgents" {
            return RpcResult::Unary(Err(Status::not_found(format!(
                "unknown {service}/{method}"
            ))));
        }
        let (frames, stream) = tokio::sync::mpsc::channel(4);
        tokio::spawn(async move {
            tokio::time::sleep(SNAPSHOT_DELAY).await;
            let snapshot = SessionAgentRoster {
                rev: 1,
                agents: vec![SessionAgentEntry {
                    agent_id: THE_AGENT.to_string(),
                    name: "Gemma Local Coder".to_string(),
                    daemon_instance_id: "a-daemon".to_string(),
                    replaces: vec!["Grep".to_string(), "Shell".to_string()],
                    ..Default::default()
                }],
                ..Default::default()
            };
            if frames.send(Ok(snapshot.encode_to_vec())).await.is_err() {
                return;
            }
            // Held open: a stream that ended would read as a dropped subscription.
            frames.closed().await;
        });
        RpcResult::ServerStream(Ok(stream))
    }
}

/// A stub daemon that accepts the `StreamSessionAgents` subscription and never sends a snapshot —
/// the one case the wait for a roster must not turn into an agent that never starts.
struct DaemonThatNeverAnswers;

#[async_trait]
impl RpcService for DaemonThatNeverAnswers {
    async fn handle_rpc(&self, service: &str, method: &str, _message: &RpcMessage) -> RpcResult {
        if method != "StreamSessionAgents" {
            return RpcResult::Unary(Err(Status::not_found(format!(
                "unknown {service}/{method}"
            ))));
        }
        let (frames, stream) = tokio::sync::mpsc::channel::<Result<Vec<u8>, Status>>(1);
        tokio::spawn(async move { frames.closed().await });
        RpcResult::ServerStream(Ok(stream))
    }
}

/// Serve [`DaemonServingOneAgentLate`] on a fresh Unix socket, one endpoint per connection — the
/// server opens a connection for the roster subscription of its own.
///
/// `tag` names the socket: the tests run in parallel, and two binding one path would collide.
fn a_daemon_socket_serving_one_agent_late(
    tag: &str,
) -> (std::path::PathBuf, tokio::task::JoinHandle<()>) {
    a_daemon_socket_serving(tag, || DaemonServingOneAgentLate)
}

/// Serve the stub `service` builds on a fresh Unix socket named by `tag`, one endpoint per
/// connection.
fn a_daemon_socket_serving<S: RpcService + 'static>(
    tag: &str,
    service: fn() -> S,
) -> (std::path::PathBuf, tokio::task::JoinHandle<()>) {
    let socket_path = tddy_sandbox::SandboxSpec::short_ipc_socket_path(tag);
    let listener = tokio::net::UnixListener::bind(&socket_path).expect("bind stub daemon socket");
    let daemon = tokio::spawn(async move {
        loop {
            let Ok((stream, _addr)) = listener.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let (read_half, write_half) = tokio::io::split(stream);
                let (_client, endpoint) = tddy_stdio::StdioEndpoint::from_duplex(
                    read_half,
                    write_half,
                    service(),
                    tddy_rpc::RequestTransport::UnixSocket,
                );
                endpoint.run().await;
            });
        }
    });
    (socket_path, daemon)
}

/// The real `tddy-tools --mcp`, wired the way a host-run agent is: the daemon socket and the
/// codebase session, and no roster seed.
fn an_mcp_server_with_no_roster_seed(daemon_socket: &std::path::Path) -> Child {
    tokio::process::Command::new(env!("CARGO_BIN_EXE_tddy-tools"))
        .arg("--mcp")
        .env_remove("TDDY_SUBAGENTS_JSON")
        .env_remove("TDDY_SANDBOX_TOOL_IPC")
        .env("TDDY_REMOTE_DAEMON_SOCKET", daemon_socket)
        .env("TDDY_REMOTE_SESSION_ID", THE_CODEBASE_SESSION)
        .env("TDDY_REMOTE_SESSION_TOKEN", "a-session-token")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn tddy-tools --mcp")
}

async fn send(stdin: &mut ChildStdin, message: Value) {
    let line = format!("{message}\n");
    tokio::time::timeout(IO_TIMEOUT, stdin.write_all(line.as_bytes()))
        .await
        .expect("write to tddy-tools stdin timed out")
        .expect("write to tddy-tools stdin");
}

/// The response to request `id`, skipping any notification the server sends in between.
async fn the_response_to(stdout: &mut BufReader<ChildStdout>, id: u64) -> Value {
    loop {
        let mut line = String::new();
        tokio::time::timeout(IO_TIMEOUT, stdout.read_line(&mut line))
            .await
            .expect("read from tddy-tools stdout timed out")
            .expect("read from tddy-tools stdout");
        let message: Value = serde_json::from_str(&line)
            .unwrap_or_else(|e| panic!("invalid JSON-RPC line {line:?}: {e}"));
        if message["id"] == json!(id) {
            return message;
        }
    }
}

/// Complete the handshake and ask for the tool list at once, with no pause anywhere — the
/// order and pace Claude Code uses.
async fn the_first_tool_list_a_client_is_given(child: &mut Child) -> Vec<String> {
    let mut stdin = child.stdin.take().expect("child stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("child stdout"));
    send(
        &mut stdin,
        json!({
            "jsonrpc": "2.0", "id": 0, "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "an-eager-client", "version": "0.0.1"}
            }
        }),
    )
    .await;
    the_response_to(&mut stdout, 0).await;
    send(
        &mut stdin,
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
    )
    .await;
    send(
        &mut stdin,
        json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}),
    )
    .await;
    let listed = the_response_to(&mut stdout, 1).await;
    listed["result"]["tools"]
        .as_array()
        .expect("tools/list returns a tools array")
        .iter()
        .map(|tool| tool["name"].as_str().unwrap_or_default().to_string())
        .collect()
}

#[tokio::test]
async fn the_first_tool_list_already_reaches_the_agent_the_roster_attaches() {
    // Given — an agent attached to the session, announced only over the roster stream, and late
    let (socket, daemon) = a_daemon_socket_serving_one_agent_late("rosterreach");
    let mut server = an_mcp_server_with_no_roster_seed(&socket);

    // When — a client asks for its tools the moment it can
    let tools = the_first_tool_list_a_client_is_given(&mut server).await;
    let _ = server.kill().await;
    daemon.abort();

    // Then — the very first answer can already address the agent
    assert!(
        tools.iter().any(|name| name == "subagent_new_session")
            && tools.iter().any(|name| name == "subagent_prompt"),
        "the first tools/list must carry the conversation tools for the agent the roster \
         attaches; got: {tools:?}"
    );
}

#[tokio::test]
async fn the_first_tool_list_already_withholds_what_the_attached_agent_replaces() {
    // Given
    let (socket, daemon) = a_daemon_socket_serving_one_agent_late("rosterwithhold");
    let mut server = an_mcp_server_with_no_roster_seed(&socket);

    // When
    let tools = the_first_tool_list_a_client_is_given(&mut server).await;
    let _ = server.kill().await;
    daemon.abort();

    // Then — a takeover holds from the first answer, not from whenever the roster lands
    let offered: Vec<&String> = tools
        .iter()
        .filter(|name| *name == "Grep" || *name == "Shell")
        .collect();
    assert!(
        offered.is_empty(),
        "the tools the attached agent replaces must be withheld from the first tools/list; \
         still offered: {offered:?}"
    );
}

#[tokio::test]
async fn serves_on_the_seed_when_the_daemon_never_sends_a_roster() {
    // Given — a subscription the daemon accepts and never answers
    let (socket, daemon) = a_daemon_socket_serving("rosternever", || DaemonThatNeverAnswers);
    let mut server = an_mcp_server_with_no_roster_seed(&socket);

    // When
    let asked_at = std::time::Instant::now();
    let tools = the_first_tool_list_a_client_is_given(&mut server).await;
    let waited = asked_at.elapsed();
    let _ = server.kill().await;
    daemon.abort();

    // Then — the agent still starts, after a bounded wait, on the catalog it would have had anyway
    assert!(
        waited < Duration::from_secs(8),
        "the wait for a roster must be bounded well inside an MCP client's patience; took {waited:?}"
    );
    assert!(
        tools.iter().any(|name| name == "Read")
            && !tools.iter().any(|name| name == "subagent_new_session"),
        "with no roster, the first tools/list is the seed's catalog; got: {tools:?}"
    );
}

/// The roster a host-run agent is spawned with, as the spawning daemon read it from the codebase
/// host: [`THE_AGENT`], replacing Grep and Shell, at rev 1.
fn the_roster_the_spawn_read() -> SessionAgentRoster {
    SessionAgentRoster {
        rev: 1,
        agents: vec![SessionAgentEntry {
            agent_id: THE_AGENT.to_string(),
            name: "Gemma Local Coder".to_string(),
            daemon_instance_id: "a-daemon".to_string(),
            replaces: vec!["Grep".to_string(), "Shell".to_string()],
            ..Default::default()
        }],
        ..Default::default()
    }
}

/// The real `tddy-tools --mcp` wired like [`an_mcp_server_with_no_roster_seed`], plus the roster
/// the spawn already read.
fn an_mcp_server_seeded_with(
    daemon_socket: &std::path::Path,
    roster: &SessionAgentRoster,
) -> Child {
    let (seed_key, seed_value) = tddy_tools::session_agents::roster_seed_env(roster);
    tokio::process::Command::new(env!("CARGO_BIN_EXE_tddy-tools"))
        .arg("--mcp")
        .env_remove("TDDY_SUBAGENTS_JSON")
        .env_remove("TDDY_SANDBOX_TOOL_IPC")
        .env("TDDY_REMOTE_DAEMON_SOCKET", daemon_socket)
        .env("TDDY_REMOTE_SESSION_ID", THE_CODEBASE_SESSION)
        .env("TDDY_REMOTE_SESSION_TOKEN", "a-session-token")
        .env(seed_key, seed_value)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn tddy-tools --mcp")
}

/// How long a client waits for `server`'s first tool list, and what it is given.
async fn the_first_tool_list_and_how_long_it_took(mut server: Child) -> (Vec<String>, Duration) {
    let asked_at = std::time::Instant::now();
    let tools = the_first_tool_list_a_client_is_given(&mut server).await;
    let waited = asked_at.elapsed();
    let _ = server.kill().await;
    (tools, waited)
}

#[tokio::test]
async fn a_spawn_seeded_roster_is_in_the_first_tool_list_without_waiting_for_the_stream() {
    // Given — a daemon that never answers the subscription, so only the seed can put the agent in
    // the first answer; and, as a baseline, the same start without the seed, which waits out the
    // stream (and absorbs the first launch of a freshly linked binary, which alone can take
    // seconds on macOS)
    let (socket, daemon) = a_daemon_socket_serving("rosterseeded", || DaemonThatNeverAnswers);
    let (_, unseeded) =
        the_first_tool_list_and_how_long_it_took(an_mcp_server_with_no_roster_seed(&socket)).await;

    // When
    let (tools, seeded) = the_first_tool_list_and_how_long_it_took(an_mcp_server_seeded_with(
        &socket,
        &the_roster_the_spawn_read(),
    ))
    .await;
    daemon.abort();

    // Then — the seeded roster is the first answer
    assert!(
        tools.iter().any(|name| name == "subagent_new_session"),
        "the first tools/list must reach the agent the spawn seeded; got: {tools:?}"
    );
    assert!(
        !tools.iter().any(|name| name == "Grep" || name == "Shell"),
        "the first tools/list must withhold what the seeded agent replaces; got: {tools:?}"
    );
    // … and a roster already known skips the stream wait the unseeded start paid. Compared rather
    // than bounded, because process start-up time is the machine's, not the code's.
    assert!(
        seeded + Duration::from_secs(3) < unseeded,
        "a roster the spawn already supplied must not wait on the stream; seeded start took \
         {seeded:?}, unseeded {unseeded:?}"
    );
}
