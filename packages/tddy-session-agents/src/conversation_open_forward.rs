// `encode_to_vec` and `decode` are `prost::Message` methods; the trait is imported anonymously
// because only its methods are used.
use prost::Message as _;

use tddy_service::proto::session_agents_svc::OpenAgentConversationResponse;

use tddy_rpc::Status;

use std::sync::Arc;

use tddy_service::proto::session_agents_svc::OpenAgentConversationRequest;

pub async fn forward_open_agent_conversation(
    req: &OpenAgentConversationRequest,
    owner: &str,
    conversation_id: &str,
    slot: &Arc<tokio::sync::RwLock<Option<Arc<livekit::Room>>>>,
) -> Result<(), Status> {
    let forwarded = OpenAgentConversationRequest {
        conversation_id: conversation_id.to_string(),
        daemon_instance_id: owner.to_string(),
        ..req.clone()
    };
    let answered = tddy_daemon_livekit::livekit_peer_discovery::forward_to_peer(
        slot,
        owner,
        crate::SERVICE_NAME,
        "OpenAgentConversation",
        forwarded.encode_to_vec(),
    )
    .await?;
    let opened = OpenAgentConversationResponse::decode(answered.as_slice())
        .map_err(|e| Status::internal(format!("decode OpenAgentConversationResponse: {e}")))?;
    if opened.conversation_id != conversation_id {
        return Err(Status::internal(format!(
            "daemon '{owner}' opened conversation {:?} instead of the requested \
                 {conversation_id:?}, so a prompt to it could not be routed and a cancel could not \
                 name it",
            opened.conversation_id
        )));
    }
    Ok(())
}
