use crate::project_notification_log::ProjectNotificationLog;
use crate::project_notification_log::ProjectNotificationRecord;
use crate::session_watches::inbox_position_of;
use crate::session_watches::matcher::watch_matches;
use crate::session_watches::PendingEnd;
use crate::session_watches::ProjectWatches;
use crate::session_watches::SessionWatch;
use crate::session_watches::WatchOwner;
use crate::session_watches::MAX_EVENTS_PER_ANSWER;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use tddy_daemon_kernel::session_notification::SessionNotificationSource;

/// Why a watch ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchEndReason {
    /// Its time to live passed.
    Expired,
    /// Its orchestrator's agent ended.
    OrchestratorEnded,
    /// Every session it names ended.
    SessionsEnded,
}

/// A watch that ended, reported once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchEnd {
    pub watch_id: String,
    pub reason: WatchEndReason,
}

/// What one delivery hands an orchestrator.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WatchDelivery {
    /// The records its watches matched, oldest first, each once; at most
    /// [`MAX_EVENTS_PER_ANSWER`].
    pub events: Vec<ProjectNotificationRecord>,
    /// Matched records the log evicted before they could be delivered.
    pub lost: u64,
    /// The watches that ended since the last delivery.
    pub ended: Vec<WatchEnd>,
}

impl WatchDelivery {
    /// Whether there is nothing to hand over.
    pub fn is_empty(&self) -> bool {
        self.events.is_empty() && self.lost == 0 && self.ended.is_empty()
    }
}

/// Why `watch` has ended at `now_unix_ms`, or `None` while it lives.
pub(super) fn end_reason(
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
pub(super) fn apply_delivery(
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
pub(super) fn end_finished(
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
pub(super) fn end_expired(
    state: &mut ProjectWatches,
    owner: &WatchOwner,
    now_unix_ms: u64,
) -> bool {
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
