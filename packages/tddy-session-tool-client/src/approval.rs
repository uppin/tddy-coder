//! The approval a headless coding agent's permission prompt waits on: `ExecToolService/RequestApproval`,
//! over the transport tool calls use.
//!
//! The daemon holds the call open until the conversation answers the parked permission, and denies
//! it itself after its own bound. [`APPROVAL_CLIENT_BOUND`] is that bound plus a margin, so it fires
//! only when the daemon has gone silent, and a silent daemon is a refusal rather than a hung prompt.
//!
//! Nothing here is capped at [`crate::MAX_REMOTE_BLOCK_MS`]: that ceiling clamps the block time of
//! `Await` and `Shell` arguments, and no call on this path carries a deadline of its own.

use std::sync::Arc;
use std::time::Duration;

use prost::Message as _;
use tddy_rpc::RpcClientTransport;
use tddy_service::proto::exec_tools::{ApprovalRequest, ApprovalResponse};

use crate::room_transport::LiveKitRoomKey;
use crate::rpc_transports::{connect_sandbox_ipc, livekit_session};
use crate::{
    detect_session_tool_transport, incomplete_livekit_reason, SessionToolEnvelope,
    SessionToolTransport, NOT_CONFIGURED_REASON,
};

/// How long an approval waits for its conversation's answer before it is refused on this side.
///
/// The daemon denies a parked approval after its own bound, 60 s (`ApprovalRoutes`). This is that
/// bound plus a margin for the daemon's answer to reach this side. It is a second copy of the
/// daemon's value because this crate does not depend on the daemon's approval crate.
pub const APPROVAL_CLIENT_BOUND: Duration = Duration::from_secs(70);

/// Ask the daemon to decide a permission prompt raised by `conversation_id`, over the transport tool
/// calls use.
///
/// `Ok(true)` is an allow and `Ok(false)` a rejection. `Err` is a refusal, a failed call, or no
/// route, and carries the reason. A missing route is an `Err`, never an allow.
pub async fn request_approval(
    conversation_id: &str,
    tool_name: &str,
    input: &serde_json::Value,
) -> Result<bool, String> {
    let Some(transport) = detect_session_tool_transport() else {
        return Err(NOT_CONFIGURED_REASON.to_string());
    };
    match transport {
        SessionToolTransport::SandboxIpc { socket_path } => {
            let client = connect_sandbox_ipc(&socket_path).await?;
            request_approval_via_rpc_transport(
                &client,
                &SessionToolEnvelope::default(),
                conversation_id,
                tool_name,
                input,
            )
            .await
        }
        SessionToolTransport::DaemonUds {
            socket_path,
            session_id,
            session_token,
            daemon_instance_id,
        } => {
            let client = connect_sandbox_ipc(&socket_path).await?;
            let envelope = SessionToolEnvelope {
                session_id,
                session_token,
                daemon_instance_id,
            };
            request_approval_via_rpc_transport(
                &client,
                &envelope,
                conversation_id,
                tool_name,
                input,
            )
            .await
        }
        SessionToolTransport::DaemonHttp {
            session_id,
            daemon_url,
            session_token,
            daemon_instance_id,
        } => {
            let envelope = SessionToolEnvelope {
                session_id,
                session_token,
                daemon_instance_id,
            };
            request_approval_via_daemon_http(
                &daemon_url,
                &envelope,
                conversation_id,
                tool_name,
                input,
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
            let key = LiveKitRoomKey {
                url,
                room,
                token,
                server_identity,
            };
            let session = livekit_session(&key).await?;
            // A daemon that left the room would never answer, so it is refused now rather than
            // waited on for the whole bound.
            if !session.peer_present() {
                return Err(format!(
                    "remote daemon participant \"{}\" is no longer in room \"{}\"",
                    key.server_identity, key.room
                ));
            }
            let envelope = SessionToolEnvelope {
                session_id,
                session_token,
                daemon_instance_id,
            };
            request_approval_via_rpc_transport(
                session.transport(),
                &envelope,
                conversation_id,
                tool_name,
                input,
            )
            .await
        }
        SessionToolTransport::IncompleteLiveKit { missing } => {
            Err(incomplete_livekit_reason(&missing))
        }
    }
}

/// [`request_approval`] over an already-connected RPC transport, bounded by
/// [`APPROVAL_CLIENT_BOUND`].
pub async fn request_approval_via_rpc_transport(
    client: &Arc<dyn RpcClientTransport>,
    envelope: &SessionToolEnvelope,
    conversation_id: &str,
    tool_name: &str,
    input: &serde_json::Value,
) -> Result<bool, String> {
    request_approval_within(
        client,
        envelope,
        conversation_id,
        tool_name,
        input,
        APPROVAL_CLIENT_BOUND,
    )
    .await
}

/// [`request_approval_via_rpc_transport`] with the bound given: the answer must arrive within `bound`,
/// or the approval is refused, naming the approval that went unanswered.
pub async fn request_approval_within(
    client: &Arc<dyn RpcClientTransport>,
    envelope: &SessionToolEnvelope,
    conversation_id: &str,
    tool_name: &str,
    input: &serde_json::Value,
    bound: Duration,
) -> Result<bool, String> {
    let request = ApprovalRequest {
        session_token: envelope.session_token.clone(),
        session_id: envelope.session_id.clone(),
        daemon_instance_id: envelope.daemon_instance_id.clone(),
        conversation_id: conversation_id.to_string(),
        tool_name: tool_name.to_string(),
        input_json: input.to_string(),
    };
    let call = client.call_unary(
        "exec_tools.ExecToolService",
        "RequestApproval",
        request.encode_to_vec(),
    );
    let bytes = match tokio::time::timeout(bound, call).await {
        Ok(Ok(bytes)) => bytes,
        Ok(Err(status)) => return Err(status.message().to_string()),
        Err(_elapsed) => {
            return Err(format!(
                "the approval for conversation {conversation_id} was not answered within {bound:?}"
            ))
        }
    };
    let response = ApprovalResponse::decode(bytes.as_slice())
        .map_err(|e| format!("could not decode the approval reply: {e}"))?;
    Ok(response.allow)
}

/// [`request_approval`] over the daemon's HTTP listener: a JSON `POST` to the method's path, the same
/// route `ExecuteTool` takes over HTTP.
async fn request_approval_via_daemon_http(
    daemon_url: &str,
    envelope: &SessionToolEnvelope,
    conversation_id: &str,
    tool_name: &str,
    input: &serde_json::Value,
) -> Result<bool, String> {
    let body = serde_json::json!({
        "session_token": envelope.session_token,
        "session_id": envelope.session_id,
        "daemon_instance_id": envelope.daemon_instance_id,
        "conversation_id": conversation_id,
        "tool_name": tool_name,
        "input_json": input.to_string(),
    });
    let url = format!(
        "{}/exec_tools.ExecToolService/RequestApproval",
        daemon_url.trim_end_matches('/')
    );
    let client = reqwest::Client::builder()
        .timeout(APPROVAL_CLIENT_BOUND)
        .build()
        .map_err(|e| format!("could not start the approval client: {e}"))?;
    let response = client
        .post(&url)
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("relay connection error: {e}"))?;
    let status = response.status();
    let reply: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("relay parse error: {e}"))?;
    if !status.is_success() {
        return Err(reply
            .get("message")
            .and_then(serde_json::Value::as_str)
            .map_or_else(
                || format!("the daemon refused the approval with HTTP {status}"),
                str::to_string,
            ));
    }
    // proto3 JSON omits a `false` allow, so an absent field is a refusal.
    Ok(reply
        .get("allow")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false))
}
