//! Session rooms over the hub: the `SessionReach` and `SessionRooms` ports, in memory.
//!
//! The host already speaks to session rooms through these ports; `LiveKitSessionReach` and
//! `SessionRoomRegistry` are the production implementations. These are the same ports over a
//! [`RoomHub`], so a suite that proves what happens *in* a session room — a tool call served
//! where the worktree lives, a staged file fetched back — runs without a server, while the
//! compat package keeps proving the LiveKit implementations open real rooms.

use std::sync::{Arc, Mutex};

use tddy_daemon_extensions::{
    OpenedRoom, ReachableSessions, ReachedSession, SessionReach, SessionRooms,
};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::daemon_identity::local_instance_id_for_config;
use tddy_daemon_kernel::peer_forwarding::{daemon_rpc_identity, session_room_name};
use tddy_rpc::{RequestTransport, Status};
use tddy_service::proto::session_room::{
    OpenSessionRoomRequest, OpenSessionRoomResponse, SessionRoomService, SessionRoomServiceServer,
};

use crate::RoomHub;

/// The URL an [`OpenedRoom`] over the hub names. Not a network address: a scheme that says
/// where a caller joins, and that no dialer will mistake for a LiveKit server.
pub const HUB_URL: &str = "hub://local";

/// Which sessions this daemon hosts a room for, over the hub.
#[derive(Clone)]
pub struct HubSessionRooms {
    /// session id → the participant serving it, kept for exactly as long as the room is hosted:
    /// a `HubParticipant` leaves when dropped, so hosting is holding it.
    hosted: Arc<Mutex<std::collections::HashMap<String, crate::HubParticipant>>>,
    hub: RoomHub,
}

impl HubSessionRooms {
    pub fn new(hub: RoomHub) -> Arc<Self> {
        Arc::new(Self {
            hosted: Arc::new(Mutex::new(std::collections::HashMap::new())),
            hub,
        })
    }

    fn keep_hosted(&self, session_id: &str, participant: crate::HubParticipant) {
        self.hosted
            .lock()
            .expect("hub session rooms lock")
            .insert(session_id.to_string(), participant);
    }
}

impl SessionRooms for HubSessionRooms {
    fn hosts(&self, session_id: &str) -> bool {
        self.hosted
            .lock()
            .expect("hub session rooms lock")
            .contains_key(session_id)
    }

    fn close(&self, session_id: &str) {
        self.hosted
            .lock()
            .expect("hub session rooms lock")
            .remove(session_id);
        // The room's participants leave with it: a closed room answers nobody.
        let room = session_room_name(session_id);
        for (identity, _) in self.hub.participants(&room) {
            self.hub.leave(&room, &identity);
        }
    }

    fn close_for_worktree(&self, _worktree: &std::path::Path) {
        // The hub keeps no worktree index: suites close rooms by session, as the flows they
        // exercise do. A suite that needs worktree-scoped close belongs on the real registry.
    }
}

/// Session rooms opened over the hub, as they are reached.
#[derive(Clone)]
pub struct HubSessionReach {
    hub: RoomHub,
    config: DaemonConfig,
    rooms: Arc<HubSessionRooms>,
}

impl HubSessionReach {
    pub fn new(hub: RoomHub, config: DaemonConfig, rooms: Arc<HubSessionRooms>) -> Arc<Self> {
        Arc::new(Self { hub, config, rooms })
    }

    async fn open_room(
        &self,
        session: &ReachedSession,
        host: Arc<dyn ReachableSessions>,
    ) -> Result<OpenedRoom, Status> {
        let room = session_room_name(&session.session_id);
        let server_identity = daemon_rpc_identity(&local_instance_id_for_config(&self.config));
        let participant = self.hub.join(&room, &server_identity, "");
        participant.serve(Arc::new(host.session_services()?));
        // The terminal bridge, as the LiveKit reach stages it: expose first so a participant
        // joining the room can drive the terminal the host made drivable.
        host.expose_terminal(&session.session_id).await?;
        self.rooms.keep_hosted(&session.session_id, participant);
        Ok(OpenedRoom {
            room,
            url: HUB_URL.to_string(),
            server_identity,
        })
    }

    /// The `session_room.SessionRoomService` entry, resolving sessions through `sessions`.
    pub fn session_room_entry(
        self: &Arc<Self>,
        sessions: Arc<dyn ReachableSessions>,
    ) -> tddy_rpc::ServiceEntry {
        let server = SessionRoomServiceServer::new(HubSessionRoomServer {
            reach: Arc::clone(self),
            sessions,
        });
        tddy_rpc::ServiceEntry {
            name: "session_room.SessionRoomService",
            service: Arc::new(server) as Arc<dyn tddy_rpc::RpcService>,
        }
    }
}

/// `SessionRoomService`, answered by the hub reach that opens the rooms.
struct HubSessionRoomServer {
    reach: Arc<HubSessionReach>,
    sessions: Arc<dyn ReachableSessions>,
}

#[async_trait::async_trait]
impl SessionRoomService for HubSessionRoomServer {
    async fn open_session_room(
        &self,
        request: tddy_rpc::Request<OpenSessionRoomRequest>,
    ) -> Result<tddy_rpc::Response<OpenSessionRoomResponse>, Status> {
        let request = request.into_inner();
        let session = self
            .sessions
            .resolve(&request.session_token, &request.session_id)
            .await?;
        let opened = self
            .reach
            .open_now(session, Arc::clone(&self.sessions))
            .await?
            .ok_or_else(|| {
                Status::failed_precondition("no room integration hosts rooms on this daemon")
            })?;
        Ok(tddy_rpc::Response::new(OpenSessionRoomResponse {
            livekit_room: opened.room,
            livekit_url: opened.url,
            livekit_server_identity: opened.server_identity,
        }))
    }
}

#[async_trait::async_trait]
impl SessionReach for HubSessionReach {
    async fn open_now(
        &self,
        session: ReachedSession,
        host: Arc<dyn ReachableSessions>,
    ) -> Result<Option<OpenedRoom>, Status> {
        Ok(Some(self.open_room(&session, host).await?))
    }

    async fn session_reached(
        &self,
        session: ReachedSession,
        host: Arc<dyn ReachableSessions>,
    ) -> Result<(), Status> {
        if session.arrived_over == RequestTransport::LiveKit {
            return self.open_now(session, host).await.map(drop);
        }
        let reach = self.clone();
        tokio::spawn(async move {
            let session_id = session.session_id.clone();
            if let Err(failure) = reach.open_now(session, host).await {
                log::warn!(
                    "hub session_reach: could not open the room of session {}: {}",
                    session_id,
                    failure.message
                );
            }
        });
        Ok(())
    }
}
