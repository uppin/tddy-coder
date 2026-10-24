//! The daemon side of the worker: requests matched to their answers by id, events delivered before
//! results, and the in-process loop a test drives over pipes.

use std::io::BufReader;
use std::sync::Arc;

use super::{CloneRequest, SpawnClient, WorkerEvent};

impl SpawnClient {
    /// [`SpawnClient::clone_repo`], with each [`WorkerEvent`] the worker reports for this request
    /// handed to `on_event` as it arrives, before the result.
    pub fn clone_repo_observed(
        &self,
        req: CloneRequest,
        on_event: impl FnMut(WorkerEvent),
    ) -> anyhow::Result<()> {
        // TODO(worker-mux): implement — envelope with a fresh id, route `event` lines to on_event
        let _ = (req, on_event);
        anyhow::bail!("clone_repo_observed: not implemented")
    }
}

/// Serve worker requests read from `request_reader`, writing lines to `response_writer`, on a
/// thread of this process — the worker loop without the fork, so a test can drive it over pipes.
#[cfg(unix)]
pub fn spawn_worker_over(
    request_reader: std::fs::File,
    response_writer: std::fs::File,
) -> std::thread::JoinHandle<()> {
    // TODO(worker-mux): implement — the same loop `fork_spawn_worker`'s child runs
    let _ = (request_reader, response_writer);
    std::thread::spawn(|| ())
}

/// A [`SpawnClient`] over an existing pair of pipe ends, talking to the worker whose pid is
/// `worker_pid` (named in errors when it goes away).
#[cfg(unix)]
pub fn client_over(
    request_writer: std::fs::File,
    response_reader: std::fs::File,
    worker_pid: u32,
) -> SpawnClient {
    // TODO(worker-mux): implement — start the reader thread routing lines by id
    let _ = worker_pid;
    SpawnClient {
        request_tx: Arc::new(std::sync::Mutex::new(request_writer)),
        response_rx: Arc::new(std::sync::Mutex::new(BufReader::new(response_reader))),
    }
}
