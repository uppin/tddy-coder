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

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tddy_daemon_kernel::session_notification::SessionNotificationSource;

use crate::project_notification_log::{ProjectNotificationLog, ProjectNotificationRecord};

mod delivery;
pub(crate) mod matcher;
mod store;
mod stream;

pub use stream::{StreamBatch, StreamRefusal, StreamedEvent, STREAM_LEASE_GRACE};

/// The longest a watch lives without being renewed: 24 hours.
pub const MAX_WATCH_TTL_SECS: u64 = 86_400;
/// How many live watches one orchestrator may hold.
pub const MAX_LIVE_WATCHES: usize = 16;
/// How many matched events one delivery carries; the rest wait for the next.
pub const MAX_EVENTS_PER_ANSWER: usize = 50;

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

/// Which records a watch matches: those of `session_ids`, or — with `started_by_me` — of the
/// sessions its owner started; and of `kinds` only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchFilter {
    pub session_ids: Vec<String>,
    pub started_by_me: bool,
    pub kinds: Vec<WatchKind>,
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
}

/// One live watch as `ListSessionWatches` shows it: the watch, and how many matched events wait.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedWatch {
    pub watch: SessionWatch,
    pub waiting: usize,
}

pub use crate::session_watches::delivery::{WatchDelivery, WatchEnd, WatchEndReason};

/// A watch that ended and has not been reported yet, and whose it was.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct PendingEnd {
    owner: WatchOwner,
    end: WatchEnd,
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

