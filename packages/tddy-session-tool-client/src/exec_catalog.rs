//! The exec tools this session's agent is offered, asked of the daemon that serves the session
//! (`#project-home` 5/7).
//!
//! `ListExecTools` answers per session when it is given one: a project-home session is offered its
//! read-only tools and the project-home tools, and any other session the catalog without them. This
//! asks that question over the two transports that carry a session identity to the daemon. The
//! sandbox socket and LiveKit carry none, so they answer `None` and the caller keeps its static set.

use std::path::Path;
use std::time::Duration;

use prost::Message;
use tddy_service::proto::exec_tools::{ListExecToolsRequest, ListExecToolsResponse};

use crate::rpc_transports::connect_sandbox_ipc;
use crate::{detect_session_tool_transport, SessionToolTransport};

/// How long the daemon's answer may take before the caller advertises its static set.
const ANSWER_DEADLINE: Duration = Duration::from_secs(10);

/// The names of the exec tools the daemon offers this session.
///
/// `None` when no session-scoped answer is available: the configured transport cannot carry the
/// question, or the answer fails or misses [`ANSWER_DEADLINE`]. The failure is logged, and the caller
/// advertises the static exec catalog. A tool call is still refused by the daemon when the session may
/// not run it, so this only shapes what the agent is offered.
pub async fn session_exec_tool_names() -> Option<Vec<String>> {
    let asked = match detect_session_tool_transport()? {
        SessionToolTransport::DaemonUds {
            socket_path,
            session_id,
            session_token,
            daemon_instance_id,
        } => {
            let request = scoped_request(session_id, session_token, daemon_instance_id);
            tokio::time::timeout(ANSWER_DEADLINE, list_via_uds(&socket_path, request)).await
        }
        SessionToolTransport::DaemonHttp {
            session_id,
            daemon_url,
            session_token,
            daemon_instance_id,
        } => {
            let request = scoped_request(session_id, session_token, daemon_instance_id);
            tokio::time::timeout(ANSWER_DEADLINE, list_via_http(&daemon_url, request)).await
        }
        _ => return None,
    };
    match asked {
        Ok(Ok(names)) => Some(names),
        Ok(Err(e)) => {
            log::warn!(
                target: "tddy_session_tool_client",
                "ListExecTools for this session failed ({e}); advertising the static exec catalog"
            );
            None
        }
        Err(_) => {
            log::warn!(
                target: "tddy_session_tool_client",
                "ListExecTools for this session did not answer within {ANSWER_DEADLINE:?}; advertising the static exec catalog"
            );
            None
        }
    }
}

fn scoped_request(
    session_id: String,
    session_token: String,
    daemon_instance_id: String,
) -> ListExecToolsRequest {
    ListExecToolsRequest {
        session_token,
        daemon_instance_id,
        session_id,
    }
}

async fn list_via_uds(
    socket_path: &Path,
    request: ListExecToolsRequest,
) -> Result<Vec<String>, String> {
    let client = connect_sandbox_ipc(socket_path).await?;
    let bytes = client
        .call_unary(
            "exec_tools.ExecToolService",
            "ListExecTools",
            request.encode_to_vec(),
        )
        .await
        .map_err(|e| format!("tool rpc call: {e}"))?;
    let response = ListExecToolsResponse::decode(bytes.as_slice())
        .map_err(|e| format!("decode ListExecToolsResponse: {e}"))?;
    Ok(response.tools.into_iter().map(|tool| tool.name).collect())
}

async fn list_via_http(daemon_url: &str, request: ListExecToolsRequest) -> Result<Vec<String>, String> {
    let body = serde_json::json!({
        "session_token": request.session_token,
        "session_id": request.session_id,
        "daemon_instance_id": request.daemon_instance_id,
    });
    let url = format!(
        "{}/exec_tools.ExecToolService/ListExecTools",
        daemon_url.trim_end_matches('/')
    );
    let response = reqwest::Client::new()
        .post(&url)
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("relay connection error: {e}"))?;
    let status = response.status();
    let value: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("relay answer is not JSON: {e}"))?;
    if !status.is_success() {
        return Err(format!("the daemon answered {status}: {value}"));
    }
    value["tools"]
        .as_array()
        .ok_or_else(|| format!("the daemon's answer carries no tools list: {value}"))?
        .iter()
        .map(|tool| {
            tool["name"]
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| format!("a tool without a name in the daemon's answer: {tool}"))
        })
        .collect()
}
