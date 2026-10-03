//! A subagent conversation's calls to its facilitating daemon: its tool calls, which run in the
//! conversation's own worktree, and the worktree operations that are not tool calls.
//!
//! In its own module rather than beside [`crate::dispatch_session_tool`]: `lib.rs` is over the file
//! budget (`packages/tddy-session-tool-client/docs/code-issues/oversized-file-lib.md`), and this is
//! the one place the conversation id enters the envelope.

use std::sync::Arc;

use prost::Message as _;
use tddy_service::proto::exec_tools::{
    conversation_worktree_request::Op, ConversationWorktreeRequest, ConversationWorktreeResponse,
    DiffOp, ExecuteToolRequest, PullOp, PullRangeOp, RemoveOp, ResetOp, SyncOp,
};

#[cfg(feature = "livekit")]
use crate::livekit_session;
use crate::{
    clamp_remote_blocking_args, connect_sandbox_ipc, detect_session_tool_transport,
    dispatch_request_via_daemon_http, dispatch_request_via_livekit,
    dispatch_request_via_rpc_transport, execute_tool_request, incomplete_livekit_error,
    not_configured_error, LiveKitRoomKey, SessionToolEnvelope, SessionToolTransport,
};

/// A worktree operation a conversation asks its daemon for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversationWorktreeOp {
    /// Hand everything committed since the base to the session worktree, 3-way.
    Pull,
    /// Delete the worktree, its branch and its base ref.
    Remove,
}

/// Run `tool_name` for `conversation_id` over whichever transport this process was given — the
/// same transports as [`crate::dispatch_session_tool`], with the conversation named on the wire.
pub async fn dispatch_conversation_tool(
    conversation_id: &str,
    tool_name: &str,
    args: serde_json::Value,
) -> String {
    let Some(transport) = detect_session_tool_transport() else {
        return not_configured_error();
    };
    let call = |envelope: &SessionToolEnvelope, args: &serde_json::Value| {
        conversation_tool_request(envelope, conversation_id, tool_name, args)
    };
    match transport {
        SessionToolTransport::SandboxIpc { socket_path } => {
            // The socket identifies the session to the sandbox-runner; the envelope stays empty.
            let request = call(&SessionToolEnvelope::default(), &args);
            match connect_sandbox_ipc(&socket_path).await {
                Ok(client) => dispatch_request_via_rpc_transport(&client, request).await,
                Err(e) => error_body(e),
            }
        }
        SessionToolTransport::DaemonUds {
            socket_path,
            session_id,
            session_token,
            daemon_instance_id,
        } => {
            let request = call(
                &SessionToolEnvelope {
                    session_id,
                    session_token,
                    daemon_instance_id,
                },
                &args,
            );
            match connect_sandbox_ipc(&socket_path).await {
                Ok(client) => dispatch_request_via_rpc_transport(&client, request).await,
                Err(e) => error_body(e),
            }
        }
        SessionToolTransport::DaemonHttp {
            session_id,
            daemon_url,
            session_token,
            daemon_instance_id,
        } => {
            let request = call(
                &SessionToolEnvelope {
                    session_id,
                    session_token,
                    daemon_instance_id,
                },
                &args,
            );
            dispatch_request_via_daemon_http(&daemon_url, &request).await
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
            // Only this transport can hang, so only here are a call's blocks shortened.
            let args = clamp_remote_blocking_args(tool_name, &args);
            let request = call(
                &SessionToolEnvelope {
                    session_id,
                    session_token,
                    daemon_instance_id,
                },
                &args,
            );
            let key = LiveKitRoomKey {
                url,
                room,
                token,
                server_identity,
            };
            dispatch_request_via_livekit(&key, request).await
        }
        SessionToolTransport::IncompleteLiveKit { missing } => incomplete_livekit_error(&missing),
    }
}

