// `encode_to_vec` is a `prost::Message` method; the trait is imported anonymously because
// only its methods are used.
use prost::Message as _;

use tddy_service::proto::session_agents_svc::CancelAgentConversationRequest;

use tddy_daemon_livekit::livekit_peer_discovery::CommonRoom;
use tddy_rpc::Status;

pub async fn forward_cancel_agent_conversation(
    session_token: &str,
    session_id: &str,
    daemon_instance_id: &str,
    conversation_id: &str,
    room: &CommonRoom,
) -> Result<(), Status> {
    room.forward_to_peer(
        daemon_instance_id,
        crate::SERVICE_NAME,
        "CancelAgentConversation",
        CancelAgentConversationRequest {
            // The detaching caller's own token: the peer authenticates a cancel exactly as it
            // authenticated the open, and this daemon holds no other credential to present.
            session_token: session_token.to_string(),
            session_id: session_id.to_string(),
            daemon_instance_id: daemon_instance_id.to_string(),
            conversation_id: conversation_id.to_string(),
        }
        .encode_to_vec(),
    )
    .await?;
    Ok(())
}
