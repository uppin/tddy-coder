//! Streaming a watch to a process: the acknowledged position and the stream lease
//! (`#activity-wake` 4/5).
//!
//! A stream reads the same watches and positions as [`SessionWatchStore::take_delivery`] and
//! [`SessionWatchStore::deliver_wake_with`], so it must hold a watch exclusively while it runs. It does
//! so with a **lease**: held while a stream keeps calling, renewed by each call, kept in memory
//! only, and lapsing [`STREAM_LEASE_GRACE`] after the last call. While a watch is streamed the two
//! other deliveries skip it.
//!
//! A new stream of the same owner **takes a watch over** at once: the agent resumes its monitor
//! (`--watch`) without waiting out the old lease, from the last acknowledged position, and the old
//! stream is refused from its next call. A watch nobody resumes ends at its TTL.
//!
//! A stream's position advances only on **acknowledgement**: the next call's `ack_seq` says what
//! the process printed, so a batch it received but never printed is handed again.
//!
//! A call may **pick** a subset of what its watches matched ([`StreamPick`]): at most
//! `max_events`, of some kinds, of some sessions. Acknowledgement is exact: `ack_seq` acknowledges
//! the records the stream's latest answer handed, up to that seq, and nothing else. A watch's
//! position advances to just before the first record it still owes, and the acknowledged records
//! past it are kept in [`SessionWatch::acked_above`] — so what a pick left out stays pending, and
//! what was acknowledged is never handed again.
//!
//! Feature: docs/ft/daemon/1-WIP/PRD-2026-11-17-session-watch-monitor.md

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::time::Duration;

use super::{SessionWatchStore, WatchOwner, MAX_EVENTS_PER_ANSWER};
use crate::project_notification_log::{ProjectNotificationLog, ProjectNotificationRecord};
use crate::session_watches::delivery::end_finished;
use crate::session_watches::delivery::end_reason;
use crate::session_watches::delivery::WatchEnd;
use crate::session_watches::matcher::watch_owes;
use crate::session_watches::stream_call::StreamPick;
use crate::session_watches::stream_call::StreamRefusal;
use crate::session_watches::{SessionWatch, WatchKind};

/// How long a watch stays streamed after a stream's last call returned: the monitor's turnaround
/// between two calls, with margin. A stream that stops calling hands its watches back to tool
/// answers and the wake once it passes.
pub const STREAM_LEASE_GRACE: Duration = Duration::from_secs(15);

/// One record a stream is handed, with the streamed watches that matched it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamedEvent {
    pub record: ProjectNotificationRecord,
    /// The ids of the streamed watches that matched the record, in the order the call named them.
    pub watch_ids: Vec<String>,
}

/// What one stream call hands its stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamBatch {
    /// The stream now holding the lease on the named watches: the id the stream's first call was
    /// given, which each later call presents.
    pub stream_id: String,
    /// The records the named watches matched after the acknowledged position, oldest first, each
    /// once; at most [`super::MAX_EVENTS_PER_ANSWER`]. Not yet acknowledged: the next call that does
    /// not acknowledge them is handed them again.
    pub events: Vec<StreamedEvent>,
    /// Matched records the log evicted before they could be handed.
    pub lost: u64,
    /// The named watches that ended, reported once.
    pub ended: Vec<WatchEnd>,
    /// The `ack_seq` the next call presents once these events are printed: the newest handed
    /// `seq`, or the acknowledged position when nothing was handed.
    pub next_ack_seq: u64,
}

impl SessionWatchStore {
    /// This store, its stream leases lapsing `grace` after a stream's last call rather than
    /// [`STREAM_LEASE_GRACE`].
    pub fn with_stream_lease_grace(mut self, grace: Duration) -> Self {
        self.stream_lease_grace = grace;
        self
    }

    /// One call of a stream over `owner`'s watches `watch_ids`, under the project's lock.
    ///
    /// - It first advances each named watch's position to `ack_seq` — refused when `ack_seq` is past
    ///   what this stream was handed;
    /// - then takes or renews the lease: `stream_id` `None` starts a new stream, which **takes the
    ///   named watches over** from any stream of the same owner still holding them — the monitor is
    ///   resumed, and the superseded stream's next call is refused as
    ///   [`StreamRefusal::Superseded`]; `Some` continues the stream it names;
    /// - and answers the records after the new positions, the `lost` count and the ended watches,
    ///   **without** advancing past them.
    ///
    /// Refused for no watch named, or a watch that is not `owner`'s live watch.
    pub async fn take_stream_batch(
        &self,
        owner: &WatchOwner,
        stream_id: Option<&str>,
        watch_ids: &[String],
        ack_seq: Option<u64>,
        log: &ProjectNotificationLog,
        now_unix_ms: u64,
    ) -> Result<StreamBatch, StreamRefusal> {
        self.take_stream_batch_picking(
            owner,
            stream_id,
            watch_ids,
            ack_seq,
            &StreamPick::default(),
            log,
            now_unix_ms,
        )
        .await
    }