/// Ask the facilitating daemon for `op` on `conversation_id`'s worktree; the answer's
/// `result_json`, or a `{"error", "is_error": true}` body naming why it could not be asked.
pub async fn conversation_worktree(conversation_id: &str, op: ConversationWorktreeOp) -> String {
    ask_conversation_worktree(|envelope| {
        conversation_worktree_request(&envelope, conversation_id, op)
    })
    .await
}

/// Send the `ConversationWorktree` request `request` builds, over whichever transport this process
/// was given; the builder is handed the envelope that transport identifies the session by.
async fn ask_conversation_worktree(
    request: impl Fn(SessionToolEnvelope) -> ConversationWorktreeRequest,
) -> String {
    let Some(transport) = detect_session_tool_transport() else {
        return not_configured_error();
    };
    match transport {
        SessionToolTransport::SandboxIpc { socket_path } => {
            // The runner rewrites the request to name the jail's own session.
            match connect_sandbox_ipc(&socket_path).await {
                Ok(client) => ask_daemon(&client, request(SessionToolEnvelope::default())).await,
                Err(e) => error_body(e),
            }
        }
        SessionToolTransport::DaemonUds {
            socket_path,
            session_id,
            session_token,
            daemon_instance_id,
        } => match connect_sandbox_ipc(&socket_path).await {
            Ok(client) => {
                let envelope = SessionToolEnvelope {
                    session_id,
                    session_token,
                    daemon_instance_id,
                };
                ask_daemon(&client, request(envelope)).await
            }
            Err(e) => error_body(e),
        },
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
            ask_daemon_over_http(&daemon_url, &request(envelope)).await
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
            let envelope = SessionToolEnvelope {
                session_id,
                session_token,
                daemon_instance_id,
            };
            ask_daemon_over_livekit(
                &LiveKitRoomKey {
                    url,
                    room,
                    token,
                    server_identity,
                },
                request(envelope),
            )
            .await
        }
        SessionToolTransport::IncompleteLiveKit { missing } => incomplete_livekit_error(&missing),
    }
}

#[cfg(feature = "livekit")]
async fn ask_daemon_over_livekit(
    key: &LiveKitRoomKey,
    request: ConversationWorktreeRequest,
) -> String {
    match livekit_session(key).await {
        Ok(session) if session.peer_present() => ask_daemon(session.transport(), request).await,
        Ok(_) => error_body(format!(
            "remote daemon participant \"{}\" is no longer in room \"{}\"",
            key.server_identity, key.room
        )),
        Err(e) => error_body(e),
    }
}

/// Without the `livekit` feature a split session cannot reach its daemon at all, and says so.
#[cfg(not(feature = "livekit"))]
async fn ask_daemon_over_livekit(
    _key: &LiveKitRoomKey,
    _request: ConversationWorktreeRequest,
) -> String {
    error_body(
        "remote worktree dispatch requires the 'livekit' cargo feature; \
         rebuild with: cargo build -p tddy-tools --features livekit"
            .to_string(),
    )
}

/// `ExecToolService/ConversationWorktree` over an RPC transport, answered with the result JSON.
async fn ask_daemon(
    client: &Arc<dyn tddy_rpc::RpcClientTransport>,
    request: ConversationWorktreeRequest,
) -> String {
    let bytes = match client
        .call_unary(
            "exec_tools.ExecToolService",
            "ConversationWorktree",
            request.encode_to_vec(),
        )
        .await
    {
        Ok(bytes) => bytes,
        Err(e) => return error_body(format!("conversation worktree rpc call: {e}")),
    };
    match ConversationWorktreeResponse::decode(bytes.as_slice()) {
        Ok(response) => response.result_json,
        Err(e) => error_body(format!("conversation worktree rpc decode response: {e}")),
    }
}

