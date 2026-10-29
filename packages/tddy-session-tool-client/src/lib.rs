//! Generic session tool dispatch — forwards MCP tool calls to `tddy-daemon` via sandbox IPC,
//! direct HTTP, or LiveKit RPC to a remote daemon, depending on environment.
//!
//! Moved out of `tddy-tools` by `#unbundle` node 5. It is a crate of its own rather than part of
//! `tddy-service` — where the plan first put it — because [`dispatch_session_tool`] selects
//! between all four transports in one place. The LiveKit arm needed `tddy-livekit`, which itself
//! depends on `tddy-service`, and Cargo rejects that edge (`cyclic package dependency`); it now
//! joins its room through `tddy_room_client::RoomConnector` instead, which the binary installs
//! ([`install_room_connector`]) — this crate names no LiveKit crate.
//!
//! Sitting above `tddy-service` also settles a debt the plan recorded against the original
//! destination: `tddy-service` depends on `tddy-tui`, so a client hosted there would have pulled the
//! TUI into every in-jail binary that dispatches a tool call.
//!
//! `tddy-daemon`, `tddy-sandbox-app` and `tddy-sandbox-darwin` reach the daemon through this crate
//! and no longer depend on `tddy-tools` at all.

mod approval;
mod conversation;
pub mod daemon_http_rpc;
mod exec_catalog;
mod room_transport;
mod rpc_transports;

pub use approval::{
    request_approval, request_approval_via_rpc_transport, request_approval_within,
    APPROVAL_CLIENT_BOUND,
};
pub use conversation::{
    conversation_worktree, diff_conversation_worktree, dispatch_conversation_tool,
    pull_conversation_range, reset_conversation_worktree, sync_conversation_worktree,
    ConversationWorktreeOp,
};
pub use exec_catalog::{session_exec_tool_answer, SessionExecToolAnswer};

use std::path::PathBuf;

/// Read an environment variable, treating unset and blank alike — a blank join token or server
/// identity configures nothing. One spelling for the whole workspace, in the crate that owns the
/// rule.
use tddy_core::spawn_env::env_non_empty as non_empty_env;

use crate::room_transport::dispatch_via_livekit;
pub use tddy_sandbox::session_id_from_env;

/// How `tddy-tools --mcp` reaches the daemon's `ExecuteTool` handler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionToolTransport {
    /// In-jail MCP → unix socket → sandbox-runner → SessionChannel → host daemon.
    SandboxIpc { socket_path: PathBuf },
    /// Direct connection to *this host's* daemon over its agent-facing unix socket, using the same
    /// `tddy-rpc` framing the sandbox socket uses.
    ///
    /// The transport a co-located agent needs when the daemon runs **embedded** in an application
    /// (Tddy Desktop): such a host serves no HTTP listener at all
    /// (`tddy_daemon::runtime::RuntimeHost::Embedded`), so [`SessionToolTransport::DaemonHttp`]
    /// pointed at `listen.web_port` reached the OAuth callback port and every tool call came back
    /// `relay parse error` from an empty 404 body.
    ///
    /// Unlike [`SessionToolTransport::SandboxIpc`] this carries a full envelope: the agent runs
    /// *beside* the jail rather than inside one, so the connection implies no session and the
    /// daemon has to be told which one is calling.
    DaemonUds {
        socket_path: PathBuf,
        session_id: String,
        session_token: String,
        daemon_instance_id: String,
    },
    /// Direct HTTP Connect POST to the daemon's `/rpc` — `ExecToolService/ExecuteTool` for tool
    /// calls, and, through the relay, the roster and conversation RPCs (see
    /// [`crate::daemon_http_rpc::DaemonHttpRpcClient`]).
    DaemonHttp {
        session_id: String,
        daemon_url: String,
        session_token: String,
        daemon_instance_id: String,
    },
    /// Direct LiveKit RPC to a *remote* daemon's `ExecToolService` — a split session, where the
    /// agent runs on one host and its worktree lives on another
    /// (docs/ft/daemon/remote-managed-worktree.md).
    LiveKit {
        url: String,
        room: String,
        token: String,
        server_identity: String,
        session_id: String,
        session_token: String,
        daemon_instance_id: String,
    },
    /// Not a transport but the absence of a usable one: some of the LiveKit variables are set and
    /// the rest are not.
    ///
    /// Carried as an outcome rather than reported as "nothing configured" because the two are not
    /// the same failure. The LiveKit variables are only ever exported for a split session, whose
    /// worktree is on another host — `split_remote_tool_env` leaves `daemon_url` empty for exactly
    /// that reason — so there is no relay on this side to fall through to, and a stray one would
    /// answer from the wrong host's filesystem. Naming the missing variable is the only answer that
    /// points at the fault.
    IncompleteLiveKit { missing: Vec<&'static str> },
}

