//! A mirror joins its session's room through the client room-transport port — see `packages/tddy-room-client/docs/room-client.md`.

use std::sync::Arc;
use std::time::Duration;

use pretty_assertions::assert_eq;
use tddy_room_client::in_memory::{InMemoryRoomConnector, ScriptedJoin};
use tddy_room_client::{RoomAuth, RoomJoin};
use tddy_session_sync::credentials::LiveKitCredentials;
use tddy_session_sync::{join_session_room, AttachError, SessionAddress};

const A_WAIT: Duration = Duration::from_secs(10);

/// The token lifetime a mirror joins with: six hours, LiveKit's own default.
const SIX_HOURS: Duration = Duration::from_secs(6 * 3600);

fn the_address() -> SessionAddress {
    SessionAddress {
        session_id: "1780828020298-abc".to_string(),
        project_id: "my-app".to_string(),
        worktree_path: "/home/dev/.tddy/worktrees/my-app/feat-x".to_string(),
        daemon_instance_id: "udoo-1780828020298".to_string(),
    }
}

fn the_credentials() -> LiveKitCredentials {
    LiveKitCredentials {
        url: "ws://10.0.0.5:7880".to_string(),
        api_key: "devkey".to_string(),
        api_secret: "a-secret".to_string(),
    }
}

#[tokio::test]
async fn a_mirror_joins_the_session_room_under_its_own_identity_and_waits_for_the_daemon() {
    // Given a room — what it answers does not matter here, only what the mirror asks for
    let connector = InMemoryRoomConnector::answering(ScriptedJoin::PeerAbsent);

    // When the mirror joins
    let _ = join_session_room(
        &the_address(),
        &the_credentials(),
        "syncer-1780828020298-abc-n1",
        A_WAIT,
        &connector,
    )
    .await;

    // Then it asked for the session's room, as itself, minting from the credentials, and waited for
    // the daemon that owns the session
    assert_eq!(
        connector.joins(),
        vec![(
            RoomJoin {
                url: "ws://10.0.0.5:7880".to_string(),
                room: "session-1780828020298-abc".to_string(),
                identity: "syncer-1780828020298-abc-n1".to_string(),
                auth: RoomAuth::Credentials {
                    api_key: "devkey".to_string(),
                    api_secret: "a-secret".to_string(),
                    ttl: SIX_HOURS,
                },
            },
            "daemon-udoo-1780828020298".to_string(),
            A_WAIT,
        )]
    );
}

#[tokio::test]
async fn a_daemon_that_never_arrives_is_reported_absent() {
    // Given a room the daemon never joins
    let connector = Arc::new(InMemoryRoomConnector::answering(ScriptedJoin::PeerAbsent));

    // When the mirror joins
    let refused = join_session_room(
        &the_address(),
        &the_credentials(),
        "syncer-1780828020298-abc-n1",
        A_WAIT,
        &*connector,
    )
    .await
    .err()
    .expect("an absent daemon is an attach failure");

    // Then the failure names whom it waited for, where, and how long
    assert_eq!(
        refused,
        AttachError::DaemonAbsent {
            identity: "daemon-udoo-1780828020298".to_string(),
            room: "session-1780828020298-abc".to_string(),
            waited: A_WAIT,
        }
    );
}
