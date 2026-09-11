//! Shared test helpers for tddy-daemon integration and acceptance tests.
//!
//! Import with:
//! ```ignore
//! use tddy_daemon::test_util::{test_config, test_service, TEST_TOKEN, TEST_USER};
//! ```

use crate::cli_session_manager::CliSessionManager;
use crate::config::DaemonConfig;
use crate::connection_service::ConnectionServiceImpl;
use std::path::PathBuf;
use std::sync::Arc;
use tddy_daemon_kernel::{SessionUserResolver, SessionsBaseResolver};

/// Token accepted by [`test_service`] as a valid session token.
pub const TEST_TOKEN: &str = "valid-token";
/// OS user returned for [`TEST_TOKEN`] by [`test_service`].
pub const TEST_USER: &str = "testuser";

const CONFIG_YAML: &str = r#"
users:
  - github_user: "testuser"
    os_user: "testdev"
"#;

/// Build a minimal [`DaemonConfig`] suitable for unit/acceptance tests.
pub fn test_config() -> DaemonConfig {
    let dir = tempfile::tempdir().expect("create temp dir for test config");
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, CONFIG_YAML).expect("write test config");
    DaemonConfig::load(&path).expect("load test config")
}

/// Build a [`ConnectionServiceImpl`] wired to `sessions_base` with the standard test resolvers.
///
/// [`TEST_TOKEN`] resolves to [`TEST_USER`]; any other token returns `None`.
pub fn test_service(sessions_base: PathBuf) -> ConnectionServiceImpl {
    let config = test_config();
    let tddy_data_dir = sessions_base.clone();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let user_resolver: SessionUserResolver = Arc::new(|token| {
        if token == TEST_TOKEN {
            Some(TEST_USER.to_string())
        } else {
            None
        }
    });
    ConnectionServiceImpl::new(
        config,
        sessions_base_resolver,
        tddy_data_dir,
        user_resolver,
        None,
        None,
        None,
        Arc::new(CliSessionManager::new()),
    )
}

/// Block until `host.HostService` lists `peer_instance_id` among this daemon's eligible peers.
///
/// The wait is asked through the RPC, not through the roster behind it: what these suites need to
/// know before they route anything is that the *call* a client would make answers with the peer, and
/// a source that holds a row the handler would not report is exactly the failure worth catching.
///
/// The host service is built here from `service`'s own [`ConnectionServiceImpl::routing_view`], so
/// the roster this waits on is the one the connection service will classify the subsequent route
/// against. Two sources would let this return on a peer the route then cannot find.
///
/// Panics with the list that *was* returned rather than a bare timeout: "these three daemons were
/// visible and yours was not" is a different bug report from "nothing happened".
pub async fn wait_until_peer_discovered(
    service: &ConnectionServiceImpl,
    session_token: &str,
    peer_instance_id: &str,
    timeout: std::time::Duration,
) {
    use tddy_service::proto::host::HostService as _;

    // The service's *own* config, roster and token resolver — so a token this suite already uses
    // resolves here exactly as it does on the call being waited for. `tddy_data_dir` is irrelevant:
    // `ListEligibleDaemons` reads no filesystem, and a path that does not exist is a clearer
    // statement of that than a tempdir nothing writes to.
    let (config, eligible, user_resolver) = service.routing_view();
    let hosts = tddy_host_service::HostServiceImpl::new(
        config,
        std::path::Path::new("/nonexistent-list-eligible-daemons-reads-no-files"),
        user_resolver,
    )
    .with_eligible_daemon_source(eligible);

    let deadline = std::time::Instant::now() + timeout;
    loop {
        let daemons = hosts
            .list_eligible_daemons(tddy_rpc::Request::new(
                tddy_service::proto::host::ListEligibleDaemonsRequest {
                    session_token: session_token.to_string(),
                },
            ))
            .await
            .expect("ListEligibleDaemons")
            .into_inner()
            .daemons;
        if daemons.iter().any(|d| d.instance_id == peer_instance_id) {
            return;
        }
        let visible: Vec<String> = daemons.into_iter().map(|d| d.instance_id).collect();
        assert!(
            std::time::Instant::now() < deadline,
            "daemon {peer_instance_id} never appeared in ListEligibleDaemons within {timeout:?}; \
             visible instead: {visible:?}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    }
}

/// Join `ws_url` with `token`, serving every coordinate a daemon's RPC participant answers a
/// *forwarded* call on, and run it until the returned handle is dropped or aborted.
///
/// One helper rather than one per suite. Which coordinates a peer serves is a fact about the
/// daemon, not about the suite that pins a forward, and a peer that does not mount a coordinate
/// answers a forward to it with `Unknown service` — so four copies of this list is how three
/// cross-host suites came to still be serving `connection.ConnectionService` alone after
/// `#unbundle` node 6 moved the thirteen session-file RPCs onto
/// `session_files.SessionFilesService`.
///
/// The production roster is `runtime::build`'s, which mounts these two among several more; the
/// extras are the ones no forward in these suites addresses, and each needs wiring a test daemon
/// does not have.
pub async fn serve_daemon_rpc_participant(
    ws_url: &str,
    token: &str,
    service: &Arc<ConnectionServiceImpl>,
) -> tokio::task::JoinHandle<()> {
    let roster = tddy_rpc::MultiRpcService::new(vec![
        service.session_files_entry(),
        tddy_rpc::ServiceEntry {
            name: "connection.ConnectionService",
            service: Arc::new(tddy_service::ConnectionServiceServer::from_arc(Arc::clone(
                service,
            ))) as Arc<dyn tddy_rpc::RpcService>,
        },
    ]);
    let participant = tddy_livekit::LiveKitParticipant::connect(
        ws_url,
        token,
        roster,
        Default::default(),
        None,
        None,
    )
    .await
    .expect("daemon joins the room as its RPC participant");
    tokio::spawn(async move { participant.run().await })
}
