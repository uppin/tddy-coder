use tddy_daemon_extensions::RoomDirectory;
use tddy_rpc::{Code, Status};

pub async fn session_room_participant_identities(
    session_id: &str,
    room_name: String,
    room_directory: &dyn RoomDirectory,
) -> Result<Vec<String>, Status> {
    room_directory
        .participants(&room_name)
        .await
        .map_err(|status| match status.code {
            Code::NotFound => Status::not_found(format!(
                "the room server has no room called {room_name}; session {session_id} is \
                     not being facilitated in one"
            )),
            _ => status,
        })
}