/// The identity fields carried in every `ExecuteToolRequest`.
///
/// The sandbox socket implies identity by the connection itself and leaves these empty. A remote
/// daemon has no such context: it resolves the worktree from `session_id` against its *own*
/// sessions base and authenticates `session_token`, so an empty envelope would find no worktree
/// and no user.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionToolEnvelope {
    pub session_id: String,
    pub session_token: String,
    pub daemon_instance_id: String,
}

/// The longest a tool call bound for a remote daemon may block.
///
/// Nothing on this path bounds a call: `RpcClient::call_server_stream` carries no deadline and
/// [`dispatch_via_streaming_rpc`] awaits its frames unbounded, so a codebase daemon that dies after
/// the request is published hangs the tool forever rather than failing it (docs/dev/TODO.md, "No
/// LiveKit RPC call has a client-side deadline"). Short blocks are what keeps that bounded in
/// practice: the agent drives long work through the background-job protocol and polls in slices, so
/// a dead peer shows up as one poll that never returns rather than as a session that simply stopped.
///
/// The budget also stays clear of the 30s idle deadline a *forwarded* stream imposes
/// (`PEER_FORWARD_STREAM_IDLE_TIMEOUT`). Nothing forwards today — `tddy-tools` joins the room and
/// calls the codebase daemon's participant directly, which routes the call locally — but a request
/// ever routed through a third daemon would need that margin, and 10s of it is the round trip.
pub const MAX_REMOTE_BLOCK_MS: u64 = 20_000;

/// What the remote tool engine blocks for when a call names no block time.
///
/// Mirrored from `tddy_tool_engine`'s `tool_await` and `tool_shell` (`unwrap_or(30_000)`);
/// this crate does not depend on that crate. It matters here because it sits *above* the ceiling:
/// the two request shapes an agent actually emits — `Await {job_id}` and `Shell {command}` — carry
/// no block time at all, so the ceiling has to be written in rather than left to the engine.
const REMOTE_ENGINE_DEFAULT_BLOCK_MS: u64 = 30_000;

/// The engine's default is what an unclamped call would block for, so a ceiling at or above it would
/// be no ceiling at all in the case that actually happens.
const _: () = assert!(MAX_REMOTE_BLOCK_MS < REMOTE_ENGINE_DEFAULT_BLOCK_MS);

/// Cap a requested block time at [`MAX_REMOTE_BLOCK_MS`].
///
/// A ceiling only: `0` means "return the job's status immediately" and raising it would turn every
/// status poll into a blocking call.
pub fn clamp_remote_block_ms(requested: u64) -> u64 {
    requested.min(MAX_REMOTE_BLOCK_MS)
}

/// The argument each tool blocks on, in the order the remote engine reads them.
///
/// `tool_await` takes `timeout_ms` and falls back to `block_until_ms` only when the first key is
/// *absent*; `tool_shell` reads `block_until_ms` alone. Every other tool returns as soon as it has
/// its answer and carries no block time to cap.
fn remote_block_arg_keys(tool_name: &str) -> &'static [&'static str] {
    match tool_name {
        "Await" => &["timeout_ms", "block_until_ms"],
        "Shell" => &["block_until_ms"],
        // A wait holds the call for up to its `timeout_ms` (#home-inbox 7/8), so it needs the same
        // ceiling as `Await`: a minute-long wait sits past every transport's deadline.
        "WaitProjectNotifications" => &["timeout_ms"],
        _ => &[],
    }
}

/// Read a block time the way the remote engine would if it read every JSON number.
///
/// JSON has a single number type, so a serializer that emitted `30000.0` sent the same request as
/// one that emitted `30000` — the engine's `as_i64` rejects the first and falls back to its default,
/// which is precisely the block being capped. A negative value asks for an effectively unbounded
/// block (the engine casts it to `u64`), so it is treated as the largest request there is and lands
/// on the ceiling like any other. Anything that is not a number is not a request at all.
fn requested_block_ms(value: &serde_json::Value) -> Option<u64> {
    if let Some(requested) = value.as_u64() {
        return Some(requested);
    }
    if let Some(requested) = value.as_i64() {
        // Only negatives reach here; `as_u64` took the rest.
        debug_assert!(requested < 0);
        return Some(u64::MAX);
    }
    let requested = value.as_f64()?;
    if requested.is_nan() {
        return None;
    }
    if requested < 0.0 {
        return Some(u64::MAX);
    }
    // A float-to-integer cast saturates, so an infinity arrives as the same unbounded ask.
    Some(requested as u64)
}

