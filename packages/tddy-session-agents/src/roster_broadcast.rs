// `encode_to_vec` is a `prost::Message` method; the trait is imported anonymously because
// only its methods are used.
use prost::Message as _;

use std::sync::Arc;

use tddy_service::proto::session_agents_svc::SessionAgentRoster;

pub async fn broadcast_roster(
    session_id: &str,
    roster: &SessionAgentRoster,
    publisher: Arc<tddy_livekit::BroadcastPublisher>,
) {
    if let Err(e) = publisher.publish(&roster.encode_to_vec()).await {
        log::warn!(
            "could not broadcast session {session_id}'s roster on \
                 {}: {e}",
            tddy_daemon_livekit::session_room::SESSION_AGENTS_TOPIC
        );
    }
}

/// Broadcast `roster` to everyone in `session_id`'s room through the room port: encoded, on the
/// session-agents topic.
pub fn broadcast_roster_through(
    rooms: &dyn tddy_daemon_extensions::RoomBroadcast,
    session_id: &str,
    roster: &SessionAgentRoster,
) {
    let _ = (rooms, session_id, roster);
    todo!("broadcast_roster_through")
}
