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
use std::sync::{Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use tddy_core::atomic_file::write_atomic_labelled;

use crate::config::DaemonConfig;
use crate::livekit_peer_discovery::local_instance_id_for_config;
use crate::multi_host::{DaemonInstanceId, EligibleDaemonInfo};

/// Basename of the registry file inside the storage directory.
const REGISTRY_FILE: &str = "known-hosts.json";

/// Subdirectory of the daemon data directory the registry lives in.
const REGISTRY_DIR: &str = "hosts";

/// Where a daemon rooted at `tddy_data_dir` keeps its host registry.
///
/// One function rather than the join spelled out at each call site: the daemon builds the registry
/// twice — once for the RPC surface, once for the discovery path — and two directories would mean
/// discovery recording sightings the Hosts screen never reads.
#[must_use]
pub fn host_registry_dir(tddy_data_dir: impl AsRef<Path>) -> PathBuf {
    tddy_data_dir.as_ref().join(REGISTRY_DIR)
}

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
/// The file is published through [`tddy_core::atomic_file`] rather than the hand-rolled
/// staging-file-plus-rename in [`crate::github_token_store::FileGitHubTokenStore`]. That store, and
/// the two others like it, are excluded from `atomic_file` because `write_atomic` only carries
/// permission bits over from an *existing* target, so a credential file would be created at the
/// process umask on its very first write. **A host list is not a credential** — knowing which
/// machines this daemon has seen grants nothing — so the reason for the exclusion does not apply
/// here, and reproducing the pattern would add a fourth hand-rolled writer to the set
/// `docs/dev/TODO.md` exists to shrink.
///
/// A read tolerates a missing or unparseable file by starting empty; the screen then shows the live
/// roster only, which is the pre-registry behaviour. A **write** failure is returned, because a
/// registry that silently fails to persist is indistinguishable from a working one until the
/// restart that loses everything.
pub struct FileHostRegistry {
    registry_path: PathBuf,
    /// Serialises the read-modify-write in [`Self::record_sighting`] / [`Self::record_departure`].
    ///
    /// Each write republishes the whole file, so two sightings racing without this would not
    /// interleave bytes — `rename` is atomic — but the loser would still publish a document built
    /// from a snapshot taken before the winner's, dropping that host entirely. The daemon holds one
    /// registry, so one lock here serialises every writer in the process.
    write_lock: Mutex<()>,
}

impl FileHostRegistry {
    /// Keep the registry in `REGISTRY_FILE` under `storage_dir`.
    pub fn new(storage_dir: impl AsRef<Path>) -> Self {
        Self {
            registry_path: storage_dir.as_ref().join(REGISTRY_FILE),
            write_lock: Mutex::new(()),
        }
    }

    /// The file the registry is persisted in.
    #[must_use]
    pub fn registry_path(&self) -> &Path {
        &self.registry_path
    }

    /// Hold the write lock, recovering from poisoning instead of propagating it.
    ///
    /// The lock guards a file, not the `()` behind it, and the file is only ever replaced whole —
    /// so a thread that panicked mid-write left nothing torn for the next one to inherit.
    fn write_guard(&self) -> MutexGuard<'_, ()> {
        self.write_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Every recorded host, or none when the file is absent or unreadable.
    fn read_all(&self) -> Vec<KnownHost> {
        // No file is the ordinary state of a daemon that has not seen anyone yet, so it is not
        // worth a log line; a file that will not parse is.
        let Ok(bytes) = std::fs::read(&self.registry_path) else {
            return Vec::new();
        };
        serde_json::from_slice(&bytes).unwrap_or_else(|e| {
            log::warn!(
                "host registry {} could not be parsed ({e}); continuing with an empty registry",
                self.registry_path.display()
            );
            Vec::new()
        })
    }

    /// Publish `hosts` as the whole registry.
    fn write_all(&self, hosts: &[KnownHost]) -> Result<(), String> {
        let json = serde_json::to_vec_pretty(hosts)
            .map_err(|e| format!("serializing the host registry: {e}"))?;
        write_atomic_labelled(&self.registry_path, json)
    }
}

impl HostRegistry for FileHostRegistry {
    fn record_sighting(&self, sighting: &HostSighting, now_unix_ms: i64) -> Result<(), String> {
        let _guard = self.write_guard();
        let mut hosts = self.read_all();
        match hosts
            .iter_mut()
            .find(|host| host.instance_id == sighting.instance_id.0)
        {
            Some(known) => {
                known.last_seen_unix_ms = now_unix_ms;
                // Refreshed only when the sighting actually carries the column. A roster-derived
                // sighting knows the id and the label and nothing else
                // (`HostSighting::from_eligible`), so an empty value there means "not observed",
                // not "now empty" — writing it over a repos path recorded from a richer sighting
                // would blank a column the screen has already shown.
                if !sighting.label.is_empty() {
                    known.label = sighting.label.clone();
                }
                if !sighting.repos_base_path.is_empty() {
                    known.repos_base_path = sighting.repos_base_path.clone();
                }
                if sighting.max_attachment_bytes != 0 {
                    known.max_attachment_bytes = sighting.max_attachment_bytes;
                }
            }
            None => hosts.push(KnownHost {
                instance_id: sighting.instance_id.0.clone(),
                label: sighting.label.clone(),
                first_seen_unix_ms: now_unix_ms,
                last_seen_unix_ms: now_unix_ms,
                repos_base_path: sighting.repos_base_path.clone(),
                max_attachment_bytes: sighting.max_attachment_bytes,
            }),
        }
        self.write_all(&hosts)
    }

    fn record_departure(
        &self,
        instance_id: &DaemonInstanceId,
        now_unix_ms: i64,
    ) -> Result<(), String> {
        let _guard = self.write_guard();
        let mut hosts = self.read_all();
        let Some(known) = hosts
            .iter_mut()
            .find(|host| host.instance_id == instance_id.0)
        else {
            // A departure we never saw arrive. Creating an entry for it would record a sighting
            // that never happened, so there is nothing to persist and nothing to report.
            return Ok(());
        };
        known.last_seen_unix_ms = now_unix_ms;
        self.write_all(&hosts)
    }

    fn known_hosts(
        &self,
        live_roster: &[EligibleDaemonInfo],
        local_instance_id: &str,
        now_unix_ms: i64,
    ) -> Vec<KnownHostView> {
        let mut views: Vec<KnownHostView> = self
            .read_all()
            .into_iter()
            .map(|host| KnownHostView {
                online: live_roster
                    .iter()
                    .any(|live| live.instance_id.0 == host.instance_id),
                is_local: host.instance_id == local_instance_id,
                host,
            })
            .collect();

        // The roster is a sighting in its own right: a machine answering right now is known about,
        // whatever the file says, and hiding it would be the one failure this screen exists to
        // prevent. The entry is not written back — a read stays a read, and the discovery path is
        // what turns a roster into a durable record.
        for live in live_roster {
            if views
                .iter()
                .any(|view| view.host.instance_id == live.instance_id.0)
            {
                continue;
            }
            views.push(KnownHostView {
                host: KnownHost {
                    instance_id: live.instance_id.0.clone(),
                    label: live.label.clone(),
                    first_seen_unix_ms: now_unix_ms,
                    last_seen_unix_ms: now_unix_ms,
                    repos_base_path: String::new(),
                    max_attachment_bytes: 0,
                },
                online: true,
                is_local: live.instance_id.0 == local_instance_id,
            });
        }
        views
    }
}

/// This daemon's sighting of itself.
///
/// [`crate::livekit_peer_discovery::CommonRoomPeerRegistry`] deliberately excludes the local row —
/// it answers "who *else* is in the room" — so without this the one host an operator is certainly
/// looking at would be the only one never recorded. Unlike a roster sighting, this one can fill the
/// host facts in, because they are read from this daemon's own configuration.
#[must_use]
pub fn local_host_sighting(config: &DaemonConfig) -> HostSighting {
    let instance_id = local_instance_id_for_config(config);
    HostSighting {
        label: format!("{instance_id} (this daemon)"),
        instance_id: DaemonInstanceId(instance_id),
        repos_base_path: config.repos_base_path_or_default().to_string(),
        max_attachment_bytes: config.max_attachment_bytes,
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
