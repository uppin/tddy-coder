//! Streaming a watch to a process: the acknowledged position and the stream lease
//! (`#activity-wake` 4/5).
//!
//! A stream reads the same watches and positions as [`SessionWatchStore::take_delivery`] and
//! [`SessionWatchStore::deliver_with`], so it must hold a watch exclusively while it runs. It does
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
//! Feature: docs/ft/daemon/1-WIP/PRD-2026-11-17-session-watch-monitor.md

use std::time::Duration;

use super::{SessionWatchStore, WatchEnd, WatchOwner};
use crate::project_notification_log::{ProjectNotificationLog, ProjectNotificationRecord};

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

/// Why a stream call was refused. Nothing moves on a refusal: no position, no lease.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamRefusal {
    /// The call named no watch.
    NoWatchNamed,
    /// A named watch is not a live watch of the caller: another orchestrator's, unknown, or ended.
    NotYourLiveWatch { watch_id: String },
    /// This stream no longer holds a named watch: a newer stream of the same owner took it over —
    /// the monitor was resumed elsewhere (`--watch`).
    Superseded { watch_id: String },
    /// `ack_seq` is past what this stream was handed.
    AckPastHanded { ack_seq: u64, handed: u64 },
    /// `stream_id` names no stream holding the named watches.
    UnknownStream { stream_id: String },
}

impl std::fmt::Display for StreamRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StreamRefusal::NoWatchNamed => write!(
                f,
                "name at least one watch_id: the watch_id CreateSessionWatch or ListSessionWatches returned"
            ),
            StreamRefusal::NotYourLiveWatch { watch_id } => write!(
                f,
                "no live watch {watch_id} of yours: list your watches with ListSessionWatches"
            ),
            StreamRefusal::Superseded { watch_id } => write!(
                f,
                "watch {watch_id} was taken over by another monitor: this stream no longer holds it — resume it with tddy-tools monitor --watch {watch_id} if this one should stream it"
            ),
            StreamRefusal::AckPastHanded { ack_seq, handed } => write!(
                f,
                "ack_seq {ack_seq} is past what this stream was handed ({handed}): acknowledge the next_ack_seq an answer returned"
            ),
            StreamRefusal::UnknownStream { stream_id } => write!(
                f,
                "stream {stream_id} holds none of these watches: start a new stream by calling without stream_id"
            ),
        }
    }
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
        // TODO(watch-monitor): implement — acknowledge, take or renew the lease, hand the batch.
        let _ = (
            stream_id,
            ack_seq,
            log,
            now_unix_ms,
            owner,
            &self.leases,
            self.stream_lease_grace,
        );
        Err(StreamRefusal::NotYourLiveWatch {
            watch_id: watch_ids.first().cloned().unwrap_or_default(),
        })
    }

    /// Whether `owner`'s watch `watch_id` is held by a stream's lease at `now_unix_ms`.
    pub async fn is_streamed(&self, owner: &WatchOwner, watch_id: &str, now_unix_ms: u64) -> bool {
        // TODO(watch-monitor): implement — read the lease map.
        let _ = (owner, watch_id, now_unix_ms);
        false
    }
}

/// The in-memory leases of every stream on this host.
#[derive(Debug, Default)]
pub(super) struct StreamLeases {
    // TODO(watch-monitor): implement — per watch: the holding stream, its lease's end and what it
    // was handed.
}
