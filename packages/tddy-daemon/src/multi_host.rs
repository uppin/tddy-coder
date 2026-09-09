//! Multi-host daemon identity, discoverability, and routing.

use tddy_service::proto::connection::ProjectEntry;

use crate::config::DaemonConfig;
use crate::livekit_peer_discovery::{
    local_base_instance_id_for_config, local_instance_id_for_config,
};

/// Stable identifier for a daemon instance in a shared LiveKit common room (from config).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DaemonInstanceId(pub String);

/// One row for UI / API: which daemon can run a session and how it is labeled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EligibleDaemonInfo {
    pub instance_id: DaemonInstanceId,
    pub label: String,
}

/// Source of eligible daemons for explicit selection before StartSession / ConnectSession / ResumeSession.
#[async_trait::async_trait]
pub trait EligibleDaemonSource: Send + Sync {
    fn list_eligible_daemons(&self) -> Vec<EligibleDaemonInfo>;

    /// The same hosts, keyed by **durable host id** instead of the per-process instance id.
    ///
    /// [`list_eligible_daemons`](Self::list_eligible_daemons) answers "what may I route to", so its
    /// ids must be the ones peers answer to — including the startup-timestamp suffix a daemon adds
    /// for LiveKit identity uniqueness. The host registry answers "what machines exist", which must
    /// survive a restart, so it is keyed by the id without that suffix. The two coincide for every
    /// source that does not suffix, which is why the default is the identity mapping.
    fn live_known_hosts(&self) -> Vec<EligibleDaemonInfo> {
        self.list_eligible_daemons()
    }

    /// Extra `ListProjects` rows from peer daemons (each row must set `daemon_instance_id`).
    /// Default: none. Live discovery / gRPC fan-out implementations override this.
    ///
    /// Async so the RPC handler awaits the peer fan-out directly on its runtime — no worker
    /// thread is parked bridging a sync trait method to async RPC calls.
    async fn peer_project_entries(&self, _session_token: &str) -> Vec<ProjectEntry> {
        Vec::new()
    }
}

/// Returns the local machine’s default daemon instance id (short hostname when available).
pub fn local_daemon_instance_id() -> DaemonInstanceId {
    DaemonInstanceId(local_hostname_or_local())
}

fn local_hostname_or_local() -> String {
    #[cfg(unix)]
    {
        let mut buf = [0u8; 256];
        let rc = unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) };
        if rc != 0 {
            log::debug!("local_hostname_or_local: gethostname failed rc={}", rc);
            "local".to_string()
        } else {
            let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
            let s = std::str::from_utf8(&buf[..len])
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|_| "local".to_string());
            if s.is_empty() {
                "local".to_string()
            } else {
                s
            }
        }
    }
    #[cfg(not(unix))]
    {
        "local".to_string()
    }
}

/// The eligible row for a daemon named `instance_id`, labelled the way every daemon labels itself.
///
/// One spelling of `"<id> (this daemon)"`, so the local row reads the same whether it came from
/// config, from the hostname, or from a LiveKit advertisement.
#[must_use]
pub fn eligible_daemon_entry_for(instance_id: DaemonInstanceId) -> EligibleDaemonInfo {
    let label = format!("{} (this daemon)", instance_id.0);
    EligibleDaemonInfo { instance_id, label }
}

/// One eligible row for this process, named by the machine's hostname alone.
///
/// Prefer [`LocalOnlyEligibleDaemonSource::for_config`] wherever the daemon's configuration is in
/// hand: the hostname is only the *default* instance id, and a daemon with `daemon_instance_id`
/// set answers to something else entirely.
#[must_use]
pub fn local_eligible_daemon_entry() -> EligibleDaemonInfo {
    let entry = eligible_daemon_entry_for(local_daemon_instance_id());
    log::debug!(
        "local_eligible_daemon_entry: instance_id={} label={}",
        entry.instance_id.0,
        entry.label
    );
    entry
}

/// The eligible-daemon source of a daemon with no peer discovery: this machine, and only it.
///
/// It holds **two** rows for the one host because the daemon has two ids for itself: the instance
/// id every RPC routes on (`daemon_instance_id`, plus the startup-timestamp suffix when configured)
/// and the durable host id the registry remembers it by. Deriving both here, from the same config,
/// is what keeps the Hosts screen from showing one machine as two rows — one of them the daemon
/// answering the call, marked offline.
pub struct LocalOnlyEligibleDaemonSource {
    routing_row: EligibleDaemonInfo,
    durable_row: EligibleDaemonInfo,
}

impl LocalOnlyEligibleDaemonSource {
    /// The local rows this daemon's configuration names it by.
    #[must_use]
    pub fn for_config(config: &DaemonConfig) -> Self {
        Self {
            routing_row: eligible_daemon_entry_for(DaemonInstanceId(local_instance_id_for_config(
                config,
            ))),
            durable_row: eligible_daemon_entry_for(DaemonInstanceId(
                local_base_instance_id_for_config(config),
            )),
        }
    }

    /// A source for a daemon that answers to `instance_id` and nothing else — no suffix, so the
    /// routing id and the durable id are the same.
    #[must_use]
    pub fn named(instance_id: DaemonInstanceId) -> Self {
        let row = eligible_daemon_entry_for(instance_id);
        Self {
            routing_row: row.clone(),
            durable_row: row,
        }
    }
}

impl EligibleDaemonSource for LocalOnlyEligibleDaemonSource {
    fn list_eligible_daemons(&self) -> Vec<EligibleDaemonInfo> {
        vec![self.routing_row.clone()]
    }

    fn live_known_hosts(&self) -> Vec<EligibleDaemonInfo> {
        vec![self.durable_row.clone()]
    }
}

/// Lists the local daemon under its **hostname**, ignoring any configured instance id.
///
/// Kept for callers that have no `DaemonConfig` in hand; a daemon assembling its own services has
/// one, and uses [`LocalOnlyEligibleDaemonSource::for_config`].
pub struct StubEligibleDaemonSource;

impl EligibleDaemonSource for StubEligibleDaemonSource {
    fn list_eligible_daemons(&self) -> Vec<EligibleDaemonInfo> {
        let entry = local_eligible_daemon_entry();
        log::info!(
            "StubEligibleDaemonSource: listing local daemon instance_id={}",
            entry.instance_id.0
        );
        vec![entry]
    }
}
