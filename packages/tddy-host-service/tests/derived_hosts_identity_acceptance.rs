//! A derived host names its parent by the id the web addresses that daemon with, and an online
//! known host says which id that is (`#remote-shell` 9/11).
//!
//! A daemon has two ids: the **durable** one the host registry files it under, and the **routing**
//! one it answers RPCs on, which differs when `daemon_instance_id_append_startup_timestamp` is set
//! — the installed desktop's configuration. The web addresses a daemon by the routing id (the
//! common-room advertisement and `/api/config` carry it), so a derived host whose parent is the
//! durable id is offered under a daemon the web does not know, and a Hosts-screen row keyed on the
//! durable id has no connection to ask through. These tests run the real handlers under a config
//! that sets the flag, so the two ids differ as they do on the desktop.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-11-16-remote-shell-derived-host-identity.md

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pretty_assertions::assert_eq;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::daemon_identity::{
    local_base_instance_id_for_config, local_instance_id_for_config,
};
use tddy_host_service::host_private_key::HostUserFiles;
use tddy_host_service::host_registry::{
    host_registry_dir, now_unix_ms, FileHostRegistry, HostRegistry, HostSighting,
};
use tddy_host_service::multi_host::{DaemonInstanceId, EligibleDaemonInfo, EligibleDaemonSource};
use tddy_host_service::test_util::{test_config, test_user_resolver, TEST_TOKEN};
use tddy_host_service::HostServiceImpl;
use tddy_rpc::Request;
use tddy_service::proto::host::{
    HostService, KnownHostEntry, ListDerivedHostsRequest, ListKnownHostsRequest,
};

/// An empty `daemon_instance_id` is the protocol's spelling for "the daemon serving this call".
const THIS_DAEMON: &str = "";
/// The machine's durable name, as an operator configured it.
const THE_MACHINE: &str = "workstation-1";
/// A peer this daemon has seen in the room, by its durable name.
const A_PEER: &str = "server-2";
/// A listing reads one file; this only turns a hang into a failure.
const LISTING_WINDOW: Duration = Duration::from_secs(2);

/// The operator's files, read from a home on a temp path — whichever OS user is asked for.
struct AnOperatorHome {
    home: PathBuf,
}

impl HostUserFiles for AnOperatorHome {
    fn home_dir(&self, _os_user: &str) -> Result<PathBuf, String> {
        Ok(self.home.clone())
    }

    fn read_as_user(&self, _os_user: &str, path: &Path) -> Result<Vec<u8>, String> {
        std::fs::read(path).map_err(|e| e.to_string())
    }

    fn list_files_as_user(&self, _os_user: &str, dir: &Path) -> Result<Vec<PathBuf>, String> {
        Err(format!("{} is not listed by this stand-in", dir.display()))
    }
}

/// The room as this daemon sees it: peers by the routing id they answer on, each filed under its
/// durable id — what a source that strips the startup suffix answers. The roster can be emptied,
/// which is how a peer goes offline between two calls.
struct ARoom {
    /// `(durable id, routing id)` per live peer.
    peers: Mutex<Vec<(String, String)>>,
}

impl ARoom {
    fn with(peers: &[(&str, &str)]) -> Arc<Self> {
        Arc::new(Self {
            peers: Mutex::new(
                peers
                    .iter()
                    .map(|(durable, routing)| (durable.to_string(), routing.to_string()))
                    .collect(),
            ),
        })
    }

    fn everyone_leaves(&self) {
        self.peers.lock().expect("the room is not poisoned").clear();
    }
}

impl EligibleDaemonSource for ARoom {
    fn list_eligible_daemons(&self) -> Vec<EligibleDaemonInfo> {
        self.peers
            .lock()
            .expect("the room is not poisoned")
            .iter()
            .map(|(_, routing)| EligibleDaemonInfo {
                instance_id: DaemonInstanceId(routing.clone()),
                label: routing.clone(),
            })
            .collect()
    }

    fn live_known_hosts(&self) -> Vec<EligibleDaemonInfo> {
        self.peers
            .lock()
            .expect("the room is not poisoned")
            .iter()
            .map(|(durable, _)| EligibleDaemonInfo {
                instance_id: DaemonInstanceId(durable.clone()),
                label: durable.clone(),
            })
            .collect()
    }
}

/// A daemon configured as the installed desktop is: a durable name, and a routing id made unique
/// per run by its startup timestamp.
fn a_config_that_stamps_its_runs() -> DaemonConfig {
    let mut config = test_config();
    config.daemon_instance_id = Some(THE_MACHINE.to_string());
    config.daemon_instance_id_append_startup_timestamp = true;
    config
}

/// A daemon serving `HostService` under that config, whose operator reaches `buildbox` over SSH,
/// in a room of `peers` — each of which its registry has already recorded, as the discovery tick
/// does for every peer it sees.
struct ADaemon {
    config: DaemonConfig,
    service: HostServiceImpl,
    room: Arc<ARoom>,
    _home: tempfile::TempDir,
    _tddy_data: tempfile::TempDir,
}