/// The same call over the daemon's HTTP Connect endpoint.
async fn ask_daemon_over_http(daemon_url: &str, request: &ConversationWorktreeRequest) -> String {
    let body = connect_json_body(request);
    let url = format!(
        "{}/exec_tools.ExecToolService/ConversationWorktree",
        daemon_url.trim_end_matches('/')
    );
    let answered = reqwest::Client::new()
        .post(&url)
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await;
    match answered {
        Ok(resp) => match resp.json::<serde_json::Value>().await {
            Ok(body) => match body.get("result_json").and_then(|v| v.as_str()) {
                Some(result) => result.to_string(),
                None => error_body(format!("relay answered without a result: {body}")),
            },
            Err(e) => error_body(format!("relay parse error: {e}")),
        },
        Err(e) => error_body(format!("relay connection error: {e}")),
    }
}

/// The Connect JSON body of a `ConversationWorktree` request: the request's fields, and the chosen
/// operation as a key of its own.
pub(crate) fn connect_json_body(request: &ConversationWorktreeRequest) -> serde_json::Value {
    let mut fields = serde_json::Map::from_iter([
        (
            "session_token".to_string(),
            request.session_token.clone().into(),
        ),
        ("session_id".to_string(), request.session_id.clone().into()),
        (
            "daemon_instance_id".to_string(),
            request.daemon_instance_id.clone().into(),
        ),
        (
            "conversation_id".to_string(),
            request.conversation_id.clone().into(),
        ),
    ]);
    match &request.op {
        Some(Op::Pull(_)) => fields.insert("pull".to_string(), serde_json::json!({})),
        Some(Op::Remove(_)) => fields.insert("remove".to_string(), serde_json::json!({})),
        Some(Op::Reset(reset)) => fields.insert(
            "reset".to_string(),
            serde_json::json!({ "commit": reset.commit }),
        ),
        Some(Op::Diff(diff)) => fields.insert(
            "diff".to_string(),
            serde_json::json!({ "from": diff.from, "to": diff.to }),
        ),
        Some(Op::PullRange(range)) => fields.insert(
            "pull_range".to_string(),
            serde_json::json!({
                "from": range.from,
                "to": range.to,
                "already_pulled": range.already_pulled
            }),
        ),
        Some(Op::Sync(_)) => fields.insert("sync".to_string(), serde_json::json!({})),
        None => None,
    };
    serde_json::Value::Object(fields)
}

fn error_body(message: String) -> String {
    serde_json::json!({ "error": message, "is_error": true }).to_string()
}

/// Ask the facilitating daemon to merge the session worktree's current files into
/// `conversation_id`'s worktree before a turn; the answer's `result_json` (`{"sync": {…} | null}` or
/// `{"conflicts": […], "moreConflicts": n}`) or a `{"error", "is_error": true}` body.
pub async fn sync_conversation_worktree(conversation_id: &str) -> String {
    ask_conversation_worktree(|envelope| conversation_sync_request(&envelope, conversation_id))
        .await
}

/// The `ConversationWorktree` request a sync sends.
pub(crate) fn conversation_sync_request(
    envelope: &SessionToolEnvelope,
    conversation_id: &str,
) -> ConversationWorktreeRequest {
    conversation_op_request(envelope, conversation_id, Op::Sync(SyncOp {}))
}

/// Ask the facilitating daemon to reset `conversation_id`'s worktree to `commit`, or to its base
/// when `commit` is `None`; the answer's `result_json` (`{"reset": {to, droppedCommits} | null}`)
/// or a `{"error", "is_error": true}` body.
pub async fn reset_conversation_worktree(conversation_id: &str, commit: Option<&str>) -> String {
    ask_conversation_worktree(|envelope| {
        conversation_reset_request(&envelope, conversation_id, commit)
    })
    .await
}

/// The `ConversationWorktree` request a reset sends; an empty `commit` names the base.
pub(crate) fn conversation_reset_request(
    envelope: &SessionToolEnvelope,
    conversation_id: &str,
    commit: Option<&str>,
) -> ConversationWorktreeRequest {
    conversation_op_request(
        envelope,
        conversation_id,
        Op::Reset(ResetOp {
            commit: commit.unwrap_or_default().to_string(),
        }),
    )
}

