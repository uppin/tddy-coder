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
//!
//! # Identity
//!
//! Entries are keyed by the **durable host id** — the un-suffixed
//! [`crate::livekit_peer_discovery::local_base_instance_id_for_config`], not the per-process
//! instance id LiveKit routes on. A daemon configured with
//! `daemon_instance_id_append_startup_timestamp` gets a fresh instance id on every restart, and
//! "never delete a host" would otherwise turn every restart into a permanent extra offline row.
//!
//! # Write policy
//!
//! Sightings arrive on a 500 ms discovery tick, so the store must not write per tick. It holds the
//! document in memory and republishes the file **only when a snapshot actually changes it** — a new
//! host, a departure, a changed column, or a `last_seen` older than
//! [`LAST_SEEN_REFRESH_FLOOR_MS`]. A tick that sees exactly what the last one saw, recently, touches
//! no disk at all.
//!
//! # Peer-supplied input
//!
//! `instance_id`, `label` and `repos_base_path` come from a peer's self-declared common-room
//! metadata and land in permanent storage that has no deletion RPC. They are bounded here, at the
//! one boundary every writer passes through, rather than at each call site.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use tddy_core::atomic_file::write_atomic_labelled;

use crate::config::DaemonConfig;
use crate::livekit_peer_discovery::local_base_instance_id_for_config;
use crate::multi_host::{DaemonInstanceId, EligibleDaemonInfo};

/// Basename of the registry file inside the storage directory.
const REGISTRY_FILE: &str = "known-hosts.json";

/// Subdirectory of the daemon data directory the registry lives in.
const REGISTRY_DIR: &str = "hosts";

/// Longest host id that may be persisted.
///
/// An over-long or oddly-shaped id is **rejected**, never truncated: a truncated id names a
/// different host, so recording it would invent a machine that does not exist.
const MAX_INSTANCE_ID_BYTES: usize = 128;

/// Longest label that may be persisted. A label is display text, so truncating one is honest.
const MAX_LABEL_BYTES: usize = 256;

/// Longest `repos_base_path` that may be persisted. A truncated path is a *wrong* path, so an
/// over-long one is dropped (recorded as not observed) rather than cut down.
const MAX_REPOS_BASE_PATH_BYTES: usize = 512;

/// How stale a recorded `last_seen` may become while its host is still visible.
///
/// **A staleness bound, not a cache TTL, and not a redundant write** — deleting it silently breaks
/// the screen this registry exists for. Without it a host visible without interruption produces no
/// material change, so its stamp never advances: a peer up for a month, then gone while this daemon
/// was itself down (so no departure is observed either, the post-restart snapshot having nothing to
/// diff against), would read "Offline, last seen a month ago" for a machine that answered
/// yesterday. That is the exact question an operator opens this screen to ask, answered wrongly.
///
/// An hour bounds the error to something read as accurate while still costing ~20,000× fewer
/// writes than stamping every 500 ms tick.
const LAST_SEEN_REFRESH_FLOOR_MS: i64 = 60 * 60 * 1000;

/// Ceiling on entries. The registry never deletes, and ids come from self-declared peer metadata,
/// so a peer reconnecting under fresh random ids would otherwise grow the file without bound and
/// re-serialize all of it on every write. Past the cap, known hosts still update; new ones are
/// refused with a warning, which is the conservative half of "never delete".
const MAX_KNOWN_HOSTS: usize = 512;

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
    /// The durable host id (see the module's *Identity* section), not the per-process instance id.
    pub instance_id: String,
    pub label: String,
    /// First sighting. Never re-stamped — that is what makes "known since" meaningful.
    pub first_seen_unix_ms: i64,
    /// Most recent sighting that changed anything, or the moment of departure.
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
///
/// The optional columns say *not observed* in the type rather than in a comment: a roster row
/// carries an id and a label and nothing else, and an absent `repos_base_path` must never blank one
/// a richer sighting already recorded. It also leaves `Some(0)` free to mean a genuine zero.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostSighting {
    /// The durable host id this sighting is recorded under.
    pub instance_id: DaemonInstanceId,
    pub label: String,
    pub repos_base_path: Option<String>,
    pub max_attachment_bytes: Option<u64>,
}

impl HostSighting {
    /// A sighting carrying only what a roster row knows: the host's id and its label.
    #[must_use]
    pub fn named(instance_id: DaemonInstanceId, label: impl Into<String>) -> Self {
        Self {
            instance_id,
            label: label.into(),
            repos_base_path: None,
            max_attachment_bytes: None,
        }
    }
}

