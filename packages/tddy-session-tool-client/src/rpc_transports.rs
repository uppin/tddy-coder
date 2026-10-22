use crate::format_tool_dispatch_result;
use crate::room_transport::installed_room_connector;
use crate::room_transport::livekit_room_cache;
use crate::room_transport::{room_session_via, LiveKitRoomKey, LiveKitSession};
use crate::SessionToolEnvelope;
use std::sync::Arc;

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
/// framing (`exec_tools.ExecToolService/ExecuteTool`) rather than the socket path itself
/// carrying any particular wire format — the socket is just a duplex byte stream `tddy-stdio`'s
/// `StdioEndpoint` can wrap like any other (see `StdioEndpoint::from_duplex`).
pub async fn dispatch_via_sandbox_ipc(
    socket_path: &std::path::Path,
    tool_name: &str,
    args: &serde_json::Value,
) -> String {
    let client = match connect_sandbox_ipc(socket_path).await {
        Ok(client) => client,
        Err(e) => {
            return serde_json::json!({"error": e, "is_error": true}).to_string();
        }
    };
    // The socket itself identifies the session to the sandbox-runner, so the envelope stays empty.
    dispatch_via_rpc_transport(&client, &SessionToolEnvelope::default(), tool_name, args).await
}

/// Forward a tool call to this host's embedded daemon over its agent-facing unix socket.
///
/// Same framing and the same connection-per-call rule as [`dispatch_via_sandbox_ipc`] — the socket
/// is a duplex byte stream either way — and differs only in carrying a real
/// [`SessionToolEnvelope`], because nothing about this connection tells the daemon which session
/// is calling.
pub async fn dispatch_via_daemon_uds(
    socket_path: &std::path::Path,
    envelope: &SessionToolEnvelope,
    tool_name: &str,
    args: &serde_json::Value,
) -> String {
    let client = match connect_sandbox_ipc(socket_path).await {
        Ok(client) => client,
        Err(e) => {
            return serde_json::json!({"error": e, "is_error": true}).to_string();
        }
    };
    dispatch_via_rpc_transport(&client, envelope, tool_name, args).await
}

/// Open one sandbox-IPC connection and return the RPC transport over it.
///
/// A connection per caller, which is what the socket is for: the sandbox-runner accepts each one and
/// bridges it to the host daemon, and a dispatch that shared a connection with a long-lived server
/// stream would interleave with frames it does not read.
pub async fn connect_sandbox_ipc(
    socket_path: &std::path::Path,
) -> Result<Arc<dyn tddy_rpc::RpcClientTransport>, String> {
    let stream = tokio::net::UnixStream::connect(socket_path)
        .await
        .map_err(|e| format!("tool ipc connect: {e}"))?;
    let (read_half, write_half) = tokio::io::split(stream);
    let (client, endpoint) = tddy_stdio::StdioEndpoint::from_duplex(
        read_half,
        write_half,
        NoCallbackToolService,
        tddy_rpc::RequestTransport::UnixSocket,
    );
    tokio::spawn(endpoint.run());
    Ok(client)
}

/// The held connection to the daemon `key` names, connecting it if this is the first caller.
///
/// Public so the roster stream rides the *same* room connection tool dispatch uses: one join, one
/// request-id space and one response loop per destination — which is the property
/// [`LiveKitRoomCache`] exists to keep.
pub async fn livekit_session(key: &LiveKitRoomKey) -> Result<Arc<LiveKitSession>, String> {
    room_session_via(
        livekit_room_cache(),
        key,
        installed_room_connector().as_ref(),
    )
    .await
}

/// Forward a tool call via HTTP to `ExecToolService/ExecuteTool`.
pub async fn dispatch_via_daemon_http(
    daemon_url: &str,
    session_id: &str,
    session_token: &str,
    daemon_instance_id: &str,
    tool_name: &str,
    args: &serde_json::Value,
) -> String {
    let envelope = SessionToolEnvelope {
        session_id: session_id.to_string(),
        session_token: session_token.to_string(),
        daemon_instance_id: daemon_instance_id.to_string(),
    };
    dispatch_request_via_daemon_http(
        daemon_url,
        &execute_tool_request(&envelope, tool_name, args),
    )
    .await
}

