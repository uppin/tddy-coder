//! The daemon side of the worker: requests matched to their answers by id, events delivered before
//! results, and the in-process loop a test drives over pipes.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Instant;

use super::wire::{RequestId, WorkerEnvelope, WorkerLine};
use super::{CloneRequest, SpawnRequest, SpawnResult, WorkerEvent, WorkerRequest, WorkerResponse};

/// Client for sending spawn requests to the worker. Used by the async daemon. Clones share one
/// worker, and concurrent calls each wait only for their own answer.
#[derive(Clone)]
pub struct SpawnClient {
    worker_pid: u32,
    next_id: Arc<AtomicU64>,
    request_tx: Arc<Mutex<File>>,
    pending: Arc<Mutex<Pending>>,
}

/// The requests written and not yet answered, by id. When the response pipe closes the reader
/// thread clears the table: dropping each sender wakes its caller, and `open` turns new requests away.
struct Pending {
    open: bool,
    by_id: HashMap<RequestId, mpsc::Sender<WorkerLine>>,
}

impl SpawnClient {
    /// [`SpawnClient::clone_repo`], with each [`WorkerEvent`] the worker reports for this request
    /// handed to `on_event` as it arrives, before the result.
    pub fn clone_repo_observed(
        &self,
        req: CloneRequest,
        on_event: impl FnMut(WorkerEvent),
    ) -> anyhow::Result<()> {
        match self.request(WorkerRequest::Clone(req), on_event)? {
            WorkerResponse::CloneOk => Ok(()),
            WorkerResponse::Error { message } => Err(anyhow::anyhow!("{}", message)),
            WorkerResponse::SpawnOk { .. } => Err(anyhow::anyhow!("unexpected spawn_ok for clone")),
        }
    }

    pub fn spawn(&self, req: SpawnRequest) -> anyhow::Result<SpawnResult> {
        match self.request(WorkerRequest::Spawn(Box::new(req)), |_| {})? {
            WorkerResponse::SpawnOk { result } => Ok(result),
            WorkerResponse::Error { message } => Err(anyhow::anyhow!("{}", message)),
            WorkerResponse::CloneOk => Err(anyhow::anyhow!("unexpected clone_ok for spawn")),
        }
    }

    pub fn clone_repo(&self, req: CloneRequest) -> anyhow::Result<()> {
        self.clone_repo_observed(req, |_| {})
    }

    /// Send one request under a fresh id and wait for its result, handing its events to `on_event`.
    fn request(
        &self,
        req: WorkerRequest,
        mut on_event: impl FnMut(WorkerEvent),
    ) -> anyhow::Result<WorkerResponse> {
        let label = worker_request_label(&req);
        let started = Instant::now();
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let envelope = serde_json::to_string(&WorkerEnvelope { id, request: req })?;
        let (answers_tx, answers_rx) = mpsc::channel();
        {
            // Registered before the write, so the answer cannot arrive before its route exists.
            let mut pending = self.pending.lock().unwrap();
            if !pending.open {
                anyhow::bail!(
                    "spawn worker pid={} exited; request {} not sent",
                    self.worker_pid,
                    id
                );
            }
            pending.by_id.insert(id, answers_tx);
        }
        log::info!("SpawnClient: sending {} id={}", label, id);
        if let Err(e) = self.write_envelope(&envelope) {
            self.pending.lock().unwrap().by_id.remove(&id);
            return Err(e);
        }
        log::info!("SpawnClient: waiting for worker {} id={}", label, id);
        loop {
            match answers_rx.recv() {
                Ok(WorkerLine::Event { event, .. }) => on_event(event),
                Ok(WorkerLine::Result { response, .. }) => {
                    log::info!(
                        "SpawnClient: received result elapsed_ms={} {}",
                        started.elapsed().as_millis(),
                        label
                    );
                    return Ok(response);
                }
                Err(mpsc::RecvError) => anyhow::bail!(
                    "spawn worker pid={} exited; request {} abandoned after {:?}",
                    self.worker_pid,
                    id,
                    started.elapsed()
                ),
            }
        }
    }

