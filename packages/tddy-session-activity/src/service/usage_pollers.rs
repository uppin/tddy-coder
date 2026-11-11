//! The per-session usage pollers `StreamSessionUsage` shares between a session's subscribers.
//!
//! Usage is evaluated **while watched**: the first subscriber of a session starts a poller, every
//! later one joins it, and the poller stops once the last subscriber has gone. A poller reads the
//! session's recorded usage context each tick and publishes the snapshot only when it differs from
//! the last one, so a subscriber sees the current snapshot at once and then each change.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use tddy_rpc::Status;
use tddy_service::proto::activity::{SessionUsageSnapshot, UsageConversation};
use tddy_usage::gather_session_usage_for;
use tddy_usage::token_accounting::ConversationRecord;
use tddy_worktree_service::stream::MpscResultStream;
use tokio::sync::{mpsc, watch};

use crate::service::ports::UsageBindings;

/// How often a poller re-evaluates its session's usage.
const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// The latest snapshot of one session; `None` until the poller has evaluated it once.
type Latest = watch::Sender<Option<SessionUsageSnapshot>>;

/// The running pollers, one per session directory.
#[derive(Clone, Default)]
pub(crate) struct UsagePollers {
    running: Arc<Mutex<HashMap<PathBuf, Latest>>>,
}

impl UsagePollers {
    /// `session_dir`'s usage as a server stream: the poller's first evaluation (or its latest, when
    /// a poller was already running), then each change. Dropping the stream leaves the poller at
    /// once, so the last subscriber to go stops it without waiting for another change.
    pub(crate) fn stream(
        &self,
        session_dir: &Path,
        bindings: &UsageBindings,
    ) -> MpscResultStream<SessionUsageSnapshot> {
        let snapshots = self.join(session_dir, bindings);
        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(relay(snapshots, tx));
        MpscResultStream::from(rx)
    }

    /// A receiver of `session_dir`'s snapshots: joins the session's poller, or starts one that
    /// evaluates `bindings` against it.
    pub(crate) fn join(
        &self,
        session_dir: &Path,
        bindings: &UsageBindings,
    ) -> watch::Receiver<Option<SessionUsageSnapshot>> {
        let mut running = self.running.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(latest) = running.get(session_dir).filter(|l| !l.is_closed()) {
            return latest.subscribe();
        }
        let (latest, rx) = watch::channel(None);
        running.insert(session_dir.to_path_buf(), latest.clone());
        tokio::spawn(poll(
            session_dir.to_path_buf(),
            bindings.clone(),
            latest,
            self.clone(),
        ));
        rx
    }

    /// Forget `session_dir`'s poller once its subscribers are gone. A poller a later subscriber
    /// replaced is live and stays.
    fn forget_if_closed(&self, session_dir: &Path) {
        let mut running = self.running.lock().unwrap_or_else(PoisonError::into_inner);
        if running.get(session_dir).is_some_and(Latest::is_closed) {
            running.remove(session_dir);
        }
    }
}

/// Forward each snapshot from `snapshots` to `tx` until either side goes away.
async fn relay(
    mut snapshots: watch::Receiver<Option<SessionUsageSnapshot>>,
    tx: mpsc::UnboundedSender<Result<SessionUsageSnapshot, Status>>,
) {
    loop {
        let latest = snapshots.borrow_and_update().clone();
        if let Some(snapshot) = latest {
            if tx.send(Ok(snapshot)).is_err() {
                return;
            }
        }
        tokio::select! {
            changed = snapshots.changed() => if changed.is_err() { return },
            () = tx.closed() => return,
        }
    }
}

/// Evaluate `session_dir`'s usage every tick, publishing each changed snapshot, until nobody
/// listens.
async fn poll(
    session_dir: PathBuf,
    bindings: UsageBindings,
    latest: Latest,
    pollers: UsagePollers,
) {
    loop {
        let dir = session_dir.clone();
        let bindings = bindings.clone();
        let records =
            tokio::task::spawn_blocking(move || gather_session_usage_for(&dir, &*bindings)).await;
        match records {
            Ok(records) => {
                let snapshot = snapshot_of(records);
                latest.send_if_modified(|current| {
                    if current.as_ref() == Some(&snapshot) {
                        return false;
                    }
                    *current = Some(snapshot);
                    true
                });
            }
            Err(e) => log::warn!("usage: evaluating {} failed: {e}", session_dir.display()),
        }
        // `closed()` resolves when the last subscriber is gone; the timeout is the tick.
        if tokio::time::timeout(POLL_INTERVAL, latest.closed())
            .await
            .is_ok()
        {
            break;
        }
    }
    pollers.forget_if_closed(&session_dir);
}

fn snapshot_of(records: Vec<ConversationRecord>) -> SessionUsageSnapshot {
    SessionUsageSnapshot {
        conversations: records.into_iter().map(conversation).collect(),
    }
}

fn conversation(record: ConversationRecord) -> UsageConversation {
    UsageConversation {
        agent: record.agent,
        id: record.id,
        model: record.model,
        input_tokens: record.input_tokens,
        cache_read_tokens: record.cache_read_tokens,
        cache_write_tokens: record.cache_write_tokens,
        output_tokens: record.output_tokens,
        reasoning_tokens: record.reasoning_tokens,
        total_tokens: record.total_tokens,
        turns: record.turns,
        cost_usd: record.cost_usd,
        parent_id: record.parent_id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_bindings() -> UsageBindings {
        Arc::new(|_: &str| None)
    }

    async fn until_no_poller_runs(pollers: &UsagePollers) -> bool {
        for _ in 0..100 {
            if pollers.running.lock().unwrap().is_empty() {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        false
    }

    #[tokio::test]
    async fn a_second_subscriber_joins_the_running_poller() {
        let pollers = UsagePollers::default();
        let dir = tempfile::tempdir().unwrap();

        let _first = pollers.join(dir.path(), &no_bindings());
        let _second = pollers.join(dir.path(), &no_bindings());

        assert_eq!(pollers.running.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_dropped_stream_stops_the_poller_without_waiting_for_a_change() {
        let pollers = UsagePollers::default();
        let dir = tempfile::tempdir().unwrap();
        let mut stream = pollers.stream(dir.path(), &no_bindings());
        let _first = futures_util::StreamExt::next(&mut stream).await;

        drop(stream);

        assert!(until_no_poller_runs(&pollers).await);
    }

    #[tokio::test]
    async fn the_poller_stops_when_its_last_subscriber_leaves() {
        let pollers = UsagePollers::default();
        let dir = tempfile::tempdir().unwrap();
        let first = pollers.join(dir.path(), &no_bindings());
        let second = pollers.join(dir.path(), &no_bindings());

        drop(first);
        assert_eq!(pollers.running.lock().unwrap().len(), 1);
        drop(second);

        assert!(until_no_poller_runs(&pollers).await);
    }
}
