//! The plugin's registries as the core's agent-roster ports: broadcasting in a hosted session room,
//! admitting owning daemons to it, and listing who is in a room.

use std::sync::Arc;

use tddy_daemon_extensions::{RoomAdmission, RoomBroadcast, RoomDirectory};
use tddy_rpc::Status;

use crate::livekit_rooms_stream::RoomRoster;
use crate::session_admission_service::SessionAdmissionRegistry;
use crate::session_room::{QueuedBroadcast, SessionRoomRegistry};

impl RoomBroadcast for SessionRoomRegistry {
    /// Queues `payload` for the session's room and returns; the publish itself runs on a task that
    /// drains the queue in order, so snapshots reach the room in the order they were handed over.
    /// A failed publish is logged there, which is all the roster's best-effort broadcast ever did
    /// with one.
    fn publish(&self, session_id: &str, topic: &str, payload: Vec<u8>) -> Result<(), Status> {
        let Some(publisher) = self.agents_publisher(session_id) else {
            return Ok(());
        };
        if topic != tddy_service::session_agents::SESSION_AGENTS_TOPIC {
            return Err(Status::invalid_argument(format!(
                "a session room publishes the roster on {}; {topic:?} has no publisher here",
                tddy_service::session_agents::SESSION_AGENTS_TOPIC
            )));
        }
        let outbox = match self.agents_outbox.get() {
            Some(outbox) => outbox,
            None => {
                let runtime = tokio::runtime::Handle::try_current().map_err(|e| {
                    Status::failed_precondition(format!(
                        "cannot publish to session {session_id}'s room outside a runtime: {e}"
                    ))
                })?;
                self.agents_outbox.get_or_init(|| {
                    let (outbox, mut queued) =
                        tokio::sync::mpsc::unbounded_channel::<QueuedBroadcast>();
                    runtime.spawn(async move {
                        while let Some((publisher, session_id, payload)) = queued.recv().await {
                            if let Err(e) = publisher.publish(&payload).await {
                                log::warn!(
                                    "could not broadcast session {session_id}'s roster on {}: {e}",
                                    tddy_service::session_agents::SESSION_AGENTS_TOPIC
                                );
                            }
                        }
                    });
                    outbox
                })
            }
        };
        outbox
            .send((publisher, session_id.to_string(), payload))
            .map_err(|_| Status::internal("the session-room broadcast queue has stopped"))
    }
}

impl RoomAdmission for SessionAdmissionRegistry {
    fn admit(&self, session_id: &str, owning_instance_id: &str) -> bool {
        SessionAdmissionRegistry::admit(self, session_id, owning_instance_id)
    }
    fn revoke(&self, session_id: &str, owning_instance_id: &str) -> bool {
        SessionAdmissionRegistry::revoke(self, session_id, owning_instance_id)
    }
    fn is_admitted(&self, session_id: &str, owning_instance_id: &str) -> bool {
        SessionAdmissionRegistry::is_admitted(self, session_id, owning_instance_id)
    }
    fn revoke_all_for_session(&self, session_id: &str) -> usize {
        SessionAdmissionRegistry::revoke_all_for_session(self, session_id)
    }
}

/// [`RoomDirectory`] over the server's room listing.
pub struct RosterDirectory(pub Arc<dyn RoomRoster>);

#[async_trait::async_trait]
impl RoomDirectory for RosterDirectory {
    async fn participants(&self, room: &str) -> Result<Vec<String>, Status> {
        let rooms = self.0.list_rooms().await.map_err(Status::from)?;
        let listed = rooms
            .into_iter()
            .find(|listed| listed.name == room)
            .ok_or_else(|| {
                Status::not_found(format!("the room server has no room called {room}"))
            })?;
        let mut identities: Vec<String> = listed
            .participants
            .into_iter()
            .map(|participant| participant.identity)
            .collect();
        identities.sort();
        Ok(identities)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::livekit_rooms_stream::RosterError;
    use tddy_service::proto::livekit::{LiveKitParticipantInfo, LiveKitRoomInfo};

    struct ListedRooms(Vec<LiveKitRoomInfo>);

    #[async_trait::async_trait]
    impl RoomRoster for ListedRooms {
        async fn list_rooms(&self) -> Result<Vec<LiveKitRoomInfo>, RosterError> {
            Ok(self.0.clone())
        }
    }

    fn a_room_with(name: &str, identities: &[&str]) -> LiveKitRoomInfo {
        LiveKitRoomInfo {
            name: name.to_string(),
            participants: identities
                .iter()
                .map(|identity| LiveKitParticipantInfo {
                    identity: identity.to_string(),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn a_listed_rooms_participants_are_its_identities_in_order() {
        let directory = RosterDirectory(Arc::new(ListedRooms(vec![
            a_room_with("session-other", &["daemon-z"]),
            a_room_with("session-abc", &["daemon-b", "daemon-a"]),
        ])));

        let identities = directory.participants("session-abc").await.unwrap();

        assert_eq!(
            identities,
            vec!["daemon-a".to_string(), "daemon-b".to_string()]
        );
    }

    #[tokio::test]
    async fn a_room_the_server_does_not_list_is_not_found() {
        let directory = RosterDirectory(Arc::new(ListedRooms(vec![a_room_with("session-x", &[])])));

        let refused = directory.participants("session-abc").await.unwrap_err();

        assert_eq!(refused.code, tddy_rpc::Code::NotFound);
    }

    #[test]
    fn a_session_whose_room_is_not_hosted_here_has_nobody_to_hear_its_roster() {
        let registry = SessionRoomRegistry::new();

        let published = registry.publish("1780828020298-abc", "session.agents", b"roster".to_vec());

        assert!(published.is_ok());
    }

    #[test]
    fn admission_through_the_port_is_the_registrys_own() {
        let registry = SessionAdmissionRegistry::new();
        let admission: &dyn RoomAdmission = &registry;

        let recorded = admission.admit("1780828020298-abc", "peer-daemon-1");

        assert_eq!(
            (
                recorded,
                registry.is_admitted("1780828020298-abc", "peer-daemon-1")
            ),
            (true, true)
        );
    }
}