/// Ask the facilitating daemon for the diff `from..to` of `conversation_id` (either bound `None`:
/// base / tip); the answer's `result_json` (`{"diff": {…}}`) or a `{"error", "is_error": true}` body.
pub async fn diff_conversation_worktree(
    conversation_id: &str,
    from: Option<&str>,
    to: Option<&str>,
) -> String {
    ask_conversation_worktree(|envelope| {
        conversation_diff_request(&envelope, conversation_id, from, to)
    })
    .await
}

/// The `ConversationWorktree` request a diff sends.
pub(crate) fn conversation_diff_request(
    envelope: &SessionToolEnvelope,
    conversation_id: &str,
    from: Option<&str>,
    to: Option<&str>,
) -> ConversationWorktreeRequest {
    conversation_op_request(
        envelope,
        conversation_id,
        Op::Diff(DiffOp {
            from: from.unwrap_or_default().to_string(),
            to: to.unwrap_or_default().to_string(),
        }),
    )
}

/// Ask the facilitating daemon to apply `conversation_id`'s commits `from..=to` that are not in
/// `already_pulled` to the session worktree; the answer's `result_json` (`{"pulled": {…} | null}`) or
/// a `{"error", "is_error": true}` body.
pub async fn pull_conversation_range(
    conversation_id: &str,
    from: Option<&str>,
    to: Option<&str>,
    already_pulled: &[String],
) -> String {
    ask_conversation_worktree(|envelope| {
        conversation_pull_range_request(&envelope, conversation_id, from, to, already_pulled)
    })
    .await
}

/// The `ConversationWorktree` request a range pull sends.
pub(crate) fn conversation_pull_range_request(
    envelope: &SessionToolEnvelope,
    conversation_id: &str,
    from: Option<&str>,
    to: Option<&str>,
    already_pulled: &[String],
) -> ConversationWorktreeRequest {
    conversation_op_request(
        envelope,
        conversation_id,
        Op::PullRange(PullRangeOp {
            from: from.unwrap_or_default().to_string(),
            to: to.unwrap_or_default().to_string(),
            already_pulled: already_pulled.to_vec(),
        }),
    )
}

/// The `ExecuteTool` a conversation's call sends.
pub(crate) fn conversation_tool_request(
    envelope: &SessionToolEnvelope,
    conversation_id: &str,
    tool_name: &str,
    args: &serde_json::Value,
) -> ExecuteToolRequest {
    ExecuteToolRequest {
        conversation_id: conversation_id.to_string(),
        ..execute_tool_request(envelope, tool_name, args)
    }
}

/// The `ConversationWorktree` request for `op`.
pub(crate) fn conversation_worktree_request(
    envelope: &SessionToolEnvelope,
    conversation_id: &str,
    op: ConversationWorktreeOp,
) -> ConversationWorktreeRequest {
    let op = match op {
        ConversationWorktreeOp::Pull => Op::Pull(PullOp {}),
        ConversationWorktreeOp::Remove => Op::Remove(RemoveOp {}),
    };
    conversation_op_request(envelope, conversation_id, op)
}

