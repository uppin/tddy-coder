//! The append-only file a record is written to: one JSON object per line, a `start` line when a
//! process exists and an `end` line when it was waited on, never truncated.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Map, Value};
use tddy_lsp::{OperationContext, ProcessOutcome, ProcessStart, ProcessToken, SpawnObserver};

use super::purpose;
use super::redact::redacted;

/// How long an append may take before the record gives up on its sink: a line lands on a local
/// file in milliseconds. A thread still stuck in a refused append past the deadline is left to
/// the process's exit to collect.
const A_LINE_LANDS_WITHIN: std::time::Duration = std::time::Duration::from_secs(1);

/// A [`SpawnObserver`] that appends what it is told to a file, applying the redaction policy
/// before a byte is written.
pub struct JsonlSpawnRecord {
    /// Where the lines go and through what port: re-appended per line (the file is opened,
    /// written and closed by each append) rather than held open, which reads the same from
    /// outside — lines in order, never truncated.
    sink: Mutex<(std::sync::Arc<dyn tddy_fs::Fs>, PathBuf)>,
    /// Cleared once a write has failed, so a record that cannot be written stops being attempted
    /// rather than failing the run it is observing.
    recording: AtomicBool,
    book: RecordBook,
}

impl JsonlSpawnRecord {
    /// Open `path` for appending, creating it when it is absent. Existing lines are never
    /// touched, so a second run appends after the first.
    pub fn open(path: &Path) -> io::Result<Self> {
        Self::open_on(path, std::sync::Arc::new(tddy_fs::DiskFs))
    }

    /// [`JsonlSpawnRecord::open`] through the filesystem port.
    ///
    /// The proving append runs on its own thread under a deadline: a record path whose write end
    /// blocks — a pipe whose reader has not come, say — must be refused rather than hold the
    /// caller forever, so past the deadline the open reports [`io::ErrorKind::WouldBlock`].
    pub fn open_on(path: &Path, fs: std::sync::Arc<dyn tddy_fs::Fs>) -> io::Result<Self> {
        // Opening is what proves the file can be appended to, so the failure a caller sees
        // is the open's rather than the first line's.
        let (done, completed) = std::sync::mpsc::channel();
        let opening = {
            let fs = fs.clone();
            let path = path.to_path_buf();
            move || {
                let _ = done.send(fs.append(&path, b""));
            }
        };
        let _ = std::thread::spawn(opening);
        match completed.recv_timeout(A_LINE_LANDS_WITHIN) {
            Ok(result) => result?,
            Err(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    format!(
                        "the record at {} did not open within {A_LINE_LANDS_WITHIN:?}",
                        path.display()
                    ),
                ));
            }
        }
        Ok(Self {
            sink: Mutex::new((fs, path.to_path_buf())),
            recording: AtomicBool::new(true),
            book: RecordBook::new(),
        })
    }

    /// Append one line, and stop recording for good if the file will not take it.
    ///
    /// The append runs on its own thread under a deadline. A line lands on a local file in
    /// milliseconds; an append that does not come back — a record path whose write end blocks,
    /// such as a pipe whose only reader has gone — would otherwise hold the run it is observing
    /// forever, so past the deadline the recording stops and the run carries on without it. The
    /// mutex is held across the attempt, so lines still land in order while the record is healthy.
    pub(super) fn write_line(&self, line: &str) {
        if !self.recording.load(Ordering::Relaxed) {
            return;
        }
        let Ok(sink) = self.sink.lock() else {
            return;
        };
        let (fs, path) = (sink.0.clone(), sink.1.clone());
        drop(sink);
        let (done, completed) = std::sync::mpsc::channel();
        let line = line.to_string();
        std::thread::spawn(move || {
            let _ = done.send(fs.append(&path, line.as_bytes()));
        });
        match completed.recv_timeout(A_LINE_LANDS_WITHIN) {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                log::warn!("the spawn record could not be written; recording stops: {error}");
                self.recording.store(false, Ordering::Relaxed);
            }
            Err(_) => {
                log::warn!(
                    "the spawn record did not take a line within {A_LINE_LANDS_WITHIN:?}; \
                     recording stops"
                );
                self.recording.store(false, Ordering::Relaxed);
            }
        }
    }
}

impl SpawnObserver for JsonlSpawnRecord {
    fn started(&self, process: &ProcessStart) -> ProcessToken {
        let (token, line) = self.book.started(process);
        self.write_line(&line);
        token
    }

    fn ended(&self, token: ProcessToken, outcome: &ProcessOutcome) {
        if let Some(line) = self.book.ended(token, outcome) {
            self.write_line(&line);
        }
    }
}

/// The `end` line's `outcome` object, matching the outcome the process ended with.
pub(super) fn outcome_json(outcome: &ProcessOutcome) -> Value {
    match outcome {
        ProcessOutcome::Exited { code } => json!({ "exit": code }),
        ProcessOutcome::Signalled { signal } => json!({ "signal": signal }),
        ProcessOutcome::SpawnFailed { error } => json!({ "spawn_failed": error }),
    }
}

/// The lines a record is made of, and the ids that pair a start with its end.
///
/// Shared by the two sinks — the always-open file the daemon writes and the one the cold command
/// line only creates once the run has state of its own — so both word a record the same way. The
/// observer hands a token back and is given it again on the end; a token is an index into what this
/// book remembers about the start, which is what makes the `id`, the pid and the start time meet
/// again on the end line.
pub(super) struct RecordBook {
    /// This process's id, half of every `id`: two writers never collide.
    writer: u32,
    next_token: AtomicU64,
    starts: Mutex<HashMap<u64, StartState>>,
}