/// Whether `watch` matches `record`, `owner_inbox_position` being the `seq` up to which the
/// watch's owner has read the messages addressed to it. Its session is named, or it started the
/// session, and the record is of a kind it asks for, and it is never the owner's own session. The
/// record's project and OS user are the log read's, which is the owner's.
///
/// A **message** record matches by who it is addressed to and who sent it: an inbox watch
/// ([`WatchFilter::is_inbox_of`] its owner) matches the messages addressed to its owner, whoever
/// sent them; any other watch matches by sender, through the session selection. And a message
/// addressed to the owner that the owner's inbox position has passed matches no watch — it was
/// read already, and it is delivered once (`crate::session_messages`).
fn watch_matches(
    watch: &SessionWatch,
    record: &ProjectNotificationRecord,
    owner_inbox_position: u64,
) -> bool {
    if record.session_id == watch.owner.session_id {
        return false;
    }
    if record.source == SessionNotificationSource::Message {
        let Some(message) = &record.message else {
            return false;
        };
        let addressed_to_the_owner = message.to_session_id == watch.owner.session_id;
        if addressed_to_the_owner && record.seq <= owner_inbox_position {
            return false;
        }
        if watch.filter.is_inbox_of(&watch.owner.session_id) {
            return addressed_to_the_owner;
        }
        if !watch.filter.kinds.contains(&WatchKind::Message) {
            return false;
        }
        return watch
            .filter
            .session_ids
            .iter()
            .any(|id| id == &record.session_id)
            || (watch.filter.started_by_me
                && record.orchestrator_session_id.as_deref()
                    == Some(watch.owner.session_id.as_str()));
    }
    let selected = watch
        .filter
        .session_ids
        .iter()
        .any(|id| id == &record.session_id)
        || (watch.filter.started_by_me
            && record.orchestrator_session_id.as_deref() == Some(watch.owner.session_id.as_str()));
    selected && WatchKind::of(record).is_some_and(|kind| watch.filter.kinds.contains(&kind))
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
/// its position at or past the record. What a read skips, so a message is delivered once across
/// the reader's watches and its reads (`crate::session_messages`).
pub(crate) fn a_watch_has_delivered(
    state: &ProjectWatches,
    owner: &WatchOwner,
    record: &ProjectNotificationRecord,
    owner_inbox_position: u64,
) -> bool {
    state.watches.iter().any(|watch| {
        &watch.owner == owner
            && watch.position >= record.seq
            && watch_matches(watch, record, owner_inbox_position)
    })
}

/// Why `watch` has ended at `now_unix_ms`, or `None` while it lives.
fn end_reason(
    watch: &SessionWatch,
    log: &ProjectNotificationLog,
    now_unix_ms: u64,
) -> Option<WatchEndReason> {
    if now_unix_ms >= watch.expires_at_unix_ms {
        return Some(WatchEndReason::Expired);
    }
    let (records, _) = log.records_after(
        &watch.owner.project_id,
        &watch.owner.os_user,
        watch.created_seq,
    );
    let ended: HashSet<&str> = records
        .iter()
        .filter(|r| r.source == SessionNotificationSource::SessionEnded)
        .map(|r| r.session_id.as_str())
        .collect();
    if ended.contains(watch.owner.session_id.as_str()) {
        return Some(WatchEndReason::OrchestratorEnded);
    }
    if !watch.filter.started_by_me
        && !watch.filter.session_ids.is_empty()
        && watch
            .filter
            .session_ids
            .iter()
            .all(|id| ended.contains(id.as_str()))
    {
        return Some(WatchEndReason::SessionsEnded);
    }
    None
}

/// Compute `owner`'s delivery from `state` and advance it past what the delivery hands over:
/// positions move past the delivered records (and past what the log evicted), and the watches
/// that ended are taken to be reported. Shared by `take_delivery`, which commits at once, and
/// `deliver_with`, which rolls `state` back when its delivery is refused.
fn apply_delivery(
    state: &mut ProjectWatches,
    owner: &WatchOwner,
    log: &ProjectNotificationLog,
    now_unix_ms: u64,
    streamed: &HashSet<String>,
) -> WatchDelivery {
    end_finished(state, owner, log, now_unix_ms);
    // A watch a stream's lease holds is skipped entirely: its events are the stream's alone,
    // and its position is the stream's to advance.
    let inbox = inbox_position_of(state, owner);
    let mut matched: BTreeMap<u64, ProjectNotificationRecord> = BTreeMap::new();
    let mut lost = 0_u64;
    let mut scans: Vec<(String, u64, u64)> = Vec::new();
    for watch in state
        .watches
        .iter()
        .filter(|watch| &watch.owner == owner && !streamed.contains(&watch.watch_id))
    {
        let (records, watch_lost) =
            log.records_after(&owner.project_id, &owner.os_user, watch.position);
        lost = lost.max(watch_lost);
        for record in records.iter().filter(|r| watch_matches(watch, r, inbox)) {
            matched.entry(record.seq).or_insert_with(|| record.clone());
        }
        scans.push((watch.watch_id.clone(), watch.position, watch_lost));
    }
    let events: Vec<ProjectNotificationRecord> = matched
        .values()
        .take(MAX_EVENTS_PER_ANSWER)
        .cloned()
        .collect();
    let delivered: HashSet<u64> = events.iter().map(|r| r.seq).collect();
    for watch in state
        .watches
        .iter_mut()
        .filter(|watch| &watch.owner == owner && !streamed.contains(&watch.watch_id))
    {
        let mut position = watch.position;
        if let Some((_, _, watch_lost)) = scans.iter().find(|(id, _, _)| id == &watch.watch_id) {
            position = position.max(watch.position.saturating_add(*watch_lost));
        }
        for record in events
            .iter()
            .filter(|r| delivered.contains(&r.seq) && watch_matches(watch, r, inbox))
        {
            position = position.max(record.seq);
        }
        watch.position = position;
    }
    let mut ended = Vec::new();
    state.ended.retain(|pending| {
        if &pending.owner == owner && !streamed.contains(&pending.end.watch_id) {
            ended.push(pending.end.clone());
            false
        } else {
            true
        }
    });
    WatchDelivery {
        events,
        lost,
        ended,
    }
}

/// Move `owner`'s watches that have ended out of the live set, to be reported once by its next
/// delivery. Whether any moved.
fn end_finished(
    state: &mut ProjectWatches,
    owner: &WatchOwner,
    log: &ProjectNotificationLog,
    now_unix_ms: u64,
) -> bool {
    let mut kept = Vec::with_capacity(state.watches.len());
    let mut moved = false;
    for watch in state.watches.drain(..) {
        if &watch.owner == owner {
            if let Some(reason) = end_reason(&watch, log, now_unix_ms) {
                state.ended.push(PendingEnd {
                    owner: watch.owner.clone(),
                    end: WatchEnd {
                        watch_id: watch.watch_id.clone(),
                        reason,
                    },
                });
                moved = true;
                continue;
            }
        }
        kept.push(watch);
    }
    state.watches = kept;
    moved
}

/// [`end_finished`] for expiry alone, for the operations handed no log.
fn end_expired(state: &mut ProjectWatches, owner: &WatchOwner, now_unix_ms: u64) -> bool {
    let mut kept = Vec::with_capacity(state.watches.len());
    let mut moved = false;
    for watch in state.watches.drain(..) {
        if &watch.owner == owner && now_unix_ms >= watch.expires_at_unix_ms {
            state.ended.push(PendingEnd {
                owner: watch.owner.clone(),
                end: WatchEnd {
                    watch_id: watch.watch_id.clone(),
                    reason: WatchEndReason::Expired,
                },
            });
            moved = true;
            continue;
        }
        kept.push(watch);
    }
    state.watches = kept;
    moved
}