/// Cap the block time of a tool call bound for a remote daemon.
///
/// The value is not merely clamped when present but *written in* when it is not: an absent key is
/// the common case, and it blocks for [`REMOTE_ENGINE_DEFAULT_BLOCK_MS`] rather than for nothing.
/// The same holds for a value the engine cannot read — it ignores it and applies that default too,
/// so a rewrite loses no request that was ever going to be honoured.
///
/// Only the one key the engine will actually read is touched. Filling in `timeout_ms` beside an
/// explicit `block_until_ms: 0` would override a status poll with a blocking call, since the engine
/// stops at the first key it finds.
pub fn clamp_remote_blocking_args(tool_name: &str, args: &serde_json::Value) -> serde_json::Value {
    let keys = remote_block_arg_keys(tool_name);
    let mut clamped = args.clone();
    let Some(fields) = clamped.as_object_mut() else {
        // Arguments that are not an object carry neither `command` nor `job_id`, so the call fails
        // in the engine before it blocks on anything.
        return clamped;
    };
    let Some(key) = keys
        .iter()
        .find(|key| fields.contains_key(**key))
        .or_else(|| keys.first())
    else {
        return clamped;
    };
    let requested = fields
        .get(*key)
        .and_then(requested_block_ms)
        .unwrap_or(REMOTE_ENGINE_DEFAULT_BLOCK_MS);
    fields.insert((*key).to_string(), clamp_remote_block_ms(requested).into());
    clamped
}

/// Every variable the LiveKit transport needs, reported missing in this order.
const LIVEKIT_ENV_KEYS: [&str; 5] = [
    "TDDY_REMOTE_LIVEKIT_URL",
    "TDDY_REMOTE_LIVEKIT_ROOM",
    "TDDY_REMOTE_LIVEKIT_TOKEN",
    "TDDY_REMOTE_SERVER_IDENTITY",
    "TDDY_REMOTE_SESSION_ID",
];

/// The variables that mean "this agent's worktree is on another host".
///
/// A narrower set than [`LIVEKIT_ENV_KEYS`]: `TDDY_REMOTE_SESSION_ID` is shared with the HTTP relay
/// and `TDDY_REMOTE_SERVER_IDENTITY` says nothing about LiveKit on its own, so an ordinary remote
/// session would look half-configured if either counted as intent.
const LIVEKIT_INTENT_ENV_KEYS: [&str; 3] = [
    "TDDY_REMOTE_LIVEKIT_URL",
    "TDDY_REMOTE_LIVEKIT_ROOM",
    "TDDY_REMOTE_LIVEKIT_TOKEN",
];

/// Detect which transport is configured for session tool dispatch.
pub fn detect_session_tool_transport() -> Option<SessionToolTransport> {
    if let Some(socket_path) = std::env::var_os("TDDY_SANDBOX_TOOL_IPC") {
        return Some(SessionToolTransport::SandboxIpc {
            socket_path: PathBuf::from(socket_path),
        });
    }
    // Every field is required. When some are set and some are not, this is a split session with a
    // broken environment, not a session with an HTTP relay: reporting what is missing is the only
    // answer that names the fault (see `SessionToolTransport::IncompleteLiveKit`).
    let configured = LIVEKIT_ENV_KEYS.map(non_empty_env);
    let missing: Vec<&'static str> = LIVEKIT_ENV_KEYS
        .iter()
        .zip(configured.iter())
        .filter(|(_, value)| value.is_none())
        .map(|(key, _)| *key)
        .collect();
    if let [Some(url), Some(room), Some(token), Some(server_identity), Some(session_id)] =
        configured
    {
        return Some(SessionToolTransport::LiveKit {
            url,
            room,
            token,
            server_identity,
            session_id,
            session_token: std::env::var("TDDY_REMOTE_SESSION_TOKEN").unwrap_or_default(),
            daemon_instance_id: std::env::var("TDDY_REMOTE_DAEMON_INSTANCE_ID").unwrap_or_default(),
        });
    }
    if LIVEKIT_INTENT_ENV_KEYS
        .iter()
        .any(|key| non_empty_env(key).is_some())
    {
        return Some(SessionToolTransport::IncompleteLiveKit { missing });
    }
    // An embedded daemon serves no HTTP, so it exports a socket instead of a URL. Checked before
    // the HTTP arm because a host may export both while migrating, and the socket is the one that
    // works: it reaches the daemon in this very process tree rather than a port nothing listens on.
    if let (Some(session_id), Some(socket_path)) = (
        non_empty_env("TDDY_REMOTE_SESSION_ID"),
        non_empty_env("TDDY_REMOTE_DAEMON_SOCKET"),
    ) {
        return Some(SessionToolTransport::DaemonUds {
            socket_path: PathBuf::from(socket_path),
            session_id,
            session_token: std::env::var("TDDY_REMOTE_SESSION_TOKEN").unwrap_or_default(),
            daemon_instance_id: std::env::var("TDDY_REMOTE_DAEMON_INSTANCE_ID").unwrap_or_default(),
        });
    }
    // Blank counts as unset here too: `RemoteToolEnv::env_pairs` always exports
    // `TDDY_REMOTE_DAEMON_URL`, and a split session exports it empty. Taken as a URL it produced
    // "relay connection error: relative URL without a base" on every tool call — an error naming
    // the relay for a LiveKit misconfiguration.
    if let (Some(session_id), Some(daemon_url)) = (
        non_empty_env("TDDY_REMOTE_SESSION_ID"),
        non_empty_env("TDDY_REMOTE_DAEMON_URL"),
    ) {
        return Some(SessionToolTransport::DaemonHttp {
            session_id,
            daemon_url,
            session_token: std::env::var("TDDY_REMOTE_SESSION_TOKEN").unwrap_or_default(),
            daemon_instance_id: std::env::var("TDDY_REMOTE_DAEMON_INSTANCE_ID").unwrap_or_default(),
        });
    }
    None
}