/// Records sightings durably and reports what has been seen.
///
/// A trait so the RPC layer can be handed a deterministic double, the way `HostStats` is — see
/// `ConnectionServiceImpl::with_host_stats`.
pub trait HostRegistry: Send + Sync {
    /// Record one whole snapshot: everyone visible now, and everyone who has stopped being visible.
    ///
    /// The batch is the primitive rather than a convenience over the singular calls, because the
    /// discovery path produces snapshots — recording a three-peer room one peer at a time would be
    /// three read-modify-write-fsync cycles for one observation, several times a second, forever.
    ///
    /// Returns the write failure rather than swallowing it: a registry that cannot persist is
    /// misreporting its own contract, and the caller decides how loudly to say so.
    fn record_snapshot(
        &self,
        seen: &[HostSighting],
        departed: &[DaemonInstanceId],
        now_unix_ms: i64,
    ) -> Result<(), String>;

    /// Record that `sighting` is present now.
    ///
    /// Creates the entry on first sight (stamping both timestamps) or refreshes the mutable columns
    /// on a later one. `first_seen_unix_ms` is never re-stamped.
    fn record_sighting(&self, sighting: &HostSighting, now_unix_ms: i64) -> Result<(), String> {
        self.record_snapshot(std::slice::from_ref(sighting), &[], now_unix_ms)
    }

    /// Stamp `last_seen` for a host that has stopped being visible. **Never removes the entry.**
    ///
    /// A host absent from the registry is ignored: forgetting is the one thing this type does not do,
    /// and inventing an entry for a departure we never saw arrive would be a fabricated sighting.
    fn record_departure(
        &self,
        instance_id: &DaemonInstanceId,
        now_unix_ms: i64,
    ) -> Result<(), String> {
        self.record_snapshot(&[], std::slice::from_ref(instance_id), now_unix_ms)
    }

    /// Every host ever recorded, with liveness resolved against `live_roster`.
    ///
    /// `live_roster` is the authority on `online`; the registry is the authority on existence. A host
    /// in the roster but not yet recorded still appears — the roster is a sighting in its own right,
    /// and omitting it would hide a reachable machine. `live_roster` rows are keyed by **durable**
    /// host id, the same key the registry stores, so the intersection is a straight comparison.
    ///
    /// `local` is the serving daemon's own account of itself, and the row for it is guaranteed: it
    /// is, demonstrably, right here answering, so it can never legitimately be missing however
    /// little the file has managed to record. Building that row here rather than in the handler
    /// keeps the whole registry-plus-roster join in one place.
    fn known_hosts(
        &self,
        live_roster: &[EligibleDaemonInfo],
        local: &HostSighting,
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
    /// The document, once loaded, and the lock serialising every read-modify-write.
    ///
    /// In memory because the discovery tick asks several times a second and the file is the slow,
    /// fsync-bearing part; `None` until the first access, so a store opened over a directory
    /// written by a previous process still picks that file up (which is what makes the
    /// restart-durability test a real restart).
    ///
    /// The cache is only advanced once a write has landed. A failed write therefore leaves memory
    /// describing what is actually on disk, and the next snapshot retries rather than believing a
    /// change it never persisted.
    ///
    /// A read of the registry does contend with a write in progress, and a write fsyncs — but a
    /// write now happens only when a snapshot changes something, so an `ListKnownHosts` waiting
    /// behind one is a rarity rather than the every-500-ms certainty it would be if each tick
    /// republished the file.
    document: Mutex<Option<Vec<KnownHost>>>,
}

impl FileHostRegistry {
    /// Keep the registry in `REGISTRY_FILE` under `storage_dir`.
    pub fn new(storage_dir: impl AsRef<Path>) -> Self {
        Self {
            registry_path: storage_dir.as_ref().join(REGISTRY_FILE),
            document: Mutex::new(None),
        }
    }

