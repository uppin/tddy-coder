//! Generic session tool dispatch — forwards MCP tool calls to `tddy-daemon` via sandbox IPC,
//! direct HTTP, or LiveKit RPC to a remote daemon, depending on environment.

use std::path::PathBuf;

pub use tddy_sandbox::session_id_from_env;

/// How `tddy-tools --mcp` reaches the daemon's `ExecuteTool` handler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionToolTransport {
    /// In-jail MCP → unix socket → sandbox-runner → SessionChannel → host daemon.
    SandboxIpc { socket_path: PathBuf },
    /// Direct HTTP Connect POST to `ConnectionService/ExecuteTool`.
    DaemonHttp {
        session_id: String,
        daemon_url: String,
        session_token: String,
        daemon_instance_id: String,
    },
    /// Direct LiveKit RPC to a *remote* daemon's `ConnectionService` — a split session, where the
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

/// The longest a remote `Await` may block.
///
/// A forwarded LiveKit stream is killed after `PEER_FORWARD_STREAM_IDLE_TIMEOUT` (30s) without a
/// frame, and a stalled stream is reported as an *error* rather than a clean end — so a tool
/// blocking past that deadline surfaces as a transport failure, the hardest kind of error to
/// attribute. The remaining 10s is headroom for the round trip itself.
pub const MAX_REMOTE_AWAIT_BLOCK_MS: u64 = 20_000;

/// Cap a requested `Await` block time at [`MAX_REMOTE_AWAIT_BLOCK_MS`].
///
/// A ceiling only: `0` means "return the job's status immediately" and raising it would turn every
/// status poll into a blocking call.
pub fn clamp_await_block_ms(requested: u64) -> u64 {
    requested.min(MAX_REMOTE_AWAIT_BLOCK_MS)
}

/// Cap the block time of an `Await` bound for a remote daemon.
///
/// `tool_await` accepts it as either `timeout_ms` or `block_until_ms`, so both are capped. A
/// negative value asks for an effectively unbounded block (the engine casts it to `u64`), which no
/// forwarded stream can carry — it lands on the ceiling like any other over-long request.
fn clamp_remote_await_args(tool_name: &str, args: &serde_json::Value) -> serde_json::Value {
    if tool_name != "Await" {
        return args.clone();
    }
    let mut clamped = args.clone();
    let Some(fields) = clamped.as_object_mut() else {
        return clamped;
    };
    for key in ["timeout_ms", "block_until_ms"] {
        let Some(requested) = fields.get(key).and_then(|v| v.as_i64()) else {
            continue;
        };
        let requested = if requested < 0 {
            u64::MAX
        } else {
            requested as u64
        };
        fields.insert(key.to_string(), clamp_await_block_ms(requested).into());
    }
    clamped
}

/// Read an environment variable, treating an empty value as unset — a blank join token or server
/// identity configures nothing.
fn non_empty_env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

