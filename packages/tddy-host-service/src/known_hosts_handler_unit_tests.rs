use std::sync::Arc;

use tddy_rpc::Request;
use tddy_service::proto::host::*;

use crate::host_registry::{FileHostRegistry, HostRegistry, HostSighting};
use crate::multi_host::EligibleDaemonSource;
use crate::multi_host::{DaemonInstanceId, EligibleDaemonInfo};
use crate::service::HostServiceImpl;
use tddy_daemon_kernel::SessionUserResolver;
use tddy_service::proto::host::HostService;

/// A host that is emphatically **not** the machine running the suite: every assertion about
/// `is_local` and about the roster join needs one, and a plausible hostname would make the
/// suite fail on a developer's box that happens to answer to it.
const A_REMOTE_HOST: &str = "remote-host.test.invalid";
/// The id a daemon is given by configuration, overriding its hostname.
const A_CONFIGURED_ID: &str = "configured-daemon";

/// A roster of exactly the hosts a test says are reachable.
///
/// The real sources always list the local daemon; this one lists whatever it was given, so a
/// remote host can be online, and so the registry's own guarantee about the local row is
/// provable rather than a side effect of the roster.
struct RosterOf(Vec<EligibleDaemonInfo>);

#[async_trait::async_trait]
impl EligibleDaemonSource for RosterOf {
    fn list_eligible_daemons(&self) -> Vec<EligibleDaemonInfo> {
        self.0.clone()
    }
}

fn a_roster_of(instance_ids: &[&str]) -> Arc<dyn EligibleDaemonSource> {
    Arc::new(RosterOf(
        instance_ids
            .iter()
            .map(|id| EligibleDaemonInfo {
                instance_id: DaemonInstanceId((*id).to_string()),
                label: format!("{id} (this daemon)"),
            })
            .collect(),
    ))
}

fn a_sighting_of(instance_id: &str) -> HostSighting {
    HostSighting::named(
        DaemonInstanceId(instance_id.to_string()),
        format!("{instance_id} (this daemon)"),
    )
}

fn a_config_naming_this_daemon(
    daemon_instance_id: Option<&str>,
) -> tddy_daemon_kernel::config::DaemonConfig {
    let mut yaml = "users:\n  - github_user: \"u\"\n    os_user: \"u\"\n".to_string();
    if let Some(id) = daemon_instance_id {
        // The desktop ships this pair, and it is what made one machine show up twice.
        yaml.push_str(&format!(
            "daemon_instance_id: \"{id}\"\ndaemon_instance_id_append_startup_timestamp: true\n"
        ));
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, yaml).unwrap();
    tddy_daemon_kernel::config::DaemonConfig::load(&path).unwrap()
}

/// A service over a real, empty, temp-dir-backed registry.
///
/// The store is the genuine [`FileHostRegistry`] rather than a double: the online/offline join
/// and the guarantee that the serving daemon always has a row both live in it, and a stand-in
/// that implemented them differently would let the handler's tests pass over a store that does
/// not behave that way.
fn a_service_with(
    config: tddy_daemon_kernel::config::DaemonConfig,
) -> (HostServiceImpl, Arc<dyn HostRegistry>) {
    let temp = tempfile::tempdir().unwrap();
    let _base = temp.path().to_path_buf();
    // The registry outlives the tempdir handle, which the service also holds; leaking the
    // handle keeps the directory alive for the whole test without threading it through.
    let registry: Arc<dyn HostRegistry> =
        Arc::new(FileHostRegistry::new(temp.path().join("hosts")));
    let user_resolver: SessionUserResolver = Arc::new(|token| {
        if token == "valid" {
            Some("u".to_string())
        } else {
            None
        }
    });
    let service = HostServiceImpl::new(config, temp.path(), user_resolver)
        .with_host_registry(Arc::clone(&registry));
    std::mem::forget(temp);
    (service, registry)
}

async fn known_hosts_of(service: &HostServiceImpl) -> Vec<KnownHostEntry> {
    service
        .list_known_hosts(Request::new(ListKnownHostsRequest {
            session_token: "valid".to_string(),
        }))
        .await
        .expect("list_known_hosts should succeed for a valid session")
        .into_inner()
        .hosts
}

fn entry_named<'a>(hosts: &'a [KnownHostEntry], instance_id: &str) -> &'a KnownHostEntry {
    hosts
        .iter()
        .find(|h| h.instance_id == instance_id)
        .unwrap_or_else(|| panic!("expected {instance_id} in the response"))
}