/// Format an MCP tool result string from a daemon `ExecuteToolResponse` body.
///
/// On success returns `result_json` verbatim. On error returns a JSON object with
/// `error` and `is_error: true`.
pub fn format_tool_dispatch_result(body: &serde_json::Value) -> String {
    if body
        .get("is_error")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        serde_json::json!({
            "error": body
                .get("error_message")
                .and_then(|v| v.as_str())
                .unwrap_or("relay error"),
            "is_error": true
        })
        .to_string()
    } else {
        body.get("result_json")
            .and_then(|v| v.as_str())
            .unwrap_or("{}")
            .to_string()
    }
}

/// The answer to a tool call whose split session cannot reach its worktree.
///
/// Names the variables rather than the transport: the symptom an operator sees is a tool call
/// failing, and every other wording would send them looking at LiveKit, the relay or the daemon
/// instead of at the environment the agent was spawned with.
fn incomplete_livekit_error(missing: &[&str]) -> String {
    serde_json::json!({
        "error": incomplete_livekit_reason(missing),
        "is_error": true
    })
    .to_string()
}

/// The sentence [`incomplete_livekit_error`] carries, for a caller that wants the reason itself.
pub(crate) fn incomplete_livekit_reason(missing: &[&str]) -> String {
    format!(
        "remote worktree dispatch is misconfigured: a LiveKit environment is set but {} \
         {} empty or unset",
        missing.join(", "),
        if missing.len() == 1 { "is" } else { "are" }
    )
}

/// What a tool call or an approval reports when no remote route is configured.
pub(crate) const NOT_CONFIGURED_REASON: &str =
    "remote toolset not configured: TDDY_REMOTE_SESSION_ID and TDDY_REMOTE_DAEMON_URL must be set";

fn not_configured_error() -> String {
    serde_json::json!({
        "error": NOT_CONFIGURED_REASON,
        "is_error": true
    })
    .to_string()
}

/// Dispatch a dynamic tool call to the session daemon (sandbox IPC, HTTP, or LiveKit RPC).
pub async fn dispatch_session_tool(tool_name: &str, args: serde_json::Value) -> String {
    let Some(transport) = detect_session_tool_transport() else {
        return not_configured_error();
    };
    match transport {
        SessionToolTransport::SandboxIpc { socket_path } => {
            dispatch_via_sandbox_ipc(&socket_path, tool_name, &args).await
        }
        SessionToolTransport::DaemonUds {
            socket_path,
            session_id,
            session_token,
            daemon_instance_id,
        } => {
            dispatch_via_daemon_uds(
                &socket_path,
                &SessionToolEnvelope {
                    session_id,
                    session_token,
                    daemon_instance_id,
                },
                tool_name,
                &args,
            )
            .await
        }
        SessionToolTransport::DaemonHttp {
            session_id,
            daemon_url,
            session_token,
            daemon_instance_id,
        } => {
            dispatch_via_daemon_http(
                &daemon_url,
                &session_id,
                &session_token,
                &daemon_instance_id,
                tool_name,
                &args,
            )
            .await
        }
        SessionToolTransport::LiveKit {
            url,
            room,
            token,
            server_identity,
            session_id,
            session_token,
            daemon_instance_id,
        } => {
            // Only this transport can hang: the sandbox socket and the HTTP relay both fail a dead
            // peer, so shortening their blocks would cost time and buy nothing.
            let args = clamp_remote_blocking_args(tool_name, &args);
            let key = LiveKitRoomKey {
                url,
                room,
                token,
                server_identity,
            };
            let envelope = SessionToolEnvelope {
                session_id,
                session_token,
                daemon_instance_id,
            };
            dispatch_via_livekit(&key, &envelope, tool_name, &args).await
        }
        SessionToolTransport::IncompleteLiveKit { missing } => incomplete_livekit_error(&missing),
    }
}

