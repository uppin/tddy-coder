//! `WatchWorkQueue`: the queue itself, live — its snapshot, then one per new revision, whoever wrote
//! it.

use super::WorkQueueRpcHandler;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::work_queue::WatchWorkQueueRequest;
use tddy_work_queue::rpc::WorkQueueWatchStream;

impl WorkQueueRpcHandler {
    pub(super) async fn watch_work_queue_rpc(
        &self,
        _request: Request<WatchWorkQueueRequest>,
    ) -> Result<Response<WorkQueueWatchStream>, Status> {
        // TODO(red): served by /green.
        Err(Status::unimplemented("WatchWorkQueue"))
    }
}
