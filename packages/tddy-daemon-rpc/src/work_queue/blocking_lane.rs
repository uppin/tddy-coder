//! A bounded, coalescing lane onto the blocking pool for `QueryBranch`'s git legs.
//!
//! TODO(red): stub — implemented in the green commit.

use std::sync::Arc;
use std::time::Duration;
use tddy_rpc::Status;
use tokio::sync::Semaphore;

pub(crate) struct BlockingLane<T> {
    _permits: Arc<Semaphore>,
    _marker: std::marker::PhantomData<T>,
}

impl<T: Clone + Send + 'static> BlockingLane<T> {
    pub(crate) fn new(permits: Arc<Semaphore>) -> Self {
        Self {
            _permits: permits,
            _marker: std::marker::PhantomData,
        }
    }

    pub(crate) async fn run(
        &self,
        _key: String,
        _timeout: Duration,
        _op_label: &'static str,
        _job: impl FnOnce() -> anyhow::Result<T> + Send + 'static,
    ) -> Result<T, Status> {
        todo!("BlockingLane::run")
    }
}
