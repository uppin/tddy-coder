// `encode_to_vec` is a `prost::Message` method; the trait is imported anonymously because
// only its methods are used.
use prost::Message as _;

use tddy_service::proto::session_agents_svc::SessionAgentRoster;
use tddy_service::session_agents::SESSION_AGENTS_TOPIC;

/// Broadcast `roster` to everyone in `session_id`'s room through the room port: encoded, on the
/// session-agents topic.
///
/// Best-effort, as it always was: a roster that could not reach the room is a stale browser tab, and
/// the caller has already persisted and streamed the roster it is announcing.
pub fn broadcast_roster_through(
    rooms: &dyn tddy_daemon_extensions::RoomBroadcast,
    session_id: &str,
    roster: &SessionAgentRoster,
) {
    if let Err(e) = rooms.publish(session_id, SESSION_AGENTS_TOPIC, roster.encode_to_vec()) {
        log::warn!(
            "could not broadcast session {session_id}'s roster on {SESSION_AGENTS_TOPIC}: {e}"
        );
    }
}
