//! A bounded, coalescing lane onto the blocking pool for `QueryBranch`'s git legs.
//!
//! `QueryBranch` is polled once per rendered work-queue row every few seconds. Each poll used to put
//! its git subprocesses straight onto the blocking pool, and a timed-out call left its job running
//! there — so when git slowed down the pool filled with jobs nobody waited for, and every other RPC
//! that needs the pool (`ListSessions` among them) timed out behind them (2026-11-26: ~7,900
//! timeouts and an empty session list).
//!
//! A lane caps that cost two ways:
//! - **Coalescing**: a request for a key already in flight waits for that job's answer instead of
//!   starting another one.
//! - **Bounding**: at most `permits` jobs run at once, and a permit is held until the job itself
//!   finishes — not until its caller gives up. A full lane refuses a new key at once with
//!   `Unavailable`, so the pool never holds more of this work than the lane allows.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use tddy_rpc::Status;
use tokio::sync::{watch, Semaphore};

/// What every waiter on one in-flight job receives: the value, or the job's error as text.
type Shared<T> = Option<Result<T, String>>;

pub(crate) struct BlockingLane<T> {
    permits: Arc<Semaphore>,
    in_flight: Arc<Mutex<HashMap<String, watch::Receiver<Shared<T>>>>>,
}

impl<T: Clone + Send + Sync + 'static> BlockingLane<T> {
    /// A lane drawing on `permits` — which several lanes may share, so their jobs are bounded
    /// together.
    pub(crate) fn new(permits: Arc<Semaphore>) -> Self {
        Self {
            permits,
            in_flight: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Run `job` for `key` on the blocking pool, or join the job already running for `key`.
    ///
    /// Waits at most `timeout` for the answer. A timed-out caller leaves the job running, still
    /// holding its permit, so later callers for the same key join it rather than adding another.
    pub(crate) async fn run(
        &self,
        key: String,
        timeout: Duration,
        op_label: &'static str,
        job: impl FnOnce() -> anyhow::Result<T> + Send + 'static,
    ) -> Result<T, Status> {
        let answer = self.join_or_start(key, op_label, job)?;
        await_answer(answer, timeout, op_label).await
    }

    fn join_or_start(
        &self,
        key: String,
        op_label: &'static str,
        job: impl FnOnce() -> anyhow::Result<T> + Send + 'static,
    ) -> Result<watch::Receiver<Shared<T>>, Status> {
        let mut in_flight = self
            .in_flight
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(running) = in_flight.get(&key) {
            return Ok(running.clone());
        }
        let permit = Arc::clone(&self.permits).try_acquire_owned().map_err(|_| {
            log::warn!("{op_label}: the blocking lane is full — refusing instead of queueing");
            Status::unavailable(format!(
                "{op_label}: too many git reads are already running; try again shortly"
            ))
        })?;
        let (publish, answer) = watch::channel(None);
        in_flight.insert(key.clone(), answer.clone());
        drop(in_flight);

        let registry = Arc::clone(&self.in_flight);
        let running = tokio::task::spawn_blocking(move || {
            let result = job().map_err(|e| e.to_string());
            drop(permit);
            result
        });
        tokio::spawn(async move {
            let result = running.await.unwrap_or_else(|e| Err(e.to_string()));
            registry
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&key);
            let _ = publish.send(Some(result));
        });
        Ok(answer)
    }
}

async fn await_answer<T: Clone>(
    mut answer: watch::Receiver<Shared<T>>,
    timeout: Duration,
    op_label: &'static str,
) -> Result<T, Status> {
    let waited = tokio::time::timeout(timeout, answer.wait_for(Option::is_some)).await;
    match waited {
        Ok(Ok(published)) => match published.clone() {
            Some(Ok(value)) => Ok(value),
            Some(Err(reason)) => {
                log::error!("{op_label} failed: {reason}");
                Err(Status::internal(reason))
            }
            None => Err(Status::internal(format!(
                "{op_label}: the job ended without an answer"
            ))),
        },
        Ok(Err(_closed)) => Err(Status::internal(format!(
            "{op_label}: the job ended without an answer"
        ))),
        Err(_elapsed) => {
            log::error!(
                "{op_label} timed out after {}s; the job keeps its lane slot until it finishes",
                timeout.as_secs()
            );
            Err(Status::deadline_exceeded(format!(
                "{op_label}: timed out after {}s",
                timeout.as_secs()
            )))
        }
    }
}

/// How many `QueryBranch` blocking jobs may run at once, across all of its legs.
const QUERY_BRANCH_PERMITS: usize = 4;

/// One lane per `QueryBranch` leg that runs on the blocking pool, all bounded by one shared set of
/// [`QUERY_BRANCH_PERMITS`].
pub(crate) struct QueryBranchLanes {
    pub(crate) session: BlockingLane<Option<tddy_worktree_service::branch_owner::SessionClaim>>,
    pub(crate) worktree: BlockingLane<tddy_service::proto::work_queue::BranchWorktree>,
    pub(crate) remote: BlockingLane<tddy_service::proto::work_queue::BranchRemote>,
    pub(crate) base_sync: BlockingLane<Result<tddy_core::base_sync::BranchBaseSync, String>>,
}

impl QueryBranchLanes {
    pub(crate) fn new() -> Self {
        let permits = Arc::new(Semaphore::new(QUERY_BRANCH_PERMITS));
        Self {
            session: BlockingLane::new(Arc::clone(&permits)),
            worktree: BlockingLane::new(Arc::clone(&permits)),
            remote: BlockingLane::new(Arc::clone(&permits)),
            base_sync: BlockingLane::new(permits),
        }
    }
}