/// [`dispatch_via_daemon_http`] for a request already built.
pub(crate) async fn dispatch_request_via_daemon_http(
    daemon_url: &str,
    request: &tddy_service::proto::exec_tools::ExecuteToolRequest,
) -> String {
    let req_body = serde_json::json!({
        "session_token": request.session_token,
        "session_id": request.session_id,
        "tool_name": request.tool_name,
        "args_json": request.args_json,
        "daemon_instance_id": request.daemon_instance_id,
        "conversation_id": request.conversation_id,
    });

    let url = format!(
        "{}/exec_tools.ExecToolService/ExecuteTool",
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
pub(crate) fn execute_tool_request(
    envelope: &SessionToolEnvelope,
    tool_name: &str,
    args: &serde_json::Value,
) -> tddy_service::proto::exec_tools::ExecuteToolRequest {
    tddy_service::proto::exec_tools::ExecuteToolRequest {
        session_token: envelope.session_token.clone(),
        session_id: envelope.session_id.clone(),
        tool_name: tool_name.to_string(),
        args_json: args.to_string(),
        daemon_instance_id: envelope.daemon_instance_id.clone(),
        conversation_id: String::new(),
    }
}

/// Forward a tool call over an already-connected RPC transport (`tddy-stdio`'s `StdioRpcClient`
/// over the sandbox socket, `tddy-livekit`'s `RpcClient` to a remote daemon), calling
/// `exec_tools.ExecToolService/ExecuteTool`.
///
/// Transport-agnostic by construction: what differs between the two is the request envelope, not
/// the call.
pub async fn dispatch_via_rpc_transport(
    client: &std::sync::Arc<dyn tddy_rpc::RpcClientTransport>,
    envelope: &SessionToolEnvelope,
    tool_name: &str,
    args: &serde_json::Value,
) -> String {
    dispatch_request_via_rpc_transport(client, execute_tool_request(envelope, tool_name, args))
        .await
}

/// [`dispatch_via_rpc_transport`] for a request already built — the one a conversation's call
/// builds with its conversation named.
pub(crate) async fn dispatch_request_via_rpc_transport(
    client: &std::sync::Arc<dyn tddy_rpc::RpcClientTransport>,
    request: tddy_service::proto::exec_tools::ExecuteToolRequest,
) -> String {
    use prost::Message;
    use tddy_service::proto::exec_tools::ExecuteToolResponse;

    let response_bytes = match client
        .call_unary(
            "exec_tools.ExecToolService",
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
/// `exec_tools.ExecToolService/StreamExecuteTool` and reassembling its frames.
///
/// The unary sibling returns `result_json` as one string, which over LiveKit is chunk-framed past
/// `MAX_CHUNK_FRAME_BYTES` — and that reassembly is index-keyed and best-effort, so a lost frame
/// wedges the call permanently with no error. The streamed frames are bounded below that threshold
/// and the last one is marked, so a short result is *detectable* here rather than silently passed on
/// as a complete one.
///
/// Detectable once the stream *ends*, which is as far as this goes: nothing between here and the
/// remote daemon bounds how long that takes. `tddy-tools` calls the codebase daemon's participant
/// directly and that daemon routes the call locally, so no forwarding hop — and none of the idle
/// deadlines a forwarded stream has — is involved. A peer that dies after the request is published
/// leaves the loop below waiting forever.
///
/// Every failure is logged at `error` as well as returned. The returned JSON reaches the model and
/// nothing else, and `tddy-tools`' default filter is `warn` — so without this a split session whose
/// tool path is broken produces no evidence an operator can find. The session and daemon ids are
/// what identify *which* remote worktree the call was bound for.
pub async fn dispatch_via_streaming_rpc(
    client: &std::sync::Arc<dyn tddy_rpc::RpcClientTransport>,
    envelope: &SessionToolEnvelope,
    tool_name: &str,
    args: &serde_json::Value,
) -> String {
    dispatch_request_via_streaming_rpc(client, execute_tool_request(envelope, tool_name, args))
        .await
}

/// [`dispatch_via_streaming_rpc`] for a request already built.
pub(crate) async fn dispatch_request_via_streaming_rpc(
    client: &std::sync::Arc<dyn tddy_rpc::RpcClientTransport>,
    request: tddy_service::proto::exec_tools::ExecuteToolRequest,
) -> String {
    use prost::Message;
    use tddy_service::proto::exec_tools::ExecuteToolChunk;

    let (session_id, daemon_instance_id, tool_name) = (
        &request.session_id,
        &request.daemon_instance_id,
        &request.tool_name,
    );
    let mut frames = match client
        .call_server_stream(
            "exec_tools.ExecToolService",
            "StreamExecuteTool",
            request.encode_to_vec(),
        )
        .await
    {
        Ok(frames) => frames,
        Err(e) => {
            log::error!(
                target: "tddy_session_tool_client",
                "{tool_name} failed: StreamExecuteTool call to daemon \"{daemon_instance_id}\" \
                 for session {session_id} was not accepted: {e}"
            );
            return serde_json::json!({"error": format!("tool rpc call: {e}"), "is_error": true})
                .to_string();
        }
    };

    let mut result = Vec::new();
    // TODO(livekit-rpc-deadline): this await is unbounded, so a peer that stops sending hangs the
    // tool call rather than failing it. The fix belongs on the client — a deadline on
    // `tddy_livekit::RpcClient` / `tddy_rpc`'s `ClientEngine`, which long-lived streams must not
    // inherit — not a timeout wrapped around this loop. Recorded in docs/dev/TODO.md, "No LiveKit
    // RPC call has a client-side deadline". Until then, `MAX_REMOTE_BLOCK_MS` is what keeps a
    // hang to one poll rather than a whole session.
    while let Some(frame) = frames.recv().await {
        let frame = match frame {
            Ok(bytes) => bytes,
            Err(e) => {
                log::error!(
                    target: "tddy_session_tool_client",
                    "{tool_name} failed: StreamExecuteTool from daemon \"{daemon_instance_id}\" \
                     for session {session_id} errored after {} bytes: {e}",
                    result.len()
                );
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
                log::error!(
                    target: "tddy_session_tool_client",
                    "{tool_name} failed: undecodable frame ({} bytes) from daemon \
                     \"{daemon_instance_id}\" for session {session_id} — the two hosts disagree \
                     about ExecuteToolChunk: {e}",
                    frame.len()
                );
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
                log::error!(
                    target: "tddy_session_tool_client",
                    "{tool_name} failed: frames from daemon \"{daemon_instance_id}\" for session \
                     {session_id} did not reassemble as UTF-8: {e}"
                );
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
    log::error!(
        target: "tddy_session_tool_client",
        "{tool_name} failed: StreamExecuteTool from daemon \"{daemon_instance_id}\" for session \
         {session_id} ended after {} bytes without its final frame; the partial result was \
         discarded",
        result.len()
    );
    serde_json::json!({
        "error": format!(
            "tool result truncated: {tool_name} stream ended after {} bytes without its final frame",
            result.len()
        ),
        "is_error": true
    })
    .to_string()
}
