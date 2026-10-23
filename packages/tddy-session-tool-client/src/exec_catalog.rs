//! The exec tools this session's agent is offered, asked of the daemon that serves the session
//! (`#project-home` 5/7).
//!
//! `ListExecTools` answers per session when it is given one: a project-home session is offered its
//! read-only tools and the project-home tools, and any other session the catalog without them. This
//! asks that question over the two transports that carry a session identity to the daemon. The
//! sandbox socket and LiveKit cannot carry the question today, and say so rather than guess.

use std::path::Path;
use std::time::Duration;

use prost::Message;
use tddy_service::proto::exec_tools::{ListExecToolsRequest, ListExecToolsResponse};

use crate::rpc_transports::connect_sandbox_ipc;
use crate::{detect_session_tool_transport, SessionToolTransport};

/// How long the daemon's answer may take before the question is reported as failed.
const ANSWER_DEADLINE: Duration = Duration::from_secs(10);

/// What the session's configured transport said to the question "which exec tools may this session
/// use".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionExecToolAnswer {
    /// The daemon's answer: the exec tool names this session may be offered.
    Admitted(Vec<String>),
    /// The configured transport cannot carry the question. Names the transport.
    CannotAsk(&'static str),
    /// The daemon was asked and did not answer usefully, or did not answer within the deadline.
    Failed(String),
}

/// Ask the daemon which exec tools this session may use.
///
/// Only the two transports that carry a session identity can ask: `DaemonHttp` and `DaemonUds`. The
/// caller decides what each answer means; this crate does not substitute a catalog of its own.
pub async fn session_exec_tool_answer() -> SessionExecToolAnswer {
    let asked = match detect_session_tool_transport() {
        None => return SessionExecToolAnswer::CannotAsk("no session-tool transport"),
        Some(SessionToolTransport::SandboxIpc { .. }) => {
            return SessionExecToolAnswer::CannotAsk("sandbox socket")
        }
        Some(SessionToolTransport::LiveKit { .. }) => {
            return SessionExecToolAnswer::CannotAsk("LiveKit")
        }
        Some(SessionToolTransport::IncompleteLiveKit { .. }) => {
            return SessionExecToolAnswer::CannotAsk("incomplete LiveKit configuration")
        }
        Some(SessionToolTransport::DaemonUds {
            socket_path,
            session_id,
            session_token,
            daemon_instance_id,
        }) => {
            let request = scoped_request(session_id, session_token, daemon_instance_id);
            tokio::time::timeout(ANSWER_DEADLINE, list_via_uds(&socket_path, request)).await
        }
        Some(SessionToolTransport::DaemonHttp {
            session_id,
            daemon_url,
            session_token,
            daemon_instance_id,
        }) => {
            let request = scoped_request(session_id, session_token, daemon_instance_id);
            tokio::time::timeout(ANSWER_DEADLINE, list_via_http(&daemon_url, request)).await
        }
    };
    match asked {
        Ok(Ok(names)) => SessionExecToolAnswer::Admitted(names),
        Ok(Err(e)) => SessionExecToolAnswer::Failed(format!("ListExecTools failed: {e}")),
        Err(_) => SessionExecToolAnswer::Failed(format!(
            "ListExecTools did not answer within {ANSWER_DEADLINE:?}"
        )),
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

async fn list_via_http(
    daemon_url: &str,
    request: ListExecToolsRequest,
) -> Result<Vec<String>, String> {
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
