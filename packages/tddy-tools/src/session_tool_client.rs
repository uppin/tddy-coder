//! Generic session tool dispatch — forwards MCP tool calls to `tddy-daemon` via sandbox IPC,
//! direct HTTP, or LiveKit RPC to a remote daemon, depending on environment.

use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

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

/// Which remote daemon a cached LiveKit connection reaches.
///
/// Reuse is keyed by destination, `server_identity` included: handing back a client aimed at a
/// different daemon would execute the tool against the wrong host's filesystem. The token is part of
/// the key because it is what the room was joined with — a re-minted token is a different
/// connection, not the same one under a new name.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LiveKitRoomKey {
    pub url: String,
    pub room: String,
    pub token: String,
    pub server_identity: String,
}

/// One LiveKit connection per destination, held for the life of the process.
///
/// `dispatch_via_livekit` used to connect a room, issue one call and drop it — per tool call, so an
/// agent doing fifty `Read`s paid fifty connects. `tddy-tools --mcp` runs for the whole session, so
/// it can hold the connection instead.
///
/// The per-key [`tokio::sync::OnceCell`] is what makes that safe under concurrency: an MCP server
/// issues tool calls in parallel, and a cache that checked the map, released it and only then
/// connected would open a room per racing call. Initialisation happens *inside* the cell, so the
/// first caller connects and the rest await that same connect. A failed connect leaves the cell
/// empty, so one unlucky first call cannot poison the session.
#[derive(Default)]
pub struct LiveKitRoomCache {
    cells: Mutex<HashMap<LiveKitRoomKey, RoomClientCell>>,
}

/// One destination's connection slot: empty until a caller finishes connecting it, and still empty
/// if that connect failed.
type RoomClientCell = Arc<tokio::sync::OnceCell<Arc<LiveKitSession>>>;

/// A held connection to one remote daemon, plus the means to ask whether that daemon is still in
/// the room.
///
/// The presence check exists because holding the connection moves the participant wait from *every
/// call* to *first connect*. If the codebase daemon restarts mid-session, a call issued against the
/// cached client would publish to an identity nobody is listening on — and neither
/// `tddy_livekit::RpcClient` nor `tddy_rpc`'s engine carries a request deadline, so it would hang
/// rather than error. Hanging is strictly worse than the clear timeout the connect-per-call
/// behaviour produced, so the caller checks first.
pub struct LiveKitSession {
    transport: Arc<dyn tddy_rpc::RpcClientTransport>,
    peer_present: Box<dyn Fn() -> bool + Send + Sync>,
}

impl LiveKitSession {
    pub fn new(
        transport: Arc<dyn tddy_rpc::RpcClientTransport>,
        peer_present: Box<dyn Fn() -> bool + Send + Sync>,
    ) -> Self {
        Self {
            transport,
            peer_present,
        }
    }

    /// The RPC transport addressed at the remote daemon.
    pub fn transport(&self) -> &Arc<dyn tddy_rpc::RpcClientTransport> {
        &self.transport
    }

    /// Whether the remote daemon's RPC-server participant is still in the room.
    pub fn peer_present(&self) -> bool {
        (self.peer_present)()
    }
}

impl LiveKitRoomCache {
    /// Return the session for `key`, running `connect` only if this destination has no live one.
    pub async fn client_via<F, Fut>(
        &self,
        key: &LiveKitRoomKey,
        connect: F,
    ) -> Result<Arc<LiveKitSession>, String>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<Arc<LiveKitSession>, String>>,
    {
        // The map lock only hands out the cell; every await happens on the cell, so the lock is
        // never held across one.
        let cell = {
            let mut cells = self.cells.lock().expect("livekit room cache");
            Arc::clone(cells.entry(key.clone()).or_default())
        };
        cell.get_or_try_init(connect).await.cloned()
    }
}

/// How long to wait for the remote daemon's RPC-server participant to appear in the room. It is
/// normally already joined; this only covers the window right after a daemon restart.
#[cfg(feature = "livekit")]
const SERVER_PARTICIPANT_WAIT: std::time::Duration = std::time::Duration::from_secs(10);

/// The process-wide connection cache behind [`dispatch_via_livekit`].
#[cfg(feature = "livekit")]
fn livekit_room_cache() -> &'static LiveKitRoomCache {
    static CACHE: std::sync::OnceLock<LiveKitRoomCache> = std::sync::OnceLock::new();
    CACHE.get_or_init(LiveKitRoomCache::default)
}

/// Join `room` and return a client addressed at the remote daemon's RPC-server participant.
///
/// The returned client owns the `Arc<Room>`, so the connection lives exactly as long as the cache
/// keeps the client.
#[cfg(feature = "livekit")]
async fn connect_livekit_client(key: &LiveKitRoomKey) -> Result<Arc<LiveKitSession>, String> {
    use livekit::prelude::*;

    let LiveKitRoomKey {
        url,
        room,
        token,
        server_identity,
    } = key;
    let (connected_room, mut room_events) = Room::connect(url, token, RoomOptions::default())
        .await
        .map_err(|e| format!("livekit room connect: {e}"))?;
    let connected_room = Arc::new(connected_room);

    let target: ParticipantIdentity = server_identity.clone().into();
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
            return Err(format!(
                "timed out waiting for remote daemon participant \"{server_identity}\" in room \"{room}\""
            ));
        }
    }

    // Through the factory rather than building an RpcClient directly: it owns one ClientEngine and
    // one response loop per room, so vended clients share a request-id space instead of each
    // starting at 1 and leaking its own `room.subscribe()` loop. Harmless while every call had its
    // own short-lived room; caching the room is what would make it bite.
    let presence_room = Arc::clone(&connected_room);
    let presence_target = target.clone();
    Ok(Arc::new(LiveKitSession::new(
        Arc::new(tddy_livekit::LiveKitRpcClientFactory::for_room(connected_room).client(target)),
        Box::new(move || {
            presence_room
                .remote_participants()
                .contains_key(&presence_target)
        }),
    )))
}

