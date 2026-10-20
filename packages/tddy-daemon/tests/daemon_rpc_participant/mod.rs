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

/// The LiveKit plugin wired into `host` as the daemon wires it: its room ports over `rooms` and
/// `admissions` ([`tddy_daemon::livekit_host_ports::with_livekit_room_ports`]), plus the reach and
/// terminal exposure the runtime builds beside them. The caller keeps the two registries for
/// whatever else is built over them (the split room port, the admission service).
pub fn with_the_livekit_room_ports(
    host: DaemonSessionHost,
    rooms: Arc<tddy_daemon_livekit::session_room::SessionRoomRegistry>,
    admissions: Arc<tddy_daemon_livekit::session_admission_service::SessionAdmissionRegistry>,
) -> DaemonSessionHost {
    let config = host.config().clone();
    let reach = tddy_daemon_livekit::LiveKitSessionReach::new(
        config.clone(),
        Arc::clone(&rooms),
        tddy_daemon_extensions::IntegrationStatusBoard::new()
            .reporter(tddy_daemon_livekit::LIVEKIT_INTEGRATION_ID, "LiveKit"),
    );
    let terminal_exposure: Arc<dyn tddy_daemon_extensions::TerminalExposure> =
        match tddy_daemon_livekit::terminal_exposure::LiveKitTerminalExposure::from_config(&config)
        {
            Some(exposure) => exposure,
            None => Arc::new(tddy_daemon_extensions::NoTerminalExposure),
        };
    tddy_daemon::livekit_host_ports::with_livekit_room_ports(
        host,
        rooms,
        admissions,
        tddy_daemon_livekit::room_tokens::room_token_minter(config.livekit.as_ref()),
    )
    .with_session_reach(reach)
    .with_terminal_exposure(terminal_exposure)
}
