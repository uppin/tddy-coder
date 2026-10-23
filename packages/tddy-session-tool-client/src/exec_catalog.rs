//! The exec tools this session's agent is offered, asked of the daemon that serves the session
//! (`#project-home` 5/7).
//!
//! `ListExecTools` answers per session when it is given one: a project-home session is offered its
//! read-only tools and the project-home tools, and any other session the catalog without them. Every
//! transport that carries a session to a daemon asks it: the daemon's HTTP relay, its agent-facing unix
//! socket, the sandbox socket (the runner relays the question to the host daemon, naming the jail's own
//! session) and LiveKit (the remote daemon's `ExecToolService`, as `ExecuteTool` reaches it).

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use prost::Message;
use tddy_service::proto::exec_tools::{ListExecToolsRequest, ListExecToolsResponse};

use crate::room_transport::LiveKitRoomKey;
use crate::rpc_transports::{connect_sandbox_ipc, livekit_session};
use crate::{detect_session_tool_transport, SessionToolTransport};

/// How long the daemon's answer may take before the question is reported as failed.
const ANSWER_DEADLINE: Duration = Duration::from_secs(10);

/// What the session's configured transport said to the question "which exec tools may this session
/// use".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionExecToolAnswer {
    /// The daemon's answer: the exec tool names this session may be offered.
    Admitted(Vec<String>),
    /// No session-tool transport is configured, so there is no daemon to ask and no exec tool is
    /// registered to admit.
    NoTransport,
    /// The daemon was asked and did not answer usefully, or did not answer within the deadline.
    Failed(String),
}

/// Ask the daemon which exec tools this session may use.
///
/// Every configured transport asks. The caller decides what each answer means; this crate does not
/// substitute a catalog of its own.
pub async fn session_exec_tool_answer() -> SessionExecToolAnswer {
    let asked = match detect_session_tool_transport() {
        None => return SessionExecToolAnswer::NoTransport,
        Some(SessionToolTransport::IncompleteLiveKit { missing }) => {
            return SessionExecToolAnswer::Failed(crate::incomplete_livekit_reason(&missing))
        }
        Some(SessionToolTransport::SandboxIpc { socket_path }) => {
            // The runner rewrites the request to name the jail's own session, so the envelope is empty.
            let request = scoped_request(String::new(), String::new(), String::new());
            tokio::time::timeout(ANSWER_DEADLINE, list_via_uds(&socket_path, request)).await
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
        Some(SessionToolTransport::LiveKit {
            url,
            room,
            token,
            server_identity,
            session_id,
            session_token,
            daemon_instance_id,
        }) => {
            let request = scoped_request(session_id, session_token, daemon_instance_id);
            let key = LiveKitRoomKey {
                url,
                room,
                token,
                server_identity,
            };
            tokio::time::timeout(ANSWER_DEADLINE, list_via_livekit(&key, request)).await
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
    list_over_rpc(&client, request).await
}

/// The same question over the room connection the session's tool calls ride, so the remote daemon
/// answers it from the `ExecToolService` that serves its `ExecuteTool`.
async fn list_via_livekit(
    key: &LiveKitRoomKey,
    request: ListExecToolsRequest,
) -> Result<Vec<String>, String> {
    let session = livekit_session(key).await?;
    if !session.peer_present() {
        return Err(format!(
            "remote daemon participant \"{}\" is no longer in room \"{}\"",
            key.server_identity, key.room
        ));
    }
    list_over_rpc(session.transport(), request).await
}

/// `exec_tools.ExecToolService/ListExecTools` over an RPC transport: the names it answers.
async fn list_over_rpc(
    client: &Arc<dyn tddy_rpc::RpcClientTransport>,
    request: ListExecToolsRequest,
) -> Result<Vec<String>, String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tddy_rpc::Status;
    use tddy_service::proto::exec_tools::ToolDef;
    use tokio::sync::mpsc;

    /// A peer that records the one unary call it receives and answers it with `tools`.
    struct AnswersListExecTools {
        calls: Mutex<Vec<(String, String, Vec<u8>)>>,
        tools: Vec<&'static str>,
    }

    #[async_trait::async_trait]
    impl tddy_rpc::RpcClientTransport for AnswersListExecTools {
        async fn call_unary(
            &self,
            service: &str,
            method: &str,
            request_bytes: Vec<u8>,
        ) -> Result<Vec<u8>, Status> {
            self.calls
                .lock()
                .unwrap()
                .push((service.into(), method.into(), request_bytes));
            Ok(ListExecToolsResponse {
                tools: self
                    .tools
                    .iter()
                    .map(|name| ToolDef {
                        name: name.to_string(),
                        ..Default::default()
                    })
                    .collect(),
            }
            .encode_to_vec())
        }
        async fn call_server_stream(
            &self,
            _: &str,
            _: &str,
            _: Vec<u8>,
        ) -> Result<mpsc::Receiver<Result<Vec<u8>, Status>>, Status> {
            unreachable!("ListExecTools is unary")
        }
        async fn call_client_stream(
            &self,
            _: &str,
            _: &str,
            _: Vec<Vec<u8>>,
        ) -> Result<Vec<u8>, Status> {
            unreachable!("ListExecTools is unary")
        }
        async fn call_bidi_stream(
            &self,
            _: &str,
            _: &str,
            _: Vec<Vec<u8>>,
        ) -> Result<mpsc::Receiver<Result<Vec<u8>, Status>>, Status> {
            unreachable!("ListExecTools is unary")
        }
    }

    #[tokio::test]
    async fn the_question_goes_to_the_exec_tool_service_and_the_answer_is_its_tool_names() {
        // Given a peer that answers with two tools
        let peer = Arc::new(AnswersListExecTools {
            calls: Mutex::new(Vec::new()),
            tools: vec!["Read", "ListProjectSessions"],
        });
        let client: Arc<dyn tddy_rpc::RpcClientTransport> = peer.clone();

        // When the session's exec tools are asked over that transport
        let names = list_over_rpc(
            &client,
            scoped_request("s-1".into(), "tok".into(), "d-1".into()),
        )
        .await
        .expect("answered");

        // Then the request named the session on ExecToolService/ListExecTools, and the names are the answer
        assert_eq!(names, vec!["Read", "ListProjectSessions"]);
        let calls = peer.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(
            (calls[0].0.as_str(), calls[0].1.as_str()),
            ("exec_tools.ExecToolService", "ListExecTools")
        );
        let sent = ListExecToolsRequest::decode(calls[0].2.as_slice()).unwrap();
        assert_eq!(
            (sent.session_id, sent.session_token, sent.daemon_instance_id),
            ("s-1".to_string(), "tok".to_string(), "d-1".to_string())
        );
    }
}
