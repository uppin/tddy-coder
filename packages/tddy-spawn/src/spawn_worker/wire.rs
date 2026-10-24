//! The worker's wire format: every request carries an id, and every line the worker writes back —
//! an event about a request still running, or its result — names the id it answers. That is what
//! lets several requests be in flight at once without a caller ever reading another's answer.

use serde::{Deserialize, Serialize};

use super::{WorkerRequest, WorkerResponse};

/// Identifies one request for as long as it is in flight. Assigned by the client, echoed by the worker.
pub type RequestId = u64;

/// A request as it crosses the pipe: `{"id":N,"op":…,"payload":…}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerEnvelope {
    pub id: RequestId,
    #[serde(flatten)]
    pub request: WorkerRequest,
}

/// One line from the worker: an event about request `id`, or its terminal result.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "line")]
pub enum WorkerLine {
    #[serde(rename = "event")]
    Event { id: RequestId, event: WorkerEvent },
    #[serde(rename = "result")]
    Result {
        id: RequestId,
        response: WorkerResponse,
    },
}

/// Something that happened to a request before its result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum WorkerEvent {
    /// The request's child process is running, with this pid.
    #[serde(rename = "started")]
    Started { pid: u32 },
}