    /// [`Self::take_stream_batch`], handing only what `pick` picks.
    #[allow(clippy::too_many_arguments)]
    pub async fn take_stream_batch_picking(
        &self,
        owner: &WatchOwner,
        stream_id: Option<&str>,
        watch_ids: &[String],
        ack_seq: Option<u64>,
        pick: &StreamPick,
        log: &ProjectNotificationLog,
        now_unix_ms: u64,
    ) -> Result<StreamBatch, StreamRefusal> {
        if watch_ids.is_empty() {
            return Err(StreamRefusal::NoWatchNamed);
        }
        // A wake still pasting the owner's records holds this; wait for it rather than hand them
        // again. One call is one locked step, so the guard is never held across a stream's wait.
        let guard = self.delivery_guard(owner);
        let _handing_over = guard.lock().await;
        let project = self.project_of(&owner.project_id);
        let mut state = project.lock().await;

        // Classify the named watches without moving anything: a refusal moves nothing.
        let mut live_ids: Vec<String> = Vec::new();
        let mut ended_ids: Vec<String> = Vec::new();
        for watch_id in watch_ids {
            let owned = state
                .watches
                .iter()
                .find(|watch| &watch.owner == owner && &watch.watch_id == watch_id);
            match owned {
                Some(watch) if end_reason(watch, log, now_unix_ms).is_none() => {
                    if !live_ids.contains(watch_id) {
                        live_ids.push(watch_id.clone());
                    }
                }
                Some(_) => {
                    if !ended_ids.contains(watch_id) {
                        ended_ids.push(watch_id.clone());
                    }
                }
                None if state.ended.iter().any(|pending| {
                    &pending.owner == owner && &pending.end.watch_id == watch_id
                }) =>
                {
                    if !ended_ids.contains(watch_id) {
                        ended_ids.push(watch_id.clone());
                    }
                }
                None => {
                    return Err(StreamRefusal::NotYourLiveWatch {
                        watch_id: watch_id.clone(),
                    });
                }
            }
        }

        // The lease snapshot this call is judged against.
        let leases_snapshot: HashMap<String, Lease> = {
            let leases = self
                .leases
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            leases.by_watch.clone()
        };
        let holds = |watch_id: &str, presented: &str, leases: &HashMap<String, Lease>| {
            leases.get(watch_id).is_some_and(|lease| {
                &lease.owner == owner
                    && lease.stream_id == presented
                    && now_unix_ms < lease.lease_until_unix_ms
            })
        };

        // An ended watch is reported only to the stream that was holding it.
        for watch_id in &ended_ids {
            let continued =
                stream_id.is_some_and(|presented| holds(watch_id, presented, &leases_snapshot));
            if !continued {
                return Err(StreamRefusal::NotYourLiveWatch {
                    watch_id: watch_id.clone(),
                });
            }
        }

        // Resolve the stream: a presented id must hold at least one named watch and no named
        // watch may be held by another live stream; no id starts (and takes over) a new stream.
        let effective_stream_id: String = match stream_id {
            Some(presented) => {
                for watch_id in live_ids.iter().chain(ended_ids.iter()) {
                    if let Some(lease) = leases_snapshot.get(watch_id) {
                        if &lease.owner == owner
                            && lease.stream_id != presented
                            && now_unix_ms < lease.lease_until_unix_ms
                        {
                            return Err(StreamRefusal::Superseded {
                                watch_id: watch_id.clone(),
                            });
                        }
                    }
                }
                let holds_any = live_ids
                    .iter()
                    .chain(ended_ids.iter())
                    .any(|watch_id| holds(watch_id, presented, &leases_snapshot));
                if !holds_any {
                    return Err(StreamRefusal::UnknownStream {
                        stream_id: presented.to_string(),
                    });
                }
                presented.to_string()
            }
            None => uuid::Uuid::new_v4().to_string(),
        };
        let starting = stream_id.is_none();

        // What each live watch had been handed before this call; acknowledgement may not pass it.
        let mut handed_before: HashMap<String, u64> = HashMap::new();
        for watch_id in &live_ids {
            let watch = state
                .watches
                .iter()
                .find(|watch| &watch.owner == owner && &watch.watch_id == watch_id)
                .expect("a live watch was classified above");
            let handed = if starting {
                watch.position
            } else {
                leases_snapshot
                    .get(watch_id)
                    .filter(|lease| &lease.owner == owner && lease.stream_id == effective_stream_id)
                    .map_or(watch.position, |lease| lease.handed)
            };
            if let Some(ack_seq) = ack_seq {
                if ack_seq > handed {
                    return Err(StreamRefusal::AckPastHanded { ack_seq, handed });
                }
            }
            handed_before.insert(watch_id.clone(), handed);
        }

        // Move the watches that have ended, and report the ended ones this stream held.
        let ended_moved = end_finished(&mut state, owner, log, now_unix_ms);
        let mut ended: Vec<WatchEnd> = Vec::new();
        if !ended_ids.is_empty() {
            let ended_set: HashSet<&str> = ended_ids.iter().map(String::as_str).collect();
            state.ended.retain(|pending| {
                if &pending.owner == owner && ended_set.contains(pending.end.watch_id.as_str()) {
                    ended.push(pending.end.clone());
                    false
                } else {
                    true
                }
            });
            ended.sort_by(|a, b| a.watch_id.cmp(&b.watch_id));
            // Report in the order the call named the watches.
            ended.sort_by_key(|end| {
                watch_ids
                    .iter()
                    .position(|id| id == &end.watch_id)
                    .unwrap_or(usize::MAX)
            });
        }

        // Advance only on acknowledgement — of what this stream's latest answer handed — and
        // persist what moved.
        let mut changed = ended_moved;
        if let Some(ack_seq) = ack_seq {
            let inbox = super::inbox_position_of(&state, owner);
            for watch in state
                .watches
                .iter_mut()
                .filter(|watch| &watch.owner == owner && live_ids.contains(&watch.watch_id))
            {
                let latest_batch = if starting {
                    None
                } else {
                    leases_snapshot.get(&watch.watch_id).filter(|lease| {
                        &lease.owner == owner && lease.stream_id == effective_stream_id
                    })
                };
                let acknowledged = latest_batch
                    .map(|lease| {
                        lease
                            .latest_batch
                            .iter()
                            .copied()
                            .filter(|seq| *seq <= ack_seq && *seq > watch.position)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                if acknowledge(watch, &acknowledged, ack_seq, log, inbox) {
                    changed = true;
                }
            }
        }
        if changed {
            self.persist_best_effort(&owner.project_id, &state);
        }

        // The batch: matched records after each watch's acknowledged position, unioned by seq.
        let mut matched: BTreeMap<u64, StreamedEvent> = BTreeMap::new();
        let mut lost = 0_u64;
        let mut gaps: BTreeMap<u64, u64> = BTreeMap::new();
        for watch_id in watch_ids {
            if !live_ids.contains(watch_id) {
                continue;
            }
            let Some(watch) = state
                .watches
                .iter()
                .find(|watch| &watch.owner == owner && &watch.watch_id == watch_id)
            else {
                continue;
            };
            let (records, watch_lost) =
                log.records_after(&owner.project_id, &owner.os_user, watch.position);
            let evicted_gaps = if watch_lost > 0 {
                log.evicted_relay_lost_after(&owner.project_id, &owner.os_user, watch.position)
            } else {
                0
            };
            lost = lost.max(watch_lost + evicted_gaps);
            gaps.extend(super::delivery::unreported_gaps(watch, &records));
            let inbox = super::inbox_position_of(&state, owner);
            for record in records
                .iter()
                .filter(|record| watch_owes(watch, record, inbox) && pick.picks(record))
            {
                let event = matched.entry(record.seq).or_insert_with(|| StreamedEvent {
                    record: record.clone(),
                    watch_ids: Vec::new(),
                });
                if !event.watch_ids.contains(watch_id) {
                    event.watch_ids.push(watch_id.clone());
                }
            }
        }
        // Relay gaps are reported once, here, and marked so on every streamed watch (n5).
        lost += gaps.values().sum::<u64>();
        if !gaps.is_empty() {
            for watch in state
                .watches
                .iter_mut()
                .filter(|watch| &watch.owner == owner && live_ids.contains(&watch.watch_id))
            {
                super::delivery::note_gaps_reported(watch, &gaps);
            }
            self.persist_best_effort(&owner.project_id, &state);
        }
        let events: Vec<StreamedEvent> = matched
            .into_values()
            .take(pick.max_events.clamp(1, MAX_EVENTS_PER_ANSWER))
            .collect();
        let max_position = live_ids
            .iter()
            .filter_map(|watch_id| {
                state
                    .watches
                    .iter()
                    .find(|watch| &watch.owner == owner && &watch.watch_id == watch_id)
                    .map(|watch| watch.position)
            })
            .max()
            .unwrap_or(0);
        let next_ack_seq = events
            .iter()
            .map(|event| event.record.seq)
            .max()
            .unwrap_or_else(|| ack_seq.unwrap_or(max_position).max(max_position));

        // Take or renew the lease on the live watches; an ended watch's lease is dropped.
        let grace_ms = u64::try_from(self.stream_lease_grace.as_millis()).unwrap_or(u64::MAX);
        {
            let mut leases = self
                .leases
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            for watch_id in &ended_ids {
                leases.by_watch.remove(watch_id);
            }
            for watch_id in &live_ids {
                let handed = handed_before
                    .get(watch_id)
                    .copied()
                    .unwrap_or(0)
                    .max(next_ack_seq);
                let latest_batch = events
                    .iter()
                    .filter(|event| event.watch_ids.contains(watch_id))
                    .map(|event| event.record.seq)
                    .collect();
                leases.by_watch.insert(
                    watch_id.clone(),
                    Lease {
                        owner: owner.clone(),
                        stream_id: effective_stream_id.clone(),
                        lease_until_unix_ms: now_unix_ms.saturating_add(grace_ms),
                        handed,
                        latest_batch,
                    },
                );
            }
        }

        Ok(StreamBatch {
            stream_id: effective_stream_id,
            events,
            lost,
            ended,
            next_ack_seq,
        })
    }

    /// Whether `owner`'s watch `watch_id` is held by a stream's lease at `now_unix_ms`.
    pub async fn is_streamed(&self, owner: &WatchOwner, watch_id: &str, now_unix_ms: u64) -> bool {
        let leases = self
            .leases
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        leases
            .by_watch
            .get(watch_id)
            .is_some_and(|lease| &lease.owner == owner && now_unix_ms < lease.lease_until_unix_ms)
    }

    /// The ids of `owner`'s watches a stream's lease holds at `now_unix_ms`.
    pub(super) fn streamed_watch_ids(
        &self,
        owner: &WatchOwner,
        now_unix_ms: u64,
    ) -> HashSet<String> {
        let leases = self
            .leases
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        leases
            .by_watch
            .iter()
            .filter(|(_, lease)| &lease.owner == owner && now_unix_ms < lease.lease_until_unix_ms)
            .map(|(watch_id, _)| watch_id.clone())
            .collect()
    }
}

/// One watch's lease: the stream holding it, when the lease lapses, the newest `seq` it was
/// handed, and the `seq`s of this watch's records in the stream's latest answer — what its next
/// `ack_seq` may acknowledge.
#[derive(Debug, Clone)]
pub(super) struct Lease {
    pub(super) owner: WatchOwner,
    pub(super) stream_id: String,
    pub(super) lease_until_unix_ms: u64,
    pub(super) handed: u64,
    pub(super) latest_batch: BTreeSet<u64>,
}

impl StreamPick {
    /// Whether this pick takes `record`: of a picked kind, from a picked session.
    fn picks(&self, record: &ProjectNotificationRecord) -> bool {
        let kind_picked = self.kinds.is_empty()
            || WatchKind::of(record).is_some_and(|kind| self.kinds.contains(&kind));
        let session_picked =
            self.session_ids.is_empty() || self.session_ids.contains(&record.session_id);
        kind_picked && session_picked
    }
}

/// Acknowledge `acknowledged` — records of `watch` a stream was handed, up to `ack_seq` — and
/// advance the watch's position as far as it may: up to `ack_seq`, but to just before the first
/// record past it the watch still owes. The acknowledged records past the new position are kept in
/// [`SessionWatch::acked_above`]. Whether anything moved.
fn acknowledge(
    watch: &mut SessionWatch,
    acknowledged: &[u64],
    ack_seq: u64,
    log: &ProjectNotificationLog,
    inbox: u64,
) -> bool {
    let before = (watch.position, watch.acked_above.clone());
    watch.acked_above.extend(acknowledged.iter().copied());
    let (records, _) = log.records_after(
        &watch.owner.project_id,
        &watch.owner.os_user,
        watch.position,
    );
    let first_owed = records
        .iter()
        .take_while(|record| record.seq <= ack_seq)
        .find(|record| watch_owes(watch, record, inbox))
        .map(|record| record.seq);
    let position = match first_owed {
        Some(seq) => seq.saturating_sub(1).max(watch.position),
        None => ack_seq.max(watch.position),
    };
    watch.position = position;
    watch.acked_above.retain(|seq| *seq > position);
    (watch.position, &watch.acked_above) != (before.0, &before.1)
}

/// The in-memory leases of every stream on this host.
#[derive(Debug, Default)]
pub(super) struct StreamLeases {
    pub(super) by_watch: HashMap<String, Lease>,
}