pub use crate::room_transport::{
    install_room_connector, room_session_via, LiveKitRoomCache, LiveKitRoomKey, LiveKitSession,
};
use crate::rpc_transports::dispatch_via_daemon_http;
use crate::rpc_transports::dispatch_via_daemon_uds;

pub use crate::rpc_transports::{
    connect_sandbox_ipc, dispatch_via_rpc_transport, dispatch_via_sandbox_ipc,
    dispatch_via_streaming_rpc, livekit_session,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::room_transport::worktree_activity_line;

    #[test]
    fn a_broadcast_worktree_event_renders_as_the_line_the_agents_log_carries() {
        // Given the bytes of a commit event as the facilitating daemon broadcast them
        let payload = {
            use prost::Message as _;
            tddy_service::proto::worktree_activity::WorktreeActivityEvent {
                kind: tddy_service::proto::worktree_activity::WorktreeActivityKind::Commit as i32,
                seq: 4,
                at_unix_ms: 1_760_000_000_000,
                head_commit: "abcdef1234567890abcdef1234567890abcdef12".to_string(),
                changed_files: 0,
                lines_added: 0,
                lines_removed: 0,
            }
            .encode_to_vec()
        };

        // When the receiver turns the payload into its log line
        let line = worktree_activity_line(&payload);

        // Then a split agent's process leaves behind the same sentence every other receiver does —
        // which is the whole of what subscribing to the topic does for it today
        assert_eq!(
            line.expect("a well-formed event must decode"),
            "worktree activity: commit seq=4 head=abcdef1"
        );
    }

    #[test]
    fn a_payload_that_is_not_an_activity_event_is_reported_rather_than_logged_as_one() {
        // Given bytes that are not a `WorktreeActivityEvent` — what a newer publisher, or a topic
        // collision, would put on the wire
        let payload = b"\xff\xff not protobuf at all";

        // When the receiver tries to turn them into a log line
        let line = worktree_activity_line(payload);

        // Then it says so, so the receiver can warn. A fire-and-forget topic has no reply to fail,
        // so a payload silently dropped here would be indistinguishable from a publisher that went
        // quiet.
        line.expect_err("an undecodable payload must not produce a line");
    }

    #[test]
    fn format_tool_dispatch_result_returns_result_json_on_success() {
        // Given
        let body = serde_json::json!({
            "result_json": r#"{"path":"README.md"}"#,
            "is_error": false
        });

        // When
        let out = format_tool_dispatch_result(&body);

        // Then
        assert_eq!(out, r#"{"path":"README.md"}"#);
    }

    #[test]
    fn format_tool_dispatch_result_returns_error_object_on_failure() {
        // Given
        let body = serde_json::json!({
            "is_error": true,
            "error_message": "permission denied"
        });

        // When
        let out = format_tool_dispatch_result(&body);
        let parsed: serde_json::Value = serde_json::from_str(&out).expect("json");

        // Then
        assert_eq!(parsed["is_error"], true);
        assert_eq!(parsed["error"], "permission denied");
    }

    #[test]
    #[serial_test::serial]
    fn detect_transport_prefers_sandbox_ipc_over_remote() {
        // Given
        std::env::set_var("TDDY_SANDBOX_TOOL_IPC", "/tmp/tddy-tool-ipc.sock");
        std::env::set_var("TDDY_REMOTE_SESSION_ID", "remote-session");
        std::env::set_var("TDDY_REMOTE_DAEMON_URL", "http://127.0.0.1:8080");

        // When
        let transport = detect_session_tool_transport().expect("transport");

        // Then
        assert_eq!(
            transport,
            SessionToolTransport::SandboxIpc {
                socket_path: PathBuf::from("/tmp/tddy-tool-ipc.sock")
            }
        );

        std::env::remove_var("TDDY_SANDBOX_TOOL_IPC");
        std::env::remove_var("TDDY_REMOTE_SESSION_ID");
        std::env::remove_var("TDDY_REMOTE_DAEMON_URL");
    }
}
