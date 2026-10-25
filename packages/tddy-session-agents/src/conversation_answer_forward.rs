// `encode_to_vec` and `decode` are `prost::Message` methods; the trait is imported anonymously
// because only its methods are used.
use prost::Message as _;

use tddy_daemon_kernel::peer_forwarding::CommonRoom;
use tddy_rpc::Status;
use tddy_service::proto::session_agents_svc::{
    AnswerAgentPermissionRequest, AnswerAgentPermissionResponse,
};

use crate::agent_host_callbacks::AgentRoster;

impl AgentRoster {
    /// Carry an answer to the permission request a conversation's owning daemon holds, and return
    /// the answer that daemon recorded — or the refusal its own gate gave.
    pub async fn forward_answer_agent_permission(
        &self,
        session_token: &str,
        session_id: &str,
        owner: &str,
        request: &AnswerAgentPermissionRequest,
    ) -> Result<AnswerAgentPermissionResponse, Status> {
        let slot = self
            .peer_routing
            .common_room_slot("AnswerAgentPermission")?;
        forward_answer_agent_permission(session_token, session_id, owner, request, slot).await
    }
}

/// Forward `request` to `owner` over the common room, addressed to it so the owner serves the answer
/// as its own call: its routing preamble keeps a call addressed to itself, and its own deferral gate
/// and first-answer-wins resolution decide the answer.
pub async fn forward_answer_agent_permission(
    session_token: &str,
    session_id: &str,
    owner: &str,
    request: &AnswerAgentPermissionRequest,
    room: &CommonRoom,
) -> Result<AnswerAgentPermissionResponse, Status> {
    let forwarded = AnswerAgentPermissionRequest {
        // The caller's own token: the owner authenticates the answer exactly as it authenticated the
        // open, and this daemon holds no other credential to present.
        session_token: session_token.to_string(),
        session_id: session_id.to_string(),
        daemon_instance_id: owner.to_string(),
        ..request.clone()
    };
    let answered = room
        .forward_to_peer(
            owner,
            crate::SERVICE_NAME,
            "AnswerAgentPermission",
            forwarded.encode_to_vec(),
        )
        .await?;
    AnswerAgentPermissionResponse::decode(answered.as_slice()).map_err(|e| {
        Status::internal(format!(
            "decode AnswerAgentPermissionResponse from daemon '{owner}': {e}"
        ))
    })
}