    /// Hold the document lock, recovering from poisoning instead of propagating it.
    ///
    /// The cache is only ever replaced whole, and only after a successful write — so a thread that
    /// panicked mid-snapshot left nothing half-applied for the next one to inherit.
    fn locked(&self) -> MutexGuard<'_, Option<Vec<KnownHost>>> {
        self.document
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The cached document, reading the file on first access.
    fn loaded<'a>(&self, guard: &'a mut Option<Vec<KnownHost>>) -> &'a mut Vec<KnownHost> {
        guard.get_or_insert_with(|| self.read_from_disk())
    }

    /// Every recorded host, or none when the file is absent, unreadable or unparseable.
    fn read_from_disk(&self) -> Vec<KnownHost> {
        let bytes = match std::fs::read(&self.registry_path) {
            Ok(bytes) => bytes,
            // No file is the ordinary state of a daemon that has not seen anyone yet.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
            // Anything else — a permissions problem, an I/O error, a directory where the file
            // should be — makes every host vanish, and doing that silently leaves an operator with
            // an empty screen and no thread to pull.
            Err(e) => {
                log::warn!(
                    "host registry {} could not be read ({e}); continuing with an empty registry",
                    self.registry_path.display()
                );
                return Vec::new();
            }
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
        let json = serde_json::to_vec_pretty(hosts).map_err(|e| {
            format!(
                "serializing the host registry {}: {e}",
                self.registry_path.display()
            )
        })?;
        write_atomic_labelled(&self.registry_path, json)
    }
}

/// Apply one sighting to `hosts`, reporting whether it changed anything worth a write.
///
/// A `last_seen` that merely advanced is deliberately **not** a change — it advances on every tick,
/// so counting it would make every tick a write, which is the whole thing this store avoids. One
/// that has fallen [`LAST_SEEN_REFRESH_FLOOR_MS`] behind *is*, because a stamp allowed to age
/// without limit stops meaning anything.
fn apply_sighting(hosts: &mut Vec<KnownHost>, sighting: &HostSighting, now_unix_ms: i64) -> bool {
    let instance_id = match validated_instance_id(&sighting.instance_id.0) {
        Ok(id) => id,
        Err(e) => {
            // One malformed peer must not cost the rest of the snapshot its recording.
            log::warn!("host registry: refusing to record a host: {e}");
            return false;
        }
    };
    let label = bounded_label(&sighting.label);
    let repos_base_path = bounded_repos_base_path(sighting.repos_base_path.as_deref());

    let Some(known) = hosts
        .iter_mut()
        .find(|host| host.instance_id == instance_id)
    else {
        if hosts.len() >= MAX_KNOWN_HOSTS {
            log::warn!(
                "host registry: at the {MAX_KNOWN_HOSTS}-host ceiling; not recording new host {instance_id}"
            );
            return false;
        }
        hosts.push(KnownHost {
            label: if label.is_empty() {
                instance_id.clone()
            } else {
                label
            },
            instance_id,
            first_seen_unix_ms: now_unix_ms,
            last_seen_unix_ms: now_unix_ms,
            repos_base_path: repos_base_path.unwrap_or_default(),
            max_attachment_bytes: sighting.max_attachment_bytes.unwrap_or_default(),
        });
        return true;
    };

    let mut changed =
        now_unix_ms.saturating_sub(known.last_seen_unix_ms) >= LAST_SEEN_REFRESH_FLOOR_MS;
    if !label.is_empty() && known.label != label {
        known.label = label;
        changed = true;
    }
    if let Some(repos_base_path) = repos_base_path {
        if known.repos_base_path != repos_base_path {
            known.repos_base_path = repos_base_path;
            changed = true;
        }
    }
    if let Some(max_attachment_bytes) = sighting.max_attachment_bytes {
        if known.max_attachment_bytes != max_attachment_bytes {
            known.max_attachment_bytes = max_attachment_bytes;
            changed = true;
        }
    }
    changed
}

/// Stamp a departure, reporting whether there was an entry to stamp.
fn apply_departure(
    hosts: &mut [KnownHost],
    instance_id: &DaemonInstanceId,
    now_unix_ms: i64,
) -> bool {
    let Some(known) = hosts
        .iter_mut()
        .find(|host| host.instance_id == instance_id.0.trim())
    else {
        // A departure we never saw arrive. Creating an entry for it would record a sighting that
        // never happened, so there is nothing to persist and nothing to report.
        return false;
    };
    if known.last_seen_unix_ms == now_unix_ms {
        return false;
    }
    known.last_seen_unix_ms = now_unix_ms;
    true
}

