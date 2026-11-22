//! What one stream call asks for and why one is refused: the pick a `WaitSessionWatchEvents` call
//! narrows its batch with, and the refusals of [`super::SessionWatchStore::take_stream_batch`].
//!
//! Feature: docs/ft/daemon/project-concept.md#watching-sessions

use crate::session_watches::MAX_EVENTS_PER_ANSWER;

/// What one stream call picks from what its watches matched: at most `max_events` records, of
/// `kinds` (every kind when empty), of `session_ids` (every session when empty). What it does not
/// pick stays pending for a later call — it is neither handed nor acknowledged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamPick {
    pub kinds: Vec<super::WatchKind>,
    pub session_ids: Vec<String>,
    pub max_events: usize,
}

impl Default for StreamPick {
    /// Everything the watches matched, up to [`MAX_EVENTS_PER_ANSWER`].
    fn default() -> Self {
        Self {
            kinds: Vec::new(),
            session_ids: Vec::new(),
            max_events: MAX_EVENTS_PER_ANSWER,
        }
    }
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