#[tokio::test]
async fn list_known_hosts_rejects_an_invalid_token() {
    // Given a service
    let (service, _registry) = a_service_with(a_config_naming_this_daemon(None));

    // When a request arrives with a token no session ever issued
    let result = service
        .list_known_hosts(Request::new(ListKnownHostsRequest {
            session_token: "nope".to_string(),
        }))
        .await;

    // Then
    assert!(result.is_err(), "an invalid session must be rejected");
    assert_eq!(result.unwrap_err().code, tddy_rpc::Code::Unauthenticated);
}

/// The live roster decides `online`, and the handler must consult it — a host it remembers but
/// cannot currently reach has to read as offline, not as merely absent.
#[tokio::test]
async fn list_known_hosts_marks_a_recorded_host_absent_from_the_roster_as_offline() {
    // Given a host recorded, then gone from the roster
    let (service, registry) = a_service_with(a_config_naming_this_daemon(None));
    registry
        .record_sighting(&a_sighting_of(A_REMOTE_HOST), 1_000)
        .expect("recording the sighting");
    registry
        .record_departure(&DaemonInstanceId(A_REMOTE_HOST.to_string()), 2_000)
        .expect("recording the departure");
    let service = service.with_eligible_daemon_source(a_roster_of(&[]));

    // When the hosts are listed
    let hosts = known_hosts_of(&service).await;

    // Then it is still listed, offline, with the stamp the row needs to say when it was last
    // reachable
    let departed = entry_named(&hosts, A_REMOTE_HOST);
    assert!(!departed.online, "a host outside the roster is offline");
    assert_eq!(departed.last_seen_unix_ms, 2_000);
}

/// `online` is a fact about the roster, not about being the machine that answered — a remote
/// host currently in the room reads as online, and is not flagged local.
#[tokio::test]
async fn list_known_hosts_marks_a_remote_host_in_the_live_roster_as_online() {
    // Given a recorded host that is also in the live roster
    let (service, registry) = a_service_with(a_config_naming_this_daemon(None));
    registry
        .record_sighting(&a_sighting_of(A_REMOTE_HOST), 1_000)
        .expect("recording the sighting");
    let service = service.with_eligible_daemon_source(a_roster_of(&[A_REMOTE_HOST]));

    // When the hosts are listed
    let hosts = known_hosts_of(&service).await;

    // Then
    let remote = entry_named(&hosts, A_REMOTE_HOST);
    assert!(remote.online, "a host in the live roster is online");
    assert!(
        !remote.is_local,
        "a peer is not the daemon serving the call"
    );
}

/// An operator must be able to see the daemon they are talking to, marked as such — even with
/// nothing recorded and nothing in the roster, which is a first boot's very first RPC.
#[tokio::test]
async fn list_known_hosts_always_includes_the_local_daemon() {
    // Given an empty registry and an empty roster
    let config = a_config_naming_this_daemon(Some(A_CONFIGURED_ID));
    let (service, _registry) = a_service_with(config);
    let service = service.with_eligible_daemon_source(a_roster_of(&[]));

    // When the hosts are listed
    let hosts = known_hosts_of(&service).await;

    // Then the serving daemon is there, flagged local and online
    let local = entry_named(&hosts, A_CONFIGURED_ID);
    assert!(local.is_local, "the serving daemon is flagged is_local");
    assert!(local.online, "the daemon answering the call is online");
}

/// One machine is one row. A configured `daemon_instance_id` (and the startup-timestamp suffix
/// the desktop ships alongside it) must not put the same host in the list twice — once from the
/// registry and once from the roster — with the daemon serving the page marked offline.
#[tokio::test]
async fn list_known_hosts_reports_one_row_for_a_daemon_whose_instance_id_is_configured() {
    // Given a daemon that names itself in configuration, having recorded itself at startup the
    // way the runtime does
    let config = a_config_naming_this_daemon(Some(A_CONFIGURED_ID));
    let local_sighting = crate::host_registry::local_host_sighting(&config);
    let (service, registry) = a_service_with(config);
    registry
        .record_sighting(&local_sighting, 1_000)
        .expect("recording this daemon's own entry");

    // When the hosts are listed
    let hosts = known_hosts_of(&service).await;

    // Then there is exactly one row for it, online and flagged local
    assert_eq!(
        hosts.len(),
        1,
        "one machine is one row; got {:?}",
        hosts.iter().map(|h| &h.instance_id).collect::<Vec<_>>()
    );
    let local = entry_named(&hosts, A_CONFIGURED_ID);
    assert!(local.is_local && local.online);
}