/// What an end line needs from the start it pairs with.
struct StartState {
    id: String,
    pid: Option<u32>,
    at_unix_ms: u64,
}

impl RecordBook {
    pub(super) fn new() -> Self {
        Self {
            writer: std::process::id(),
            next_token: AtomicU64::new(0),
            starts: Mutex::new(HashMap::new()),
        }
    }

    /// A `start` line for `process`, and the token its end will arrive with.
    pub(super) fn started(&self, process: &ProcessStart) -> (ProcessToken, String) {
        let token = self.next_token.fetch_add(1, Ordering::Relaxed);
        let id = format!("{}-{token}", self.writer);
        let at_unix_ms = now_unix_ms();
        if let Ok(mut starts) = self.starts.lock() {
            starts.insert(
                token,
                StartState {
                    id: id.clone(),
                    pid: process.pid,
                    at_unix_ms,
                },
            );
        }

        let mut line = json!({
            "v": 1,
            "event": "start",
            "id": id,
            "at_unix_ms": at_unix_ms,
            "origin": origin_of(process.purpose),
            "purpose": process.purpose,
            "program": process.program,
            "argv": redacted(&process.program, &process.args),
            "cwd": process.cwd,
            "pid": process.pid,
            "env_names": process.env_names,
        });
        if let (Some(operation), Some(record)) = (&process.operation, line.as_object_mut()) {
            label_with_operation(record, operation);
        }
        // One line, newline included: a sink writes exactly this, so two writers never run together.
        (ProcessToken(token), format!("{line}\n"))
    }

    /// The `end` line for `token`, or `None` when the start is unknown — a sink handed an end it
    /// never saw begin.
    pub(super) fn ended(&self, token: ProcessToken, outcome: &ProcessOutcome) -> Option<String> {
        let state = self.starts.lock().ok()?.remove(&token.0)?;
        let at_unix_ms = now_unix_ms();

        let mut record = Map::new();
        record.insert("v".to_string(), json!(1));
        record.insert("event".to_string(), json!("end"));
        record.insert("id".to_string(), json!(state.id));
        record.insert("at_unix_ms".to_string(), json!(at_unix_ms));
        // A process that never started has no pid and no duration to report.
        if let Some(pid) = state.pid {
            record.insert("pid".to_string(), json!(pid));
            record.insert(
                "elapsed_ms".to_string(),
                json!(at_unix_ms.saturating_sub(state.at_unix_ms)),
            );
        }
        record.insert("outcome".to_string(), outcome_json(outcome));
        Some(format!("{}\n", Value::Object(record)))
    }
}

/// Add the keys naming the operation a process was started for — `op`, `op_id` and `group`, each
/// only when known — so a start line joins the journal record of that operation. A run-level
/// process's line has none of them, which keeps its shape what it always was.
fn label_with_operation(record: &mut Map<String, Value>, operation: &OperationContext) {
    if let Some(op) = operation.op {
        record.insert("op".to_string(), json!(op));
    }
    if let Some(op_id) = &operation.op_id {
        record.insert("op_id".to_string(), json!(op_id));
    }
    if let Some(group) = &operation.group {
        record.insert("group".to_string(), json!(group));
    }
}

/// Whether a record of this process belongs to the engine or to the language-server layer.
fn origin_of(purpose: &str) -> &'static str {
    if purpose == purpose::LANGUAGE_SERVER {
        "lsp"
    } else {
        "engine"
    }
}

/// Unix milliseconds, without a date-formatting dependency.
fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tddy_lsp::OperationContext;

    /// A `git mv` start, labelled with `operation` when it was started for one.
    fn a_git_move_started_for(operation: Option<OperationContext>) -> ProcessStart {
        ProcessStart {
            purpose: purpose::GIT,
            program: "git".to_string(),
            args: vec!["mv".to_string(), "a.rs".to_string(), "b.rs".to_string()],
            cwd: None,
            env_names: Vec::new(),
            pid: Some(4242),
            operation,
        }
    }

    fn the_start_line_of(process: &ProcessStart) -> Value {
        let (_, line) = RecordBook::new().started(process);
        serde_json::from_str(&line).expect("a start line is JSON")
    }

    /// `#reshape` 9/19: a start line joins the journal record of the operation that started it.
    #[test]
    fn a_start_line_carries_op_op_id_and_group_when_the_process_was_started_for_one() {
        // Given a process started for operation 3, `a1`, of group `g`
        let process = a_git_move_started_for(Some(OperationContext {
            op: Some(3),
            op_id: Some("a1".to_string()),
            group: Some("g".to_string()),
        }));

        // When its start line is written
        let line = the_start_line_of(&process);

        // Then the line names all three
        assert_eq!(
            (&line["op"], &line["op_id"], &line["group"]),
            (&json!(3), &json!("a1"), &json!("g"))
        );
    }

    /// A run-level process's line keeps the shape it always had: no operation keys at all.
    #[test]
    fn a_start_line_for_a_run_level_process_has_no_op_keys() {
        // Given a process started for no operation
        let process = a_git_move_started_for(None);

        // When its start line is written
        let line = the_start_line_of(&process);

        // Then none of the operation keys is present
        let keys: Vec<&String> = line
            .as_object()
            .expect("an object")
            .keys()
            .filter(|key| ["op", "op_id", "group"].contains(&key.as_str()))
            .collect();
        assert!(keys.is_empty(), "{line}");
    }
}