/// The `ConversationWorktree` request carrying `op` for `conversation_id`, identified by `envelope`.
fn conversation_op_request(
    envelope: &SessionToolEnvelope,
    conversation_id: &str,
    op: Op,
) -> ConversationWorktreeRequest {
    ConversationWorktreeRequest {
        session_token: envelope.session_token.clone(),
        session_id: envelope.session_id.clone(),
        daemon_instance_id: envelope.daemon_instance_id.clone(),
        conversation_id: conversation_id.to_string(),
        op: Some(op),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn an_envelope() -> SessionToolEnvelope {
        SessionToolEnvelope {
            session_id: "sess-1".into(),
            session_token: "token-1".into(),
            daemon_instance_id: "daemon-a".into(),
        }
    }

    #[test]
    fn a_conversation_tool_call_names_its_conversation_on_the_wire() {
        // Given
        let args = serde_json::json!({ "path": "a.txt", "contents": "a" });

        // When
        let request = conversation_tool_request(&an_envelope(), "explore", "Write", &args);

        // Then
        assert_eq!(
            request,
            ExecuteToolRequest {
                session_token: "token-1".into(),
                session_id: "sess-1".into(),
                tool_name: "Write".into(),
                args_json: args.to_string(),
                daemon_instance_id: "daemon-a".into(),
                conversation_id: "explore".into(),
            }
        );
    }

    #[test]
    fn a_pull_names_its_conversation_and_asks_for_a_pull() {
        assert_eq!(
            conversation_worktree_request(&an_envelope(), "explore", ConversationWorktreeOp::Pull),
            ConversationWorktreeRequest {
                session_token: "token-1".into(),
                session_id: "sess-1".into(),
                daemon_instance_id: "daemon-a".into(),
                conversation_id: "explore".into(),
                op: Some(Op::Pull(PullOp {})),
            }
        );
    }

    #[test]
    fn a_remove_asks_for_a_remove() {
        assert_eq!(
            conversation_worktree_request(
                &an_envelope(),
                "explore",
                ConversationWorktreeOp::Remove
            )
            .op,
            Some(Op::Remove(RemoveOp {}))
        );
    }

    #[test]
    fn a_reset_to_a_commit_names_the_commit() {
        assert_eq!(
            conversation_reset_request(&an_envelope(), "explore", Some("3f9c2ab")).op,
            Some(Op::Reset(ResetOp {
                commit: "3f9c2ab".into()
            }))
        );
    }

    #[test]
    fn a_reset_to_the_base_sends_an_empty_commit() {
        assert_eq!(
            conversation_reset_request(&an_envelope(), "explore", None).op,
            Some(Op::Reset(ResetOp {
                commit: String::new()
            }))
        );
    }

    #[test]
    fn a_diff_names_both_bounds_and_leaves_an_omitted_one_empty() {
        assert_eq!(
            conversation_diff_request(&an_envelope(), "explore", Some("3f9c2ab"), None).op,
            Some(Op::Diff(DiffOp {
                from: "3f9c2ab".into(),
                to: String::new()
            }))
        );
    }

    #[test]
    fn a_range_pull_carries_the_callers_ledger() {
        assert_eq!(
            conversation_pull_range_request(
                &an_envelope(),
                "explore",
                None,
                Some("9e01d4c"),
                &["3f9c2ab".to_string()]
            )
            .op,
            Some(Op::PullRange(PullRangeOp {
                from: String::new(),
                to: "9e01d4c".into(),
                already_pulled: vec!["3f9c2ab".into()]
            }))
        );
    }

    #[test]
    fn a_sync_names_its_conversation_and_asks_for_a_sync() {
        assert_eq!(
            conversation_sync_request(&an_envelope(), "explore"),
            ConversationWorktreeRequest {
                session_token: "token-1".into(),
                session_id: "sess-1".into(),
                daemon_instance_id: "daemon-a".into(),
                conversation_id: "explore".into(),
                op: Some(Op::Sync(SyncOp {})),
            }
        );
    }

    #[test]
    fn a_sync_travels_over_http_as_its_own_key() {
        // Given
        let request = conversation_sync_request(&an_envelope(), "explore");

        // When
        let body = connect_json_body(&request);

        // Then
        assert_eq!(
            body,
            serde_json::json!({
                "session_token": "token-1",
                "session_id": "sess-1",
                "daemon_instance_id": "daemon-a",
                "conversation_id": "explore",
                "sync": {}
            })
        );
    }

    /// Guards the extraction: an existing op keeps its encoding.
    #[test]
    fn a_reset_still_travels_over_http_with_its_commit() {
        // Given
        let request = conversation_reset_request(&an_envelope(), "explore", Some("3f9c2ab"));

        // When
        let body = connect_json_body(&request);

        // Then
        assert_eq!(
            body,
            serde_json::json!({
                "session_token": "token-1",
                "session_id": "sess-1",
                "daemon_instance_id": "daemon-a",
                "conversation_id": "explore",
                "reset": { "commit": "3f9c2ab" }
            })
        );
    }
}