/// Forward a tool call to a remote daemon over LiveKit, addressed at its RPC-server participant.
///
/// The join token is scoped to `room` by the daemon that minted it, so the room is not re-derived
/// here — it is carried for diagnostics, where "which room did we join" is the first question.
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
    log::info!(
        target: "tddy_tools::session_tool_client",
        "dispatching {tool_name} to \"{server_identity}\" in room \"{room}\" at {url}"
    );
    let key = LiveKitRoomKey {
        url: url.to_string(),
        room: room.to_string(),
        token: token.to_string(),
        server_identity: server_identity.to_string(),
    };
    let session = match livekit_room_cache()
        .client_via(&key, || connect_livekit_client(&key))
        .await
    {
        Ok(session) => session,
        Err(e) => {
            return serde_json::json!({"error": e, "is_error": true}).to_string();
        }
    };
    // Holding the connection moves the participant wait to first connect, so a daemon that
    // restarted since then would receive nothing — and with no request deadline anywhere below
    // this, the call would hang instead of failing. Checked per call for that reason.
    if !session.peer_present() {
        return serde_json::json!({
            "error": format!(
                "remote daemon participant \"{server_identity}\" is no longer in room \"{room}\""
            ),
            "is_error": true
        })
        .to_string();
    }
    // Streaming, not unary: this is the one transport whose messages are chunk-framed past
    // MAX_CHUNK_FRAME_BYTES, where a lost chunk frame wedges the call with no error at all.
    dispatch_via_streaming_rpc(session.transport(), envelope, tool_name, args).await
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

/// The request both the unary and the streaming call send. Shared so the two cannot drift: the
/// remote daemon resolves the worktree and authenticates from these fields alone, so a difference
/// between the transports would show up as a missing worktree rather than a wrong request.
fn execute_tool_request(
    envelope: &SessionToolEnvelope,
    tool_name: &str,
    args: &serde_json::Value,
) -> tddy_service::proto::connection::ExecuteToolRequest {
    tddy_service::proto::connection::ExecuteToolRequest {
        session_token: envelope.session_token.clone(),
        session_id: envelope.session_id.clone(),
        tool_name: tool_name.to_string(),
        args_json: args.to_string(),
        daemon_instance_id: envelope.daemon_instance_id.clone(),
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
    use tddy_service::proto::connection::ExecuteToolResponse;

    let request = execute_tool_request(envelope, tool_name, args);
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

/// Forward a tool call over an already-connected RPC transport, calling
/// `connection.ConnectionService/StreamExecuteTool` and reassembling its frames.
///
/// The unary sibling returns `result_json` as one string, which over LiveKit is chunk-framed past
/// `MAX_CHUNK_FRAME_BYTES` — and that reassembly is index-keyed and best-effort, so a lost frame
/// wedges the call permanently with no error. The streamed frames are bounded below that threshold
/// and the last one is marked, so a short result is *detectable* here rather than silently passed on
/// as a complete one.
pub async fn dispatch_via_streaming_rpc(
    client: &std::sync::Arc<dyn tddy_rpc::RpcClientTransport>,
    envelope: &SessionToolEnvelope,
    tool_name: &str,
    args: &serde_json::Value,
) -> String {
    use prost::Message;
    use tddy_service::proto::connection::ExecuteToolChunk;

    let request = execute_tool_request(envelope, tool_name, args);
    let mut frames = match client
        .call_server_stream(
            "connection.ConnectionService",
            "StreamExecuteTool",
            request.encode_to_vec(),
        )
        .await
    {
        Ok(frames) => frames,
        Err(e) => {
            return serde_json::json!({"error": format!("tool rpc call: {e}"), "is_error": true})
                .to_string();
        }
    };

    let mut result = Vec::new();
    while let Some(frame) = frames.recv().await {
        let frame = match frame {
            Ok(bytes) => bytes,
            Err(e) => {
                return serde_json::json!({
                    "error": format!("tool rpc stream: {e}"),
                    "is_error": true
                })
                .to_string();
            }
        };
        let frame = match ExecuteToolChunk::decode(frame.as_slice()) {
            Ok(frame) => frame,
            Err(e) => {
                return serde_json::json!({
                    "error": format!("tool rpc decode frame: {e}"),
                    "is_error": true
                })
                .to_string();
            }
        };
        result.extend_from_slice(&frame.result_chunk);
        if !frame.last {
            continue;
        }
        // A frame boundary may split a multi-byte character, so only the reassembled result is
        // required to be UTF-8; if it is not, the frames did not reassemble into what was sent.
        let result_json = match String::from_utf8(result) {
            Ok(text) => text,
            Err(e) => {
                return serde_json::json!({
                    "error": format!("tool result frames did not reassemble as UTF-8: {e}"),
                    "is_error": true
                })
                .to_string();
            }
        };
        // `job_id` and `job_running` are already inside `result_json`, exactly as with the unary
        // response — carrying them on the frame lets the daemon report them without parsing it.
        return format_tool_dispatch_result(&serde_json::json!({
            "result_json": result_json,
            "is_error": frame.is_error,
            "error_message": frame.error_message,
        }));
    }

    // The stream ended without its final frame. The bytes collected so far are a prefix of the
    // result, and returning them would hand the agent a half-read file that looks whole — the exact
    // failure this RPC exists to make visible.
    serde_json::json!({
        "error": format!(
            "tool result truncated: {tool_name} stream ended after {} bytes without its final frame",
            result.len()
        ),
        "is_error": true
    })
    .to_string()
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