/// A host id fit to be written down, or why it is not.
fn validated_instance_id(raw: &str) -> Result<String, String> {
    let id = raw.trim();
    if id.is_empty() {
        return Err("the host id is empty".to_string());
    }
    if id.len() > MAX_INSTANCE_ID_BYTES {
        return Err(format!(
            "the host id is {} bytes, over the {MAX_INSTANCE_ID_BYTES}-byte limit",
            id.len()
        ));
    }
    if let Some(bad) = id.chars().find(|c| !is_instance_id_char(*c)) {
        return Err(format!("the host id {id:?} contains {bad:?}"));
    }
    Ok(id.to_string())
}

/// The characters a host id may be spelled with: hostnames, configured ids and the startup-suffix
/// digits, and nothing that could turn a registry row into markup or a path.
fn is_instance_id_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':' | '@' | '+')
}

/// A label short enough to store. Display text, so cutting it down is honest.
fn bounded_label(raw: &str) -> String {
    truncated_to(raw.trim(), MAX_LABEL_BYTES).to_string()
}

/// An observed repos base path short enough to store, or nothing observed.
fn bounded_repos_base_path(raw: Option<&str>) -> Option<String> {
    let path = raw?.trim();
    if path.is_empty() {
        return None;
    }
    if path.len() > MAX_REPOS_BASE_PATH_BYTES {
        // Half a path points somewhere else entirely, so this column is better left unset than
        // shown wrong.
        log::warn!(
            "host registry: ignoring a repos base path of {} bytes (over the {MAX_REPOS_BASE_PATH_BYTES}-byte limit)",
            path.len()
        );
        return None;
    }
    Some(path.to_string())
}

