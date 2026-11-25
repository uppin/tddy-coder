//! An orchestrator's watches over the sessions it can see (`#activity-wake` 2/3).
//!
//! A watch is a filter — which sessions, which kinds of event — with its own position in the
//! project notification log and a time to live. It copies nothing: the log already keeps, scopes
//! and evicts every record a watch could want, so a watch only remembers how far it has delivered.
//! [`SessionWatchStore::take_delivery`] hands an orchestrator what its watches matched since it
//! last asked, each record once, and ends the watches that have ended.
//!
//! Persisted per project as `<data dir>/projects/<project_id>/session-watches.json`, so a watch
//! and its position survive a daemon restart, and — keyed by the orchestrator's session id, which
//! `/clear` keeps — its conversation rotating.
//!
//! Feature: docs/ft/daemon/project-concept.md#watching-sessions

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::project_notification_log::ProjectNotificationRecord;

mod delivery;
pub(crate) mod matcher;
mod relay_gaps;
mod store;
mod stream;
pub mod stream_call;

pub use crate::session_watches::stream_call::StreamPick;
pub use crate::session_watches::stream_call::StreamRefusal;
pub use stream::{StreamBatch, StreamedEvent, STREAM_LEASE_GRACE};

/// The longest a watch lives without being renewed: 24 hours.
pub const MAX_WATCH_TTL_SECS: u64 = 86_400;
/// How many live watches one orchestrator may hold.
pub const MAX_LIVE_WATCHES: usize = 16;
/// How many matched events one delivery carries; the rest wait for the next.
pub const MAX_EVENTS_PER_ANSWER: usize = 50;

// A `WaitSessionWatchEvents` call's `max_events` is bounded by the tool before it reaches the store.
const _: () = assert!(
    MAX_EVENTS_PER_ANSWER == tddy_tool_engine::project_home_watch_stream_tools::MAX_EVENTS_PER_WAIT
);

/// What a store keeps per project, under the project's directory.
pub const SESSION_WATCHES_FILE: &str = "session-watches.json";

pub use crate::session_watches::matcher::WatchKind;

/// Whose watch it is: an orchestrator session of a project, and the OS user it runs as.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WatchOwner {
    pub project_id: String,
    pub session_id: String,
    pub os_user: String,
}

pub use crate::session_watches::matcher::WatchFilter;

/// How a watch delivers, besides what it matches.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WatchOptions {
    /// Whether an idle orchestrator is sent a `[tddy watch]` turn carrying what this watch matched.
    /// Off by default: the events wait for the orchestrator to collect them — through
    /// `WaitSessionWatchEvents`, a monitor, or the next tool answer.
    pub wake_when_idle: bool,
}

/// One watch, as the store keeps it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionWatch {
    pub watch_id: String,
    pub owner: WatchOwner,
    pub filter: WatchFilter,
    pub created_at_unix_ms: u64,
    pub expires_at_unix_ms: u64,
    /// The `seq` of the last log record this watch has delivered past.
    pub position: u64,
    /// The `seq` the log had when the watch was created: its endings are looked for after it,
    /// however far delivery has since advanced `position`.
    #[serde(default)]
    pub created_seq: u64,
    /// Whether this watch's events are delivered to its idle orchestrator as a turn
    /// ([`WatchOptions::wake_when_idle`]). A watch kept before the option existed does not wake.
    #[serde(default)]
    pub wake_when_idle: bool,
    /// The `seq`s past [`Self::position`] already handed and acknowledged: what a stream call that
    /// picked a subset of the matched records acknowledged, while an earlier record it did not
    /// pick holds the position back. Never handed again; pruned as the position passes them.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub acked_above: BTreeSet<u64>,
}

/// One live watch as `ListSessionWatches` shows it: the watch, and how many matched events wait.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedWatch {
    pub watch: SessionWatch,
    pub waiting: usize,
}

pub use crate::session_watches::delivery::{WatchDelivery, WatchEnd, WatchEndReason};
use crate::session_watches::matcher::watch_matches;

/// A watch that ended and has not been reported yet, whose it was, and whether it woke its owner
/// — an ending the wake reports only for a watch that did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct PendingEnd {
    owner: WatchOwner,
    end: WatchEnd,
    #[serde(default)]
    wake_when_idle: bool,
}

/// The watches of one project, as kept in memory and on disk. Its fields are the watch module's
/// alone; the rest of the crate reaches a project's state through
/// [`SessionWatchStore::project_of`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct ProjectWatches {
    #[serde(default)]
    watches: Vec<SessionWatch>,
    #[serde(default)]
    ended: Vec<PendingEnd>,
    /// Per session, the `seq` up to which it has read the messages addressed to it — the inbox
    /// position of `ReadSessionMessages`, kept beside the watches
    /// (`#activity-wake` 5/5). A session named here but holding no watches still keeps its
    /// position.
    #[serde(default)]
    inbox_positions: BTreeMap<String, u64>,
}

