//! A daemon's RPC participant in a LiveKit room, for the suites that carry a forwarded call over a
//! real room.
//!
//! The roster it serves is `tddy_session_lifecycle::test_util::daemon_rpc_roster`'s: which
//! coordinates a peer serves is a fact about the daemon, kept in one place. Joining the room is the
//! LiveKit plugin's business, so it is done here, in the wiring crate's tests.

// Each suite uses a subset of what is shared here.
#![allow(dead_code)]

use std::sync::Arc;

use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_session_lifecycle::test_util::daemon_rpc_roster;

/// Join `ws_url` with `token`, serving every coordinate a daemon's RPC participant answers a
/// *forwarded* call on, and run it until the returned handle is dropped or aborted.
pub async fn serve_daemon_rpc_participant(
    ws_url: &str,
    token: &str,
    service: &Arc<DaemonSessionHost>,
) -> tokio::task::JoinHandle<()> {
    let participant = tddy_livekit::LiveKitParticipant::connect(
        ws_url,
        token,
        daemon_rpc_roster(service),
        Default::default(),
        None,
        None,
    )
    .await
    .expect("daemon joins the room as its RPC participant");
    tokio::spawn(async move { participant.run().await })
}

/// The room ports a daemon with LiveKit configured wires into its session host, over `rooms` and
/// `admissions` — what `DaemonSessionHost::new` used to build for itself before LiveKit became a
/// plugin. The caller keeps the two registries for whatever else is built over them (the split
/// room port, the admission service).
pub fn with_the_livekit_room_ports(
    host: DaemonSessionHost,
    rooms: Arc<tddy_daemon_livekit::session_room::SessionRoomRegistry>,
    admissions: Arc<tddy_daemon_livekit::session_admission_service::SessionAdmissionRegistry>,
) -> DaemonSessionHost {
    let livekit = host.config().livekit.clone();
    let reach = tddy_daemon_livekit::LiveKitSessionReach::new(
        host.config().clone(),
        Arc::clone(&rooms),
        tddy_daemon_extensions::IntegrationStatusBoard::new()
            .reporter(tddy_daemon_livekit::LIVEKIT_INTEGRATION_ID, "LiveKit"),
    );
    let terminal_exposure: Arc<dyn tddy_daemon_extensions::TerminalExposure> =
        match tddy_daemon_livekit::terminal_exposure::LiveKitTerminalExposure::from_config(
            host.config(),
        ) {
            Some(exposure) => exposure,
            None => Arc::new(tddy_daemon_extensions::NoTerminalExposure),
        };
    host.with_session_rooms(rooms.clone())
        .with_session_reach(reach)
        .with_terminal_exposure(terminal_exposure)
        .with_session_delta_stores(Arc::new(
            tddy_daemon::session_delta_stores::RoomsHostedByThisDaemon::new(Arc::clone(&rooms)),
        ))
        .with_room_broadcast(rooms)
        .with_room_admission(admissions)
        .with_room_directory(Arc::new(
            tddy_daemon_livekit::agent_room_ports::RosterDirectory(
                tddy_daemon_livekit::livekit_rooms_stream::room_roster_from_config(
                    livekit.as_ref(),
                ),
            ),
        ))
        .with_room_tokens(tddy_daemon_livekit::room_tokens::room_token_minter(
            livekit.as_ref(),
        ))
        .with_room_connector(Arc::new(tddy_daemon_livekit::LiveKitRoomConnector))
}