    fn write_envelope(&self, envelope: &str) -> anyhow::Result<()> {
        let mut tx = self.request_tx.lock().unwrap();
        tx.write_all(format!("{envelope}\n").as_bytes())
            .and_then(|()| tx.flush())
            .map_err(|e| {
                anyhow::anyhow!("spawn worker pid={} request pipe: {}", self.worker_pid, e)
            })
    }
}

fn worker_request_label(req: &WorkerRequest) -> String {
    match req {
        WorkerRequest::Spawn(s) => format!(
            "op=spawn session={} repo={}",
            s.resume_session_id
                .as_deref()
                .or(s.new_session_id.as_deref())
                .unwrap_or("new"),
            s.repo_path
        ),
        WorkerRequest::Clone(c) => format!("op=clone dest={} os_user={}", c.destination, c.os_user),
    }
}

/// Route every line the worker writes to the request it answers. The response pipe closing, or a
/// line that does not decode, ends the routing: the table is cleared, so every waiting caller fails
/// naming the worker's pid.
#[cfg(unix)]
fn route_worker_lines(response_reader: File, pending: Arc<Mutex<Pending>>, worker_pid: u32) {
    for line in BufReader::new(response_reader).lines() {
        let line = match line {
            Ok(line) => line,
            Err(e) => {
                log::error!(
                    "SpawnClient: reading from spawn worker pid={} failed: {}",
                    worker_pid,
                    e
                );
                break;
            }
        };
        match serde_json::from_str::<WorkerLine>(line.trim()) {
            Ok(routed) => deliver(&pending, routed),
            Err(e) => {
                let preview: String = line.chars().take(256).collect();
                log::error!(
                    "SpawnClient: invalid line from spawn worker pid={}: {} (preview: {:?})",
                    worker_pid,
                    e,
                    preview
                );
                break;
            }
        }
    }
    let mut pending = pending.lock().unwrap();
    pending.open = false;
    pending.by_id.clear();
    log::info!(
        "SpawnClient: spawn worker pid={} closed its response pipe",
        worker_pid
    );
}

/// Hand one line to the caller waiting on its id. A result is the last line for its id.
#[cfg(unix)]
fn deliver(pending: &Mutex<Pending>, line: WorkerLine) {
    let (id, is_result) = match &line {
        WorkerLine::Event { id, .. } => (*id, false),
        WorkerLine::Result { id, .. } => (*id, true),
    };
    let mut pending = pending.lock().unwrap();
    let Some(answers) = pending.by_id.get(&id) else {
        log::warn!(
            "SpawnClient: dropping a line for request id={} that is not pending",
            id
        );
        return;
    };
    // A caller that has stopped waiting has nothing left to deliver to.
    let _ = answers.send(line);
    if is_result {
        pending.by_id.remove(&id);
    }
}

/// Serve worker requests read from `request_reader`, writing lines to `response_writer`, on a
/// thread of this process — the worker loop without the fork, so a test can drive it over pipes.
#[cfg(unix)]
pub fn spawn_worker_over(
    request_reader: File,
    response_writer: File,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || super::serve::serve(request_reader, response_writer))
}

/// A [`SpawnClient`] over an existing pair of pipe ends, talking to the worker whose pid is
/// `worker_pid` (named in errors when it goes away).
#[cfg(unix)]
pub fn client_over(request_writer: File, response_reader: File, worker_pid: u32) -> SpawnClient {
    let pending = Arc::new(Mutex::new(Pending {
        open: true,
        by_id: HashMap::new(),
    }));
    let routed = Arc::clone(&pending);
    std::thread::spawn(move || route_worker_lines(response_reader, routed, worker_pid));
    SpawnClient {
        worker_pid,
        next_id: Arc::new(AtomicU64::new(1)),
        request_tx: Arc::new(Mutex::new(request_writer)),
        pending,
    }
}