/// Every project's watches on this host.
pub struct SessionWatchStore {
    data_dir: PathBuf,
    /// One lock per project: every operation on a project's watches holds it, which is what makes a
    /// record delivered once across concurrent calls.
    projects: Mutex<HashMap<String, Arc<tokio::sync::Mutex<ProjectWatches>>>>,
    /// Which stream holds which watch, in memory only (`#activity-wake` 4/5): a daemon restart
    /// ends every stream's hold.
    leases: Mutex<stream::StreamLeases>,
    /// How long a watch stays streamed after its stream's last call.
    stream_lease_grace: Duration,
    /// One guard per owner, held by every call that hands that owner's watch events over — a
    /// wake's delivery for its whole paste, a collection, a stream call — so a record a wake is
    /// still pasting is not handed to its owner a second time. The project lock is held only to
    /// read and to commit, never across a paste.
    deliveries: Mutex<HashMap<WatchOwner, Arc<tokio::sync::Mutex<()>>>>,
}

impl SessionWatchStore {
    /// Rewrite `project_id`'s watches file whole, or the reason it could not be written.
    fn persist(&self, project_id: &str, state: &ProjectWatches) -> anyhow::Result<()> {
        let path = watches_file(&self.data_dir, project_id)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| anyhow::anyhow!("create {}: {e}", parent.display()))?;
        }
        let body = serde_json::to_string(state)
            .map_err(|e| anyhow::anyhow!("serialize the session watches: {e}"))?;
        let temporary = path.with_file_name(format!("{SESSION_WATCHES_FILE}.tmp"));
        let mut file = std::fs::File::create(&temporary)
            .map_err(|e| anyhow::anyhow!("create {}: {e}", temporary.display()))?;
        use std::io::Write as _;
        file.write_all(body.as_bytes())
            .and_then(|()| file.sync_all())
            .map_err(|e| anyhow::anyhow!("write {}: {e}", temporary.display()))?;
        std::fs::rename(&temporary, &path)
            .map_err(|e| anyhow::anyhow!("replace {}: {e}", path.display()))?;
        if let Some(parent) = path.parent() {
            std::fs::File::open(parent)
                .and_then(|dir| dir.sync_all())
                .map_err(|e| anyhow::anyhow!("sync {}: {e}", parent.display()))?;
        }
        Ok(())
    }

    /// [`Self::persist`], logging a failure: the in-memory watches still stand, and the next
    /// change rewrites the whole file.
    fn persist_best_effort(&self, project_id: &str, state: &ProjectWatches) {
        if let Err(e) = self.persist(project_id, state) {
            log::warn!(
                target: "tddy_session_activity::session_watches",
                "could not persist the session watches of {project_id}: {e:#}"
            );
        }
    }
}

/// Where `project_id`'s watches are kept under `data_dir`. An id that cannot name a directory is
/// refused rather than joined onto the path.
fn watches_file(data_dir: &Path, project_id: &str) -> anyhow::Result<PathBuf> {
    let names_a_directory = !project_id.is_empty()
        && project_id != "."
        && project_id != ".."
        && !project_id.contains(['/', '\\', '\0']);
    anyhow::ensure!(
        names_a_directory,
        "project id {project_id:?} cannot name a directory"
    );
    Ok(data_dir
        .join("projects")
        .join(project_id)
        .join(SESSION_WATCHES_FILE))
}

/// The refusal of a `ttl` of zero or above [`MAX_WATCH_TTL_SECS`], saying what to do instead.
fn refuse_a_bad_ttl(ttl: Duration) -> Result<(), String> {
    if ttl.is_zero() || ttl > Duration::from_secs(MAX_WATCH_TTL_SECS) {
        return Err(format!(
            "ttl_seconds must be between 1 and {MAX_WATCH_TTL_SECS} (24 hours): give CreateSessionWatch or RenewSessionWatch a ttl_seconds in that range"
        ));
    }
    Ok(())
}

/// The `seq` up to which `owner` has read the messages addressed to it, from `state`.
pub(crate) fn inbox_position_of(state: &ProjectWatches, owner: &WatchOwner) -> u64 {
    state
        .inbox_positions
        .get(&owner.session_id)
        .copied()
        .unwrap_or(0)
}

/// Whether a live watch of `owner` in `state` matches `record` and has already been handed it —
/// its position at or past the record, or the record acknowledged above it. What a read skips, so a message is delivered once across
/// the reader's watches and its reads (`crate::session_messages`).
pub(crate) fn a_watch_has_delivered(
    state: &ProjectWatches,
    owner: &WatchOwner,
    record: &ProjectNotificationRecord,
    owner_inbox_position: u64,
) -> bool {
    state.watches.iter().any(|watch| {
        &watch.owner == owner
            && (watch.position >= record.seq || watch.acked_above.contains(&record.seq))
            && watch_matches(watch, record, owner_inbox_position)
    })
}
