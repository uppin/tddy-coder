//! Durable record of every host this daemon has seen.
//!
//! [`crate::livekit_peer_discovery::CommonRoomPeerRegistry`] answers "who is in the room right
//! now": it is replaced wholesale from each room snapshot, so a host that leaves is simply gone and
//! nothing anywhere remembers it existed. That is the right model for routing an RPC — you cannot
//! forward to a peer that is not there — and the wrong one for an operator asking why the machine
//! they used this morning is not answering.
//!
//! This registry is the durable half. It records a host on sight, stamps `last_seen` when the host
//! stops being visible, and **never removes an entry**. It survives a daemon restart, so the list is
//! shared across browsers rather than living in one tab's storage.
//!
//! Liveness is deliberately *not* stored. A persisted "online" flag is wrong the moment a daemon
//! exits without notice, so [`HostRegistry::known_hosts`] takes the live roster as an argument and
//! intersects. See [`crate::connection_service`]'s `list_known_hosts`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::multi_host::{DaemonInstanceId, EligibleDaemonInfo};

/// Basename of the registry file inside the storage directory.
const REGISTRY_FILE: &str = "known-hosts.json";

/// What is remembered about a host between sightings.
///
/// Field names are the persisted JSON keys — renaming one silently drops that column for every
/// already-recorded host, which reads as "tddy forgot", so treat them as a format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnownHost {
    pub instance_id: String,
    pub label: String,
    /// First sighting. Never re-stamped — that is what makes "known since" meaningful.
    pub first_seen_unix_ms: i64,
    /// Most recent sighting.
    pub last_seen_unix_ms: i64,
    #[serde(default)]
    pub repos_base_path: String,
    #[serde(default)]
    pub max_attachment_bytes: u64,
}

/// A host as the Hosts screen reads it: what was remembered, plus liveness resolved at call time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownHostView {
    pub host: KnownHost,
    /// Present in the live roster at the moment of the call. Never persisted.
    pub online: bool,
    /// This entry is the daemon serving the call.
    pub is_local: bool,
}

/// Everything a sighting can tell us about a host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostSighting {
    pub instance_id: DaemonInstanceId,
    pub label: String,
    pub repos_base_path: String,
    pub max_attachment_bytes: u64,
}

impl HostSighting {
    /// A sighting carrying only what the eligible-daemon roster knows.
    #[must_use]
    pub fn from_eligible(info: &EligibleDaemonInfo) -> Self {
        Self {
            instance_id: info.instance_id.clone(),
            label: info.label.clone(),
            repos_base_path: String::new(),
            max_attachment_bytes: 0,
        }
    }
}

/// Records sightings durably and reports what has been seen.
///
/// A trait so the RPC layer can be handed a deterministic double, the way `HostStats` is — see
/// `ConnectionServiceImpl::with_host_stats`.
pub trait HostRegistry: Send + Sync {
    /// Record that `sighting` is present now.
    ///
    /// Creates the entry on first sight (stamping both timestamps) or advances `last_seen` and
    /// refreshes the mutable columns on a later one. `first_seen_unix_ms` is never re-stamped.
    ///
    /// Returns the write failure rather than swallowing it: a registry that cannot persist is
    /// misreporting its own contract, and the caller decides how loudly to say so.
    fn record_sighting(&self, sighting: &HostSighting, now_unix_ms: i64) -> Result<(), String>;

    /// Stamp `last_seen` for a host that has stopped being visible. **Never removes the entry.**
    ///
    /// A host absent from the registry is ignored: forgetting is the one thing this type does not do,
    /// and inventing an entry for a departure we never saw arrive would be a fabricated sighting.
    fn record_departure(
        &self,
        instance_id: &DaemonInstanceId,
        now_unix_ms: i64,
    ) -> Result<(), String>;

    /// Every host ever recorded, with liveness resolved against `live_roster`.
    ///
    /// `live_roster` is the authority on `online`; the registry is the authority on existence. A host
    /// in the roster but not yet recorded still appears — the roster is a sighting in its own right,
    /// and omitting it would hide a reachable machine.
    fn known_hosts(
        &self,
        live_roster: &[EligibleDaemonInfo],
        local_instance_id: &str,
        now_unix_ms: i64,
    ) -> Vec<KnownHostView>;
}