fn a_daemon_that_stamps_its_runs(peers: &[(&str, &str)]) -> ADaemon {
    let home_storage = tempfile::tempdir().expect("operator home storage");
    let home = home_storage.path().to_path_buf();
    std::fs::create_dir_all(home.join(".ssh")).expect("the operator has ~/.ssh");
    std::fs::write(home.join(".ssh").join("config"), "Host buildbox\n").expect("ssh config");
    let tddy_data = tempfile::tempdir().expect("tddy data dir");
    let config = a_config_that_stamps_its_runs();
    let room = ARoom::with(peers);
    let registry = Arc::new(FileHostRegistry::new(host_registry_dir(tddy_data.path())));
    for (durable, _) in peers {
        registry
            .record_sighting(
                &HostSighting {
                    instance_id: DaemonInstanceId((*durable).to_string()),
                    label: (*durable).to_string(),
                    repos_base_path: Some("repos".to_string()),
                    max_attachment_bytes: None,
                },
                now_unix_ms(),
            )
            .expect("the peer is recorded");
    }
    let service = HostServiceImpl::new(config.clone(), tddy_data.path(), test_user_resolver())
        .with_host_user_files(Arc::new(AnOperatorHome { home }))
        .with_host_registry(registry)
        .with_eligible_daemon_source(Arc::clone(&room) as Arc<dyn EligibleDaemonSource>);
    ADaemon {
        config,
        service,
        room,
        _home: home_storage,
        _tddy_data: tddy_data,
    }
}

impl ADaemon {
    /// The id this daemon answers RPCs on: what the web addresses it with.
    fn routing_id(&self) -> String {
        local_instance_id_for_config(&self.config)
    }

    /// The id the host registry files this daemon under.
    fn durable_id(&self) -> String {
        local_base_instance_id_for_config(&self.config)
    }

    /// The parent every derived host this daemon lists names.
    async fn the_parent_its_derived_hosts_name(&self) -> Vec<String> {
        let listed = tokio::time::timeout(
            LISTING_WINDOW,
            self.service
                .list_derived_hosts(Request::direct(ListDerivedHostsRequest {
                    session_token: TEST_TOKEN.to_string(),
                    daemon_instance_id: THIS_DAEMON.to_string(),
                })),
        )
        .await
        .expect("a listing reads one file and must not hang")
        .expect("a listing is answered, whatever its outcome")
        .into_inner();
        assert!(
            !listed.hosts.is_empty(),
            "the operator's config names buildbox, so one host is listed"
        );
        listed
            .hosts
            .into_iter()
            .map(|host| host.parent_instance_id)
            .collect()
    }

    async fn known_hosts(&self) -> Vec<KnownHostEntry> {
        self.service
            .list_known_hosts(Request::direct(ListKnownHostsRequest {
                session_token: TEST_TOKEN.to_string(),
            }))
            .await
            .expect("the known hosts are listed")
            .into_inner()
            .hosts
    }

    async fn known_host(&self, instance_id: &str) -> KnownHostEntry {
        self.known_hosts()
            .await
            .into_iter()
            .find(|host| host.instance_id == instance_id)
            .unwrap_or_else(|| panic!("{instance_id} is a known host"))
    }
}

/// What the Hosts screen reads off a row to address it: filed under, online, answers on.
fn addressed_as(host: &KnownHostEntry) -> (String, bool, String) {
    (
        host.instance_id.clone(),
        host.online,
        host.routing_instance_id.clone(),
    )
}

#[tokio::test]
async fn a_derived_hosts_parent_is_the_id_its_daemon_answers_on() {
    // Given a daemon whose routing id carries its startup stamp, so it differs from its durable id
    let daemon = a_daemon_that_stamps_its_runs(&[]);
    assert_ne!(
        daemon.routing_id(),
        daemon.durable_id(),
        "the flag separates the two ids"
    );

    // When it lists the hosts it reaches
    let parents = daemon.the_parent_its_derived_hosts_name().await;

    // Then every host names the id the web addresses this daemon with, not the registry's
    assert_eq!(parents, vec![daemon.routing_id()]);
}

#[tokio::test]
async fn an_online_known_host_carries_the_routing_id_it_answers_on() {
    // Given this daemon, and a peer in the room filed under `server-2` and answering on a stamped id
    let daemon = a_daemon_that_stamps_its_runs(&[(A_PEER, "server-2-1794836073070")]);

    // When the known hosts are listed
    let this_daemon = daemon.known_host(&daemon.durable_id()).await;
    let the_peer = daemon.known_host(A_PEER).await;

    // Then each online row is filed under its durable id and says which id it answers on
    assert_eq!(
        (addressed_as(&this_daemon), addressed_as(&the_peer)),
        (
            (daemon.durable_id(), true, daemon.routing_id()),
            (
                A_PEER.to_string(),
                true,
                "server-2-1794836073070".to_string()
            ),
        )
    );
}

#[tokio::test]
async fn an_offline_known_host_carries_no_routing_id() {
    // Given a peer the registry recorded while it was in the room, which has since left it
    let daemon = a_daemon_that_stamps_its_runs(&[(A_PEER, "server-2-1794836073070")]);
    daemon.room.everyone_leaves();

    // When the known hosts are listed again
    let the_peer = daemon.known_host(A_PEER).await;

    // Then the peer is still filed, offline, and answers on nothing
    assert_eq!(
        addressed_as(&the_peer),
        (A_PEER.to_string(), false, String::new())
    );
}
