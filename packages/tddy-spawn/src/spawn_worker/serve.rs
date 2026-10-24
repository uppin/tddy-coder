//! The worker's loop: read request envelopes from the request pipe and serve each on a thread of its
//! own, writing every line it produces under one write lock on the response pipe, so a slow request
//! never holds up a fast one and no two lines interleave.
//!
//! The forked worker's child and `spawn_worker_over` both run this loop.

use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::sync::Mutex;

use super::wire::{RequestId, WorkerEnvelope, WorkerLine};
use super::{run_spawn, CloneRequest, WorkerEvent, WorkerRequest, WorkerResponse};

/// Serve requests until the request pipe closes, then return once every request still running has
/// written its result.
pub(super) fn serve(request_reader: File, response_writer: File) {
    log::info!("spawn_worker: started, waiting for requests");
    let response_writer = Mutex::new(response_writer);
    std::thread::scope(|scope| {
        for line in BufReader::new(request_reader).lines() {
            let line = match line {
                Ok(line) => line,
                Err(_) => break,
            };
            log::info!("spawn_worker: received request ({} bytes)", line.len());
            let out = &response_writer;
            scope.spawn(move || serve_line(&line, out));
        }
    });
}

fn serve_line(line: &str, out: &Mutex<File>) {
    let envelope: WorkerEnvelope = match serde_json::from_str(line) {
        Ok(envelope) => envelope,
        Err(e) => {
            log::error!("spawn_worker: unreadable request: {}", e);
            // Only a line that names its id can be answered: nobody else is waiting on it.
            if let Some(id) = line_id(line) {
                let response = WorkerResponse::Error {
                    message: e.to_string(),
                };
                write_line(out, &WorkerLine::Result { id, response });
            }
            return;
        }
    };
    let id = envelope.id;
    let response = match envelope.request {
        WorkerRequest::Spawn(req) => run_spawn(&req),
        WorkerRequest::Clone(req) => run_clone(&req, |pid| {
            write_line(
                out,
                &WorkerLine::Event {
                    id,
                    event: WorkerEvent::Started { pid },
                },
            );
        }),
    };
    write_line(out, &WorkerLine::Result { id, response });
}

fn run_clone(req: &CloneRequest, on_started: impl FnOnce(u32)) -> WorkerResponse {
    log::info!(
        "spawn_worker: calling clone_as_user os_user={} dest={}",
        req.os_user,
        req.destination
    );
    let result = req.run(on_started);
    match &result {
        Ok(()) => log::info!("spawn_worker: clone_as_user ok dest={}", req.destination),
        Err(e) => log::warn!(
            "spawn_worker: clone_as_user err dest={} err={}",
            req.destination,
            e
        ),
    }
    match result {
        Ok(()) => WorkerResponse::CloneOk,
        Err(e) => WorkerResponse::Error {
            message: e.to_string(),
        },
    }
}

/// The request id a line carries, when it is JSON with one.
fn line_id(line: &str) -> Option<RequestId> {
    serde_json::from_str::<serde_json::Value>(line)
        .ok()?
        .get("id")?
        .as_u64()
}

/// Write one line and flush it, holding the response pipe's write lock for the whole line.
fn write_line(out: &Mutex<File>, line: &WorkerLine) {
    let json = serde_json::to_string(line).expect("a worker line always serializes");
    let mut writer = out.lock().expect("the response pipe's write lock");
    let written = writer
        .write_all(format!("{json}\n").as_bytes())
        .and_then(|()| writer.flush());
    if let Err(e) = written {
        log::error!("spawn_worker: failed to write a response line: {}", e);
    }
}