/// A [`HostRegistry`] persisted as one JSON file under a storage directory.
///
/// Persistence is deliberately not implemented here yet: see the changeset's testing plan for the
/// shape it must take (atomic staging-file publish, owner-only permissions, a process-wide write
/// lock around the read-modify-write, and a corrupt file reading as empty — the
/// [`crate::github_token_store::FileGitHubTokenStore`] posture).
pub struct FileHostRegistry {
    registry_path: PathBuf,
}

impl FileHostRegistry {
    /// Keep the registry in `REGISTRY_FILE` under `storage_dir`.
    pub fn new(storage_dir: impl AsRef<Path>) -> Self {
        Self {
            registry_path: storage_dir.as_ref().join(REGISTRY_FILE),
        }
    }

    /// The file the registry is persisted in.
    #[must_use]
    pub fn registry_path(&self) -> &Path {
        &self.registry_path
    }
}

impl HostRegistry for FileHostRegistry {
    fn record_sighting(&self, sighting: &HostSighting, now_unix_ms: i64) -> Result<(), String> {
        // TODO(host-registry): implement
        let _ = (sighting, now_unix_ms);
        unimplemented!("host-registry: record_sighting")
    }

    fn record_departure(
        &self,
        instance_id: &DaemonInstanceId,
        now_unix_ms: i64,
    ) -> Result<(), String> {
        // TODO(host-registry): implement
        let _ = (instance_id, now_unix_ms);
        unimplemented!("host-registry: record_departure")
    }

    fn known_hosts(
        &self,
        live_roster: &[EligibleDaemonInfo],
        local_instance_id: &str,
        now_unix_ms: i64,
    ) -> Vec<KnownHostView> {
        // TODO(host-registry): implement
        let _ = (live_roster, local_instance_id, now_unix_ms);
        unimplemented!("host-registry: known_hosts")
    }
}