/// Detect which transport is configured for session tool dispatch.
pub fn detect_session_tool_transport() -> Option<SessionToolTransport> {
    if let Some(socket_path) = std::env::var_os("TDDY_SANDBOX_TOOL_IPC") {
        return Some(SessionToolTransport::SandboxIpc {
            socket_path: PathBuf::from(socket_path),
        });
    }
    // Every field is required: a half-configured LiveKit environment must fall through to the HTTP
    // relay rather than be selected and then fail at connect.
    if let (Some(url), Some(room), Some(token), Some(server_identity), Some(session_id)) = (
        non_empty_env("TDDY_REMOTE_LIVEKIT_URL"),
        non_empty_env("TDDY_REMOTE_LIVEKIT_ROOM"),
        non_empty_env("TDDY_REMOTE_LIVEKIT_TOKEN"),
        non_empty_env("TDDY_REMOTE_SERVER_IDENTITY"),
        non_empty_env("TDDY_REMOTE_SESSION_ID"),
    ) {
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
    let session_id = std::env::var("TDDY_REMOTE_SESSION_ID").ok();
    let daemon_url = std::env::var("TDDY_REMOTE_DAEMON_URL").ok();
    if let (Some(session_id), Some(daemon_url)) = (session_id, daemon_url) {
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

fn not_configured_error() -> String {
    serde_json::json!({
        "error": "remote toolset not configured: TDDY_REMOTE_SESSION_ID and TDDY_REMOTE_DAEMON_URL must be set",
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
            // Only this transport rides a forwarded stream with an idle deadline; clamping the
            // sandbox and HTTP paths would shorten their blocks for no reason.
            let args = clamp_remote_await_args(tool_name, &args);
            let envelope = SessionToolEnvelope {
                session_id,
                session_token,
                daemon_instance_id,
            };
            dispatch_via_livekit(
                &url,
                &room,
                &token,
                &server_identity,
                &envelope,
                tool_name,
                &args,
            )
            .await
        }
    }
}

/// How long to wait for the remote daemon's RPC-server participant to appear in the room. It is
/// normally already joined; this only covers the window right after a daemon restart.
#[cfg(feature = "livekit")]
const SERVER_PARTICIPANT_WAIT: std::time::Duration = std::time::Duration::from_secs(10);

/// Forward a tool call to a remote daemon over LiveKit, addressed at its RPC-server participant.
///
/// The join token is scoped to `room` by the daemon that minted it, so the room is not re-derived
/// here — it is carried for diagnostics, where "which room did we join" is the first question.
///
/// TODO: connect the room once per process and reuse it across tool calls; today every call pays a
/// full room connect.
#[cfg(feature = "livekit")]
#[allow(clippy::too_many_arguments)]
pub async fn dispatch_via_livekit(
    url: &str,
    room: &str,
    token: &str,
    server_identity: &str,
    envelope: &SessionToolEnvelope,
    tool_name: &str,
    args: &serde_json::Value,
) -> String {
    use livekit::prelude::*;
    use std::sync::Arc;

    log::info!(
        target: "tddy_tools::session_tool_client",
        "dispatching {tool_name} to \"{server_identity}\" in room \"{room}\" at {url}"
    );
    let (connected_room, mut room_events) =
        match Room::connect(url, token, RoomOptions::default()).await {
            Ok(pair) => pair,
            Err(e) => {
                return serde_json::json!({
                    "error": format!("livekit room connect: {e}"),
                    "is_error": true
                })
                .to_string();
            }
        };
    let connected_room = Arc::new(connected_room);

    let target: ParticipantIdentity = server_identity.to_string().into();
    if !connected_room.remote_participants().contains_key(&target) {
        let appeared = tokio::time::timeout(SERVER_PARTICIPANT_WAIT, async {
            while let Some(event) = room_events.recv().await {
                if let RoomEvent::ParticipantConnected(participant) = event {
                    if participant.identity() == target {
                        return;
                    }
                }
            }
        })
        .await;
        if appeared.is_err() {
            return serde_json::json!({
                "error": format!(
                    "timed out waiting for remote daemon participant \"{server_identity}\" in room \"{room}\""
                ),
                "is_error": true
            })
            .to_string();
        }
    }

    let rpc_events = connected_room.subscribe();
    let client: Arc<dyn tddy_rpc::RpcClientTransport> = Arc::new(
        tddy_livekit::RpcClient::new_shared(connected_room, target, rpc_events),
    );
    dispatch_via_rpc_transport(&client, envelope, tool_name, args).await
}

/// Without the `livekit` feature the SDK is not linked, so a split session cannot reach its
/// worktree at all — it says so rather than degrading to a transport that would target the wrong
/// host's filesystem.
#[cfg(not(feature = "livekit"))]
#[allow(clippy::too_many_arguments)]
pub async fn dispatch_via_livekit(
    _url: &str,
    _room: &str,
    _token: &str,
    _server_identity: &str,
    _envelope: &SessionToolEnvelope,
    _tool_name: &str,
    _args: &serde_json::Value,
) -> String {
    serde_json::json!({
        "error": "remote worktree dispatch requires the 'livekit' cargo feature; \
                  rebuild with: cargo build -p tddy-tools --features livekit",
        "is_error": true
    })
    .to_string()
}

/// `dispatch_via_sandbox_ipc`'s stdio-RPC connection never receives inbound calls from the
/// sandbox-runner — any request here would be a bug, so it fails loudly rather than silently
/// no-op'ing.
struct NoCallbackToolService;

#[async_trait::async_trait]
impl tddy_rpc::RpcService for NoCallbackToolService {
    async fn handle_rpc(
        &self,
        service: &str,
        method: &str,
        _message: &tddy_rpc::RpcMessage,
    ) -> tddy_rpc::RpcResult {
        tddy_rpc::RpcResult::Unary(Err(tddy_rpc::Status::unimplemented(format!(
            "tddy-tools hosts no callback service, got {service}/{method}"
        ))))
    }
}

/// Forward a tool call over the sandbox unix IPC socket, using `tddy-rpc`'s length-prefixed
/// framing (`connection.ConnectionService/ExecuteTool`) rather than the socket path itself
/// carrying any particular wire format — the socket is just a duplex byte stream `tddy-stdio`'s
/// `StdioEndpoint` can wrap like any other (see `StdioEndpoint::from_duplex`).
pub async fn dispatch_via_sandbox_ipc(
    socket_path: &std::path::Path,
    tool_name: &str,
    args: &serde_json::Value,
) -> String {
    let stream = match tokio::net::UnixStream::connect(socket_path).await {
        Ok(s) => s,
        Err(e) => {
            return serde_json::json!({"error": format!("tool ipc connect: {e}"), "is_error": true})
                .to_string();
        }
    };
    let (read_half, write_half) = tokio::io::split(stream);
    let (client, endpoint) =
        tddy_stdio::StdioEndpoint::from_duplex(read_half, write_half, NoCallbackToolService);
    tokio::spawn(endpoint.run());
    let client: std::sync::Arc<dyn tddy_rpc::RpcClientTransport> = client;
    // The socket itself identifies the session to the sandbox-runner, so the envelope stays empty.
    dispatch_via_rpc_transport(&client, &SessionToolEnvelope::default(), tool_name, args).await
}

/// Forward a tool call via HTTP to `ConnectionService/ExecuteTool`.
pub async fn dispatch_via_daemon_http(
    daemon_url: &str,
    session_id: &str,
    session_token: &str,
    daemon_instance_id: &str,
    tool_name: &str,
    args: &serde_json::Value,
) -> String {
    let req_body = serde_json::json!({
        "session_token": session_token,
        "session_id": session_id,
        "tool_name": tool_name,
        "args_json": args.to_string(),
        "daemon_instance_id": daemon_instance_id,
    });

    let url = format!(
        "{}/connection.ConnectionService/ExecuteTool",
        daemon_url.trim_end_matches('/')
    );

    let client = reqwest::Client::new();
    match client
        .post(&url)
        .header("content-type", "application/json")
        .json(&req_body)
        .send()
        .await
    {
        Ok(resp) => match resp.json::<serde_json::Value>().await {
            Ok(body) => format_tool_dispatch_result(&body),
            Err(e) => {
                serde_json::json!({"error": format!("relay parse error: {e}"), "is_error": true})
                    .to_string()
            }
        },
        Err(e) => {
            serde_json::json!({"error": format!("relay connection error: {e}"), "is_error": true})
                .to_string()
        }
    }
}

/// Forward a tool call over an already-connected RPC transport (`tddy-stdio`'s `StdioRpcClient`
/// over the sandbox socket, `tddy-livekit`'s `RpcClient` to a remote daemon), calling
/// `connection.ConnectionService/ExecuteTool`.
///
/// Transport-agnostic by construction: what differs between the two is the request envelope, not
/// the call.
pub async fn dispatch_via_rpc_transport(
    client: &std::sync::Arc<dyn tddy_rpc::RpcClientTransport>,
    envelope: &SessionToolEnvelope,
    tool_name: &str,
    args: &serde_json::Value,
) -> String {
    use prost::Message;
    use tddy_service::proto::connection::{ExecuteToolRequest, ExecuteToolResponse};

    let request = ExecuteToolRequest {
        session_token: envelope.session_token.clone(),
        session_id: envelope.session_id.clone(),
        tool_name: tool_name.to_string(),
        args_json: args.to_string(),
        daemon_instance_id: envelope.daemon_instance_id.clone(),
    };
    let response_bytes = match client
        .call_unary(
            "connection.ConnectionService",
            "ExecuteTool",
            request.encode_to_vec(),
        )
        .await
    {
        Ok(bytes) => bytes,
        Err(e) => {
            return serde_json::json!({"error": format!("tool rpc call: {e}"), "is_error": true})
                .to_string();
        }
    };
    let response = match ExecuteToolResponse::decode(response_bytes.as_slice()) {
        Ok(resp) => resp,
        Err(e) => {
            return serde_json::json!({
                "error": format!("tool rpc decode response: {e}"),
                "is_error": true
            })
            .to_string();
        }
    };
    if response.is_error {
        serde_json::json!({"error": response.error_message, "is_error": true}).to_string()
    } else {
        response.result_json
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
