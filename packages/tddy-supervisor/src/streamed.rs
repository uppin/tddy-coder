//! `SpawnStreamed`: one binary as another OS user, its stderr streamed line by line.
//!
//! The generic successor of `CloneRepository`: the supervisor runs what the daemon asked for, under
//! the same user, exec and environment gates as `SpawnSession`, and reports `Started{pid}`, one
//! `Line` per line of stderr (split on `\r` as well as `\n`), then `Exited{code, signal,
//! stderr_tail}`. It knows nothing about what the binary is: a clone's progress is parsed, and its
//! partial directory moved into place, by the daemon.

use std::collections::VecDeque;
use std::io::Read;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::Stream;
use tddy_rpc::Status;
use tokio::sync::mpsc;

use crate::proto::supervisor::streamed_event::Event;
use crate::proto::supervisor::{StreamedEvent, StreamedExited, StreamedLine, StreamedStarted};
use crate::supervisor::Supervisor;

/// How many of the child's last stderr lines `Exited` carries.
const STDERR_TAIL_LINES: usize = 8;

/// How long stderr is read for after the child has exited.
const STDERR_DRAIN_AFTER_EXIT: Duration = Duration::from_secs(2);

/// Events buffered between the child and a slow reader.
const EVENT_BUFFER: usize = 64;

/// A streamed spawn's events as the generated server's stream type.
pub(crate) type StreamedEvents =
    std::pin::Pin<Box<dyn Stream<Item = Result<StreamedEvent, Status>> + Send>>;

fn event(event: Event) -> StreamedEvent {
    StreamedEvent { event: Some(event) }
}

/// Report on a running streamed spawn until it has ended.
pub(crate) fn report_on(
    supervisor: Arc<Supervisor>,
    pid: u32,
    stderr: std::process::ChildStderr,
    stop_grace: Duration,
) -> StreamedEvents {
    let (events, receiver) = mpsc::channel(EVENT_BUFFER);
    tokio::spawn(drive(supervisor, pid, stderr, stop_grace, events));
    Box::pin(futures_util::stream::unfold(
        receiver,
        |mut receiver| async { receiver.recv().await.map(|item| (item, receiver)) },
    ))
}

async fn drive(
    supervisor: Arc<Supervisor>,
    pid: u32,
    stderr: std::process::ChildStderr,
    stop_grace: Duration,
    events: mpsc::Sender<Result<StreamedEvent, Status>>,
) {
    let mut abandoned = events
        .send(Ok(event(Event::Started(StreamedStarted { pid }))))
        .await
        .is_err();
    let tail = Arc::new(Mutex::new(VecDeque::new()));
    let (lines, mut line_receiver) = mpsc::channel::<String>(EVENT_BUFFER);
    read_lines_in_the_background(stderr, lines, Arc::clone(&tail));
    let stop = |supervisor: &Arc<Supervisor>| {
        if let Err(error) = supervisor.stop_session(pid, stop_grace) {
            log::warn!(target: "tddy_supervisor::streamed", "could not stop streamed spawn {pid}: {error}");
        }
    };
    if abandoned {
        stop(&supervisor);
    }

    let mut exit = Box::pin(supervisor.wait_for_session_exit(pid));
    let mut status = None;
    while status.is_none() && !abandoned {
        tokio::select! {
            line = line_receiver.recv() => match line {
                Some(line) => {
                    let ev = event(Event::Line(StreamedLine { line }));
                    if events.send(Ok(ev)).await.is_err() {
                        abandoned = true;
                        stop(&supervisor);
                    }
                }
                None => status = Some((&mut exit).await),
            },
            exited = &mut exit => status = Some(exited),
        }
    }
    let status = match status {
        Some(status) => status,
        None => exit.await,
    };
    if !abandoned {
        let _ = tokio::time::timeout(STDERR_DRAIN_AFTER_EXIT, async {
            while let Some(line) = line_receiver.recv().await {
                let ev = event(Event::Line(StreamedLine { line }));
                if events.send(Ok(ev)).await.is_err() {
                    break;
                }
            }
        })
        .await;
    }

    let last = match status {
        Ok(status) => Ok(event(Event::Exited(StreamedExited {
            code: status.exit_code,
            signal: None,
            stderr_tail: joined(&tail),
        }))),
        Err(error) => Err(Status::internal(error.to_string())),
    };
    let _ = events.send(last).await;
}

fn joined(tail: &Mutex<VecDeque<String>>) -> String {
    tail.lock()
        .map(|lines| lines.iter().cloned().collect::<Vec<_>>().join("\n"))
        .unwrap_or_default()
}

fn read_lines_in_the_background(
    stderr: std::process::ChildStderr,
    lines: mpsc::Sender<String>,
    tail: Arc<Mutex<VecDeque<String>>>,
) {
    let spawned = std::thread::Builder::new()
        .name("tddy-supervisor-streamed-stderr".to_string())
        .spawn(move || {
            for_each_line(stderr, |line| {
                if let Ok(mut tail) = tail.lock() {
                    tail.push_back(line.to_string());
                    if tail.len() > STDERR_TAIL_LINES {
                        tail.pop_front();
                    }
                }
                lines.blocking_send(line.to_string()).is_ok()
            });
        });
    if let Err(error) = spawned {
        log::error!(target: "tddy_supervisor::streamed", "could not read a streamed spawn's stderr: {error}");
    }
}

/// Call `on_line` with each non-empty, trimmed line of `reader`, split on `\r` as well as `\n`.
fn for_each_line(mut reader: impl Read, mut on_line: impl FnMut(&str) -> bool) {
    let mut chunk = [0u8; 4096];
    let mut line: Vec<u8> = Vec::new();
    let mut emit = |line: &mut Vec<u8>| -> bool {
        let keep_going = {
            let text = String::from_utf8_lossy(line);
            let text = text.trim();
            text.is_empty() || on_line(text)
        };
        line.clear();
        keep_going
    };
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                for &byte in &chunk[..n] {
                    if byte == b'\r' || byte == b'\n' {
                        if !emit(&mut line) {
                            return;
                        }
                    } else {
                        line.push(byte);
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
    emit(&mut line);
}