/// Milliseconds since the Unix epoch, for stamping a sighting.
#[must_use]
pub fn now_unix_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const A_HOST: &str = "workstation-1";
    const ANOTHER_HOST: &str = "server-2";

    fn a_sighting_of(instance_id: &str) -> HostSighting {
        HostSighting {
            instance_id: DaemonInstanceId(instance_id.to_string()),
            label: format!("{instance_id} (this daemon)"),
            repos_base_path: "repos".to_string(),
            max_attachment_bytes: 1024,
        }
    }

    fn a_roster_of(instance_ids: &[&str]) -> Vec<EligibleDaemonInfo> {
        instance_ids
            .iter()
            .map(|id| EligibleDaemonInfo {
                instance_id: DaemonInstanceId((*id).to_string()),
                label: format!("{id} (this daemon)"),
            })
            .collect()
    }

    fn host_named<'a>(hosts: &'a [KnownHostView], instance_id: &str) -> &'a KnownHostView {
        hosts
            .iter()
            .find(|h| h.host.instance_id == instance_id)
            .unwrap_or_else(|| panic!("expected {instance_id} among the known hosts"))
    }

    /// Durability is the reason this registry exists: a host seen before a restart is still listed
    /// after one. Re-opening the store over the same directory is the in-process equivalent of the
    /// daemon coming back — the file is the only thing carried across.
    #[test]
    fn a_host_recorded_once_is_still_listed_after_the_store_is_reopened() {
        let dir = tempfile::tempdir().unwrap();

        let before_restart = FileHostRegistry::new(dir.path());
        before_restart
            .record_sighting(&a_sighting_of(A_HOST), 1_000)
            .expect("recording a sighting");

        let after_restart = FileHostRegistry::new(dir.path());
        let hosts = after_restart.known_hosts(&a_roster_of(&[]), "someone-else", 2_000);

        assert_eq!(
            hosts.len(),
            1,
            "a host recorded before the restart must survive it"
        );
        assert_eq!(host_named(&hosts, A_HOST).host.instance_id, A_HOST);
    }

    /// `first_seen` answers "how long has tddy known this machine". Re-stamping it on every sighting
    /// would silently turn that into "when did we last see it", which `last_seen` already says.
    #[test]
    fn reopening_the_store_preserves_the_original_first_seen_timestamp() {
        let dir = tempfile::tempdir().unwrap();

        let registry = FileHostRegistry::new(dir.path());
        registry
            .record_sighting(&a_sighting_of(A_HOST), 1_000)
            .expect("first sighting");
        registry
            .record_sighting(&a_sighting_of(A_HOST), 5_000)
            .expect("later sighting");

        let hosts =
            FileHostRegistry::new(dir.path()).known_hosts(&a_roster_of(&[]), "someone-else", 9_000);
        let host = host_named(&hosts, A_HOST);

        assert_eq!(
            host.host.first_seen_unix_ms, 1_000,
            "first_seen must keep the original sighting"
        );
        assert_eq!(
            host.host.last_seen_unix_ms, 5_000,
            "last_seen must advance to the latest sighting"
        );
    }

    /// The whole point of the registry: going away is not the same as never having existed.
    #[test]
    fn a_host_that_goes_away_keeps_its_entry_and_updates_last_seen() {
        let dir = tempfile::tempdir().unwrap();
        let registry = FileHostRegistry::new(dir.path());
        registry
            .record_sighting(&a_sighting_of(A_HOST), 1_000)
            .expect("sighting");

        registry
            .record_departure(&DaemonInstanceId(A_HOST.to_string()), 4_000)
            .expect("departure");

        let hosts = registry.known_hosts(&a_roster_of(&[]), "someone-else", 9_000);
        let host = host_named(&hosts, A_HOST);

        assert_eq!(
            host.host.last_seen_unix_ms, 4_000,
            "departure stamps last_seen"
        );
        assert!(!host.online, "a departed host is not online");
    }

    /// A registry file that cannot be parsed reads as empty rather than failing the daemon. Empty is
    /// the honest answer — the screen then shows only live hosts, which is the pre-registry
    /// behaviour — whereas refusing to start would take the whole daemon down over a cache.
    #[test]
    fn a_corrupt_registry_file_reads_as_an_empty_registry() {
        let dir = tempfile::tempdir().unwrap();
        let registry = FileHostRegistry::new(dir.path());
        std::fs::create_dir_all(dir.path()).unwrap();
        std::fs::write(registry.registry_path(), b"{ this is not json").unwrap();

        let hosts = registry.known_hosts(&a_roster_of(&[A_HOST]), "someone-else", 1_000);

        assert_eq!(
            hosts.len(),
            1,
            "a corrupt file yields no remembered hosts, leaving only the live roster"
        );
        assert!(
            host_named(&hosts, A_HOST).online,
            "the live host is still reported"
        );
    }

    /// A write that cannot land must be reported, not swallowed. A registry silently failing to
    /// persist looks identical to one that is working until the daemon restarts and everything is
    /// gone — which is exactly the failure this type exists to prevent.
    #[test]
    fn a_registry_that_cannot_be_written_reports_the_failure() {
        let dir = tempfile::tempdir().unwrap();
        // A file where the storage directory should be: creating the directory cannot succeed.
        let blocked = dir.path().join("blocked");
        std::fs::write(&blocked, b"not a directory").unwrap();

        let registry = FileHostRegistry::new(&blocked);
        let result = registry.record_sighting(&a_sighting_of(A_HOST), 1_000);

        assert!(
            result.is_err(),
            "an unwritable registry must report the failure"
        );
    }

    /// The roster is a sighting in its own right. A reachable host missing from the file would
    /// otherwise be hidden by the very screen meant to list every host.
    #[test]
    fn a_live_host_with_no_record_yet_is_still_reported_as_online() {
        let dir = tempfile::tempdir().unwrap();
        let registry = FileHostRegistry::new(dir.path());

        let hosts = registry.known_hosts(&a_roster_of(&[ANOTHER_HOST]), "someone-else", 1_000);

        assert!(
            host_named(&hosts, ANOTHER_HOST).online,
            "a host in the roster is online even with no prior record"
        );
    }

    /// `is_local` marks the daemon serving the call, so the screen can say "this one is me".
    #[test]
    fn the_serving_daemon_is_marked_as_the_local_host() {
        let dir = tempfile::tempdir().unwrap();
        let registry = FileHostRegistry::new(dir.path());
        registry
            .record_sighting(&a_sighting_of(A_HOST), 1_000)
            .expect("sighting");

        let hosts = registry.known_hosts(&a_roster_of(&[A_HOST]), A_HOST, 2_000);

        assert!(
            host_named(&hosts, A_HOST).is_local,
            "the serving daemon is local"
        );
    }
}
