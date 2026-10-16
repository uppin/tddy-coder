//! LiveKit's implementation of the core's [`SessionReach`] port, its `SessionRoomService`, and the
//! integration status it reports.
//!
//! A session reached by a caller that **arrived over LiveKit** — a remote client — needs the
//! session's room and its terminal there before it is answered, so the room is opened in that
//! caller's time and a failure fails its request. A session reached any other way — the desktop's
//! own window, the daemon's HTTP route, a local socket — is reached by something on this host that
//! needs no room at all; the room is opened in the background so the session becomes reachable from
//! elsewhere, and the outcome is reported rather than returned. A caller that is about to join the
//! room asks [`LiveKitSessionReach::session_room_entry`]'s `OpenSessionRoom`, which is loud.

use std::sync::Arc;

use tddy_daemon_extensions::{
    IntegrationReporter, IntegrationState, ReachableSessions, ReachedSession, SessionReach,
};
use tddy_daemon_kernel::config::{DaemonConfig, LiveKitConfig};
use tddy_rpc::Status;
use tddy_service::proto::session_room::{
    OpenSessionRoomRequest, OpenSessionRoomResponse, SessionRoomService, SessionRoomServiceServer,
};

use crate::session_room::{OpenedSessionRoom, SessionRoomRegistry};

/// LiveKit's id on the integration status board.
pub const LIVEKIT_INTEGRATION_ID: &str = "livekit";

/// What the `livekit:` block alone says about the integration, before anything is dialled.
///
/// [`IntegrationState::Disabled`] with no block, or with the common room switched off and no
/// credentials; [`IntegrationState::Misconfigured`] naming whichever of `url`, `api_key`,
/// `api_secret` is blank, in that order; otherwise [`IntegrationState::Connecting`].
pub fn livekit_integration_state(livekit: Option<&LiveKitConfig>) -> IntegrationState {
    let _ = livekit;
    todo!("livekit_integration_state")
}

/// Session rooms, opened for the sessions this daemon facilitates as they are reached.
pub struct LiveKitSessionReach {
    config: DaemonConfig,
    rooms: Arc<SessionRoomRegistry>,
    status: IntegrationReporter,
}

impl LiveKitSessionReach {
    pub fn new(
        config: DaemonConfig,
        rooms: Arc<SessionRoomRegistry>,
        status: IntegrationReporter,
    ) -> Arc<Self> {
        Arc::new(Self {
            config,
            rooms,
            status,
        })
    }

    /// Open `session`'s room (or reuse the open one) and expose its terminal there, now. `Ok(None)`
    /// when this daemon has no LiveKit credentials. Every outcome is reported on the board.
    pub async fn open_now(
        &self,
        session: &ReachedSession,
        host: Arc<dyn ReachableSessions>,
    ) -> Result<Option<OpenedSessionRoom>, Status> {
        let _ = (session, host);
        todo!("LiveKitSessionReach::open_now")
    }

    /// The `session_room.SessionRoomService` entry, resolving sessions through `sessions`.
    pub fn session_room_entry(
        self: &Arc<Self>,
        sessions: Arc<dyn ReachableSessions>,
    ) -> tddy_rpc::ServiceEntry {
        let server = SessionRoomServiceServer::new(SessionRoomServer {
            reach: Arc::clone(self),
            sessions,
        });
        tddy_rpc::ServiceEntry {
            name: "session_room.SessionRoomService",
            service: Arc::new(server) as Arc<dyn tddy_rpc::RpcService>,
        }
    }
}

/// `SessionRoomService`, answered by the reach that opens the rooms.
struct SessionRoomServer {
    reach: Arc<LiveKitSessionReach>,
    sessions: Arc<dyn ReachableSessions>,
}

#[async_trait::async_trait]
impl SessionRoomService for SessionRoomServer {
    async fn open_session_room(
        &self,
        request: tddy_rpc::Request<OpenSessionRoomRequest>,
    ) -> Result<tddy_rpc::Response<OpenSessionRoomResponse>, Status> {
        let _ = (request, &self.reach, &self.sessions);
        todo!("SessionRoomServer::open_session_room")
    }
}

#[async_trait::async_trait]
impl SessionReach for LiveKitSessionReach {
    async fn session_reached(
        &self,
        session: ReachedSession,
        host: Arc<dyn ReachableSessions>,
    ) -> Result<(), Status> {
        let _ = (session, host);
        todo!("LiveKitSessionReach::session_reached")
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    use super::*;

    /// A `livekit:` block as an operator writes it.
    fn a_livekit_block(yaml: &str) -> LiveKitConfig {
        serde_yaml::from_str(yaml).expect("the livekit block must parse")
    }

    #[test]
    fn a_daemon_with_no_livekit_block_has_livekit_disabled() {
        assert_eq!(livekit_integration_state(None), IntegrationState::Disabled);
    }

    #[test]
    fn a_switched_off_block_with_no_credentials_is_disabled_not_misconfigured() {
        // Given LiveKit switched off and never filled in
        let livekit = a_livekit_block("enabled: false\n");

        // Then nothing is missing: an operator who switched it off misconfigured nothing
        assert_eq!(
            livekit_integration_state(Some(&livekit)),
            IntegrationState::Disabled
        );
    }

    #[rstest]
    #[case::no_secret(
        "enabled: true\nurl: ws://10.0.0.5:7880\napi_key: devkey\n",
        &["api_secret"]
    )]
    #[case::blank_url(
        "enabled: true\nurl: '  '\napi_key: devkey\napi_secret: secret\n",
        &["url"]
    )]
    #[case::nothing_but_the_switch("enabled: true\n", &["url", "api_key", "api_secret"])]
    fn a_block_missing_credentials_is_misconfigured_naming_them_in_order(
        #[case] yaml: &str,
        #[case] missing: &[&str],
    ) {
        // Given a block an operator left incomplete
        let livekit = a_livekit_block(yaml);

        // Then the missing fields are named, in the order the block declares them
        assert_eq!(
            livekit_integration_state(Some(&livekit)),
            IntegrationState::Misconfigured {
                missing: missing.iter().map(|field| field.to_string()).collect(),
            }
        );
    }

    #[test]
    fn a_complete_block_is_connecting_until_the_server_answers() {
        // Given every credential a room needs
        let livekit = a_livekit_block(
            "enabled: true\nurl: ws://10.0.0.5:7880\napi_key: devkey\napi_secret: secret\n",
        );

        // Then nothing has been dialled yet, so it is neither up nor down
        assert_eq!(
            livekit_integration_state(Some(&livekit)),
            IntegrationState::Connecting
        );
    }
}
