use tddy_rpc::Status;

use tddy_daemon_livekit::livekit_rooms_stream::RoomRoster;

use std::sync::Arc;

pub async fn session_room_participant_identities(
    session_id: &str,
    room_name: String,
    room_roster: &Arc<dyn RoomRoster>,
) -> Result<Vec<String>, Status> {
    let rooms = room_roster.list_rooms().await.map_err(Status::from)?;
    let room = rooms
        .into_iter()
        .find(|room| room.name == room_name)
        .ok_or_else(|| {
            Status::not_found(format!(
                "the LiveKit server has no room called {room_name}; session {session_id} is \
                     not being facilitated in one"
            ))
        })?;
    let mut identities: Vec<String> = room
        .participants
        .into_iter()
        .map(|participant| participant.identity)
        .collect();
    identities.sort();
    Ok(identities)
}