/// `text` cut to at most `max_bytes`, never mid-character.
fn truncated_to(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

impl HostRegistry for FileHostRegistry {
    fn record_snapshot(
        &self,
        seen: &[HostSighting],
        departed: &[DaemonInstanceId],
        now_unix_ms: i64,
    ) -> Result<(), String> {
        let mut guard = self.locked();
        let mut next = self.loaded(&mut guard).clone();

        let mut changed = false;
        for sighting in seen {
            changed |= apply_sighting(&mut next, sighting, now_unix_ms);
        }
        for instance_id in departed {
            changed |= apply_departure(&mut next, instance_id, now_unix_ms);
        }
        if !changed {
            return Ok(());
        }

        // A write is happening anyway, so every host visible in this snapshot gets a fresh stamp
        // for free — that is what keeps `last_seen` honest without a write per tick.
        for sighting in seen {
            let id = sighting.instance_id.0.trim();
            if let Some(host) = next.iter_mut().find(|host| host.instance_id == id) {
                host.last_seen_unix_ms = now_unix_ms;
            }
        }

        self.write_all(&next)?;
        *guard = Some(next);
        Ok(())
    }

    fn known_hosts(
        &self,
        live_roster: &[EligibleDaemonInfo],
        local: &HostSighting,
        now_unix_ms: i64,
    ) -> Vec<KnownHostView> {
        let local_id = local.instance_id.0.trim();
        let mut guard = self.locked();
        let mut views: Vec<KnownHostView> = self
            .loaded(&mut guard)
            .iter()
            .cloned()
            .map(|host| KnownHostView {
                // The serving daemon is online by construction: it is answering this very call, so
                // whether a roster happens to list it says nothing.
                online: host.instance_id == local_id
                    || live_roster
                        .iter()
                        .any(|live| live.instance_id.0 == host.instance_id),
                is_local: host.instance_id == local_id,
                host,
            })
            .collect();
        drop(guard);

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
                is_local: live.instance_id.0 == local_id,
            });
        }

        // The one host that can never legitimately be missing. A first boot has recorded nothing
        // before the first RPC, and an unwritable registry never will, but an operator must still
        // see the machine they are talking to — filled in from what this daemon knows about itself
        // first-hand.
        if !views.iter().any(|view| view.is_local) {
            views.push(KnownHostView {
                host: KnownHost {
                    instance_id: local_id.to_string(),
                    label: local.label.clone(),
                    first_seen_unix_ms: now_unix_ms,
                    last_seen_unix_ms: now_unix_ms,
                    repos_base_path: local.repos_base_path.clone().unwrap_or_default(),
                    max_attachment_bytes: local.max_attachment_bytes.unwrap_or_default(),
                },
                online: true,
                is_local: true,
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
    let instance_id = local_base_instance_id_for_config(config);
    HostSighting {
        label: format!("{instance_id} (this daemon)"),
        instance_id: DaemonInstanceId(instance_id),
        repos_base_path: Some(config.repos_base_path_or_default().to_string()),
        max_attachment_bytes: Some(config.max_attachment_bytes),
    }
}

/// Milliseconds since the Unix epoch, for stamping a sighting.
#[must_use]
pub fn now_unix_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_else(|e| {
            // A clock before 1970 stamps every row "20000 days ago", which reads as data loss
            // rather than as the misconfigured clock it is.
            log::warn!(
                "host registry: the system clock is before the Unix epoch ({e}); stamping 0"
            );
            0
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Ids that cannot collide with the machine the suite runs on: two tests resolve the real
    // hostname, and a developer working on a box called `server-2` must not see a red suite.
    const A_HOST: &str = "host-a.test.invalid";
    const ANOTHER_HOST: &str = "host-b.test.invalid";
    const THE_SERVING_HOST: &str = "serving-host.test.invalid";

    fn a_sighting_of(instance_id: &str) -> HostSighting {
        HostSighting {
            instance_id: DaemonInstanceId(instance_id.to_string()),
            label: format!("{instance_id} (this daemon)"),
            repos_base_path: Some("repos".to_string()),
            max_attachment_bytes: Some(1024),
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

    /// The serving daemon, for tests that are not about the local row.
    fn a_serving_daemon() -> HostSighting {
        HostSighting::named(
            DaemonInstanceId(THE_SERVING_HOST.to_string()),
            format!("{THE_SERVING_HOST} (this daemon)"),
        )
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
        // Given a host recorded by a store that is then dropped
        let dir = tempfile::tempdir().unwrap();
        let before_restart = FileHostRegistry::new(dir.path());
        before_restart
            .record_sighting(&a_sighting_of(A_HOST), 1_000)
            .expect("recording a sighting");

        // When a new store opens over the same directory
        let after_restart = FileHostRegistry::new(dir.path());
        let hosts = after_restart.known_hosts(&a_roster_of(&[]), &a_serving_daemon(), 2_000);

        // Then the host is still there, and offline — which is the whole point: gone is not
        // forgotten.
        assert!(
            !host_named(&hosts, A_HOST).online,
            "a host recorded before the restart is listed, offline, after it"
        );
    }

    /// `first_seen` answers "how long has tddy known this machine". Re-stamping it on every sighting
    /// would silently turn that into "when did we last see it", which `last_seen` already says.
    #[test]
    fn reopening_the_store_preserves_the_original_first_seen_timestamp() {
        // Given a host seen once, then seen again with a changed label
        let dir = tempfile::tempdir().unwrap();
        let registry = FileHostRegistry::new(dir.path());
        registry
            .record_sighting(&a_sighting_of(A_HOST), 1_000)
            .expect("first sighting");
        let mut relabelled = a_sighting_of(A_HOST);
        relabelled.label = "renamed".to_string();
        registry
            .record_sighting(&relabelled, 5_000)
            .expect("later sighting");

        // When the store is reopened
        let hosts = FileHostRegistry::new(dir.path()).known_hosts(
            &a_roster_of(&[]),
            &a_serving_daemon(),
            9_000,
        );

        // Then
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
        // Given a recorded host
        let dir = tempfile::tempdir().unwrap();
        let registry = FileHostRegistry::new(dir.path());
        registry
            .record_sighting(&a_sighting_of(A_HOST), 1_000)
            .expect("sighting");

        // When it stops being visible
        registry
            .record_departure(&DaemonInstanceId(A_HOST.to_string()), 4_000)
            .expect("departure");

        // Then
        let hosts = registry.known_hosts(&a_roster_of(&[]), &a_serving_daemon(), 9_000);
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
        // Given a registry file that is not JSON
        let dir = tempfile::tempdir().unwrap();
        let registry = FileHostRegistry::new(dir.path());
        std::fs::write(dir.path().join(REGISTRY_FILE), b"{ this is not json").unwrap();

        // When the hosts are listed against a live roster
        let hosts = registry.known_hosts(&a_roster_of(&[A_HOST]), &a_serving_daemon(), 1_000);

        // Then nothing was remembered, and only the live host and the serving daemon are reported
        assert_eq!(
            hosts.len(),
            2,
            "a corrupt file yields no remembered hosts, leaving the live roster and the local row"
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
        // Given a file where the storage directory should be: creating the directory cannot succeed
        let dir = tempfile::tempdir().unwrap();
        let blocked = dir.path().join("blocked");
        std::fs::write(&blocked, b"not a directory").unwrap();

        // When a sighting is recorded
        let registry = FileHostRegistry::new(&blocked);
        let result = registry.record_sighting(&a_sighting_of(A_HOST), 1_000);

        // Then
        assert!(
            result.is_err(),
            "an unwritable registry must report the failure"
        );
    }

    /// The roster is a sighting in its own right. A reachable host missing from the file would
    /// otherwise be hidden by the very screen meant to list every host.
    #[test]
    fn a_live_host_with_no_record_yet_is_still_reported_as_online() {
        // Given an empty registry
        let dir = tempfile::tempdir().unwrap();
        let registry = FileHostRegistry::new(dir.path());

        // When a host answers the roster without ever having been recorded
        let hosts = registry.known_hosts(&a_roster_of(&[ANOTHER_HOST]), &a_serving_daemon(), 1_000);

        // Then
        assert!(
            host_named(&hosts, ANOTHER_HOST).online,
            "a host in the roster is online even with no prior record"
        );
    }

    /// `is_local` marks the daemon serving the call, so the screen can say "this one is me".
    #[test]
    fn the_serving_daemon_is_marked_as_the_local_host() {
        // Given a recorded host that happens to be the serving daemon
        let dir = tempfile::tempdir().unwrap();
        let registry = FileHostRegistry::new(dir.path());
        registry
            .record_sighting(&a_sighting_of(THE_SERVING_HOST), 1_000)
            .expect("sighting");

        // When the hosts are listed
        let hosts = registry.known_hosts(&a_roster_of(&[]), &a_serving_daemon(), 2_000);

        // Then it is flagged local, and online even though no roster listed it
        let host = host_named(&hosts, THE_SERVING_HOST);
        assert!(host.is_local, "the serving daemon is local");
        assert!(host.online, "the daemon answering the call is online");
    }

    /// The serving daemon is the one host that can never legitimately be missing — it is answering
    /// the call. Nothing recorded and nothing in the roster still has to produce its row.
    #[test]
    fn the_serving_daemon_is_listed_even_when_nothing_has_been_recorded() {
        // Given an empty registry and an empty roster
        let dir = tempfile::tempdir().unwrap();
        let registry = FileHostRegistry::new(dir.path());

        // When the hosts are listed
        let local = HostSighting {
            instance_id: DaemonInstanceId(THE_SERVING_HOST.to_string()),
            label: "this daemon".to_string(),
            repos_base_path: Some("repos".to_string()),
            max_attachment_bytes: Some(4096),
        };
        let hosts = registry.known_hosts(&a_roster_of(&[]), &local, 7_000);

        // Then its row is there, filled from what the daemon knows about itself first-hand
        let host = host_named(&hosts, THE_SERVING_HOST);
        assert!(
            host.is_local && host.online,
            "the serving daemon is present"
        );
        assert_eq!(host.host.repos_base_path, "repos");
        assert_eq!(host.host.max_attachment_bytes, 4096);
    }

    /// The discovery tick repeats the same snapshot several times a second. Rewriting the file each
    /// time would cost two fsyncs per host per tick, forever, for no new information.
    #[test]
    fn a_snapshot_that_changes_nothing_does_not_rewrite_the_file() {
        // Given a recorded host, and the file taken away afterwards so any write is visible
        let dir = tempfile::tempdir().unwrap();
        let registry = FileHostRegistry::new(dir.path());
        registry
            .record_snapshot(&[a_sighting_of(A_HOST)], &[], 1_000)
            .expect("first snapshot");
        std::fs::remove_file(dir.path().join(REGISTRY_FILE)).expect("removing the registry file");

        // When the very same snapshot arrives again, later
        registry
            .record_snapshot(&[a_sighting_of(A_HOST)], &[], 2_000)
            .expect("an unchanged snapshot");

        // Then nothing was written
        assert!(
            !dir.path().join(REGISTRY_FILE).exists(),
            "an unchanged snapshot must not touch the disk"
        );
    }

    /// The corollary: a snapshot that *does* change something writes immediately, so an arrival or
    /// a departure is never sitting unrecorded behind a skipped write.
    #[test]
    fn a_snapshot_with_an_arrival_and_a_departure_is_written_once() {
        // Given a host recorded, and the file taken away so the next write is visible
        let dir = tempfile::tempdir().unwrap();
        let registry = FileHostRegistry::new(dir.path());
        registry
            .record_snapshot(&[a_sighting_of(A_HOST)], &[], 1_000)
            .expect("first snapshot");
        std::fs::remove_file(dir.path().join(REGISTRY_FILE)).expect("removing the registry file");

        // When the next snapshot has A gone and B arrived
        registry
            .record_snapshot(
                &[a_sighting_of(ANOTHER_HOST)],
                &[DaemonInstanceId(A_HOST.to_string())],
                6_000,
            )
            .expect("a changed snapshot");

        // Then both transitions are on disk, from that one write
        let reopened = FileHostRegistry::new(dir.path());
        let hosts = reopened.known_hosts(&a_roster_of(&[]), &a_serving_daemon(), 9_000);
        assert_eq!(
            host_named(&hosts, A_HOST).host.last_seen_unix_ms,
            6_000,
            "the departure was stamped"
        );
        assert_eq!(
            host_named(&hosts, ANOTHER_HOST).host.first_seen_unix_ms,
            6_000,
            "the arrival was recorded"
        );
    }

    /// The other half of the skip: a stamp is allowed to lag, but not to age without limit. A host
    /// still visible after the refresh floor gets its `last_seen` written, so a row can never say
    /// "last seen a month ago" about a machine that was reachable until yesterday.
    #[test]
    fn a_host_still_present_after_the_refresh_floor_has_its_last_seen_written() {
        // Given a host recorded, and the file taken away so any write is visible
        let dir = tempfile::tempdir().unwrap();
        let registry = FileHostRegistry::new(dir.path());
        registry
            .record_snapshot(&[a_sighting_of(A_HOST)], &[], 1_000)
            .expect("first snapshot");
        std::fs::remove_file(dir.path().join(REGISTRY_FILE)).expect("removing the registry file");

        // When the same unchanged snapshot arrives a moment later, and again past the floor
        let within_the_floor = 1_000 + LAST_SEEN_REFRESH_FLOOR_MS - 1;
        registry
            .record_snapshot(&[a_sighting_of(A_HOST)], &[], within_the_floor)
            .expect("a snapshot inside the refresh floor");
        assert!(
            !dir.path().join(REGISTRY_FILE).exists(),
            "a tick inside the refresh floor must not touch the disk"
        );

        let past_the_floor = 1_000 + LAST_SEEN_REFRESH_FLOOR_MS;
        registry
            .record_snapshot(&[a_sighting_of(A_HOST)], &[], past_the_floor)
            .expect("a snapshot past the refresh floor");

        // Then the stamp on disk has caught up
        let reopened = FileHostRegistry::new(dir.path());
        let hosts = reopened.known_hosts(&a_roster_of(&[]), &a_serving_daemon(), past_the_floor);
        assert_eq!(
            host_named(&hosts, A_HOST).host.last_seen_unix_ms,
            past_the_floor,
            "a host still present past the refresh floor is stamped"
        );
    }

    /// The registry never deletes and its ids are self-declared, so the ceiling is what stops a peer
    /// reconnecting under fresh random ids from growing the file without bound. Refusing a *new*
    /// host is the whole of that bound; refusing an update to a host already known would be a bug,
    /// because it would freeze every remaining machine's record the moment the file filled up.
    #[test]
    fn a_full_registry_refuses_a_new_host_and_still_updates_a_known_one() {
        // Given a registry filled exactly to its ceiling, in one batch
        let dir = tempfile::tempdir().unwrap();
        let registry = FileHostRegistry::new(dir.path());
        let ids: Vec<String> = (0..MAX_KNOWN_HOSTS)
            .map(|n| format!("host-{n}.test.invalid"))
            .collect();
        let full_room: Vec<HostSighting> = ids.iter().map(|id| a_sighting_of(id)).collect();
        registry
            .record_snapshot(&full_room, &[], 1_000)
            .expect("filling the registry");

        // When one more machine appears, alongside a known one whose label has changed
        let mut renamed = a_sighting_of(&ids[0]);
        renamed.label = "renamed while full".to_string();
        registry
            .record_snapshot(
                &[a_sighting_of("one-too-many.test.invalid"), renamed],
                &[],
                2_000,
            )
            .expect("recording past the ceiling");

        // Then the newcomer is refused and the known host is still maintained
        let hosts = registry.known_hosts(&a_roster_of(&[]), &a_serving_daemon(), 3_000);
        assert_eq!(
            hosts.len(),
            MAX_KNOWN_HOSTS + 1,
            "the ceiling holds, and only the serving daemon is added to it"
        );
        assert!(
            !hosts
                .iter()
                .any(|h| h.host.instance_id == "one-too-many.test.invalid"),
            "a new host past the ceiling is refused"
        );
        assert_eq!(
            host_named(&hosts, &ids[0]).host.label,
            "renamed while full",
            "a host already known is still updated when the registry is full"
        );
    }

    /// A host id is a key, and half a key names a different machine. Peer metadata is self-declared,
    /// so an id that cannot be a host id is refused rather than cut down to fit.
    #[test]
    fn a_host_id_over_the_length_limit_is_refused_rather_than_truncated() {
        // Given a peer declaring an absurd id
        let dir = tempfile::tempdir().unwrap();
        let registry = FileHostRegistry::new(dir.path());
        let huge = "h".repeat(MAX_INSTANCE_ID_BYTES + 1);

        // When it is recorded alongside a well-formed peer
        registry
            .record_snapshot(&[a_sighting_of(&huge), a_sighting_of(A_HOST)], &[], 1_000)
            .expect("recording the snapshot");

        // Then the well-formed peer is stored and the absurd one is not
        let hosts = registry.known_hosts(&a_roster_of(&[]), &a_serving_daemon(), 2_000);
        assert!(
            !hosts.iter().any(|h| h.host.instance_id.starts_with("hhh")),
            "an over-long host id is never persisted"
        );
        assert_eq!(
            host_named(&hosts, A_HOST).host.instance_id,
            A_HOST,
            "one bad peer does not cost the rest of the snapshot its recording"
        );
    }

    /// An id carrying anything but the characters a host id is spelled with is self-declared junk.
    #[test]
    fn a_host_id_with_unexpected_characters_is_refused() {
        // Given a peer declaring an id with markup in it
        let dir = tempfile::tempdir().unwrap();
        let registry = FileHostRegistry::new(dir.path());

        // When it is recorded
        registry
            .record_snapshot(&[a_sighting_of("<script>alert(1)</script>")], &[], 1_000)
            .expect("recording the snapshot");

        // Then nothing was stored
        let hosts = registry.known_hosts(&a_roster_of(&[]), &a_serving_daemon(), 2_000);
        assert!(
            !hosts.iter().any(|h| h.host.instance_id.contains('<')),
            "a malformed host id is never persisted"
        );
    }

    /// A label is display text, so an over-long one is cut to fit rather than refused — the host
    /// itself is still worth remembering.
    #[test]
    fn an_over_long_label_is_truncated_and_the_host_is_still_recorded() {
        // Given a peer with an enormous label
        let dir = tempfile::tempdir().unwrap();
        let registry = FileHostRegistry::new(dir.path());
        let mut sighting = a_sighting_of(A_HOST);
        sighting.label = "L".repeat(MAX_LABEL_BYTES * 4);

        // When it is recorded
        registry
            .record_sighting(&sighting, 1_000)
            .expect("recording the sighting");

        // Then the host is there with a bounded label
        let hosts = registry.known_hosts(&a_roster_of(&[]), &a_serving_daemon(), 2_000);
        assert_eq!(
            host_named(&hosts, A_HOST).host.label.len(),
            MAX_LABEL_BYTES,
            "a label is stored bounded"
        );
    }

    /// A sighting that knows only an id and a label must not blank the columns a richer sighting
    /// already recorded — that is what the optional columns are for.
    #[test]
    fn a_roster_only_sighting_keeps_the_repos_base_path_a_richer_one_recorded() {
        // Given a host recorded from a full sighting
        let dir = tempfile::tempdir().unwrap();
        let registry = FileHostRegistry::new(dir.path());
        registry
            .record_sighting(&a_sighting_of(A_HOST), 1_000)
            .expect("full sighting");

        // When only a roster row is seen next
        registry
            .record_sighting(
                &HostSighting::named(DaemonInstanceId(A_HOST.to_string()), "renamed"),
                2_000,
            )
            .expect("roster sighting");

        // Then the repos base path survives
        let hosts = registry.known_hosts(&a_roster_of(&[]), &a_serving_daemon(), 3_000);
        assert_eq!(host_named(&hosts, A_HOST).host.repos_base_path, "repos");
    }
}
