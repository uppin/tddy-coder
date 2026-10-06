//! The append-only file a record is written to: one JSON object per line, a `start` line when a
//! process exists and an `end` line when it was waited on, never truncated.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Map, Value};
use tddy_lsp::{ProcessOutcome, ProcessStart, ProcessToken, SpawnObserver};

use super::purpose;
use super::redact::redacted;

/// A [`SpawnObserver`] that appends what it is told to a file, applying the redaction policy
/// before a byte is written.
pub struct JsonlSpawnRecord {
    file: Mutex<File>,
    /// Cleared once a write has failed, so a record that cannot be written stops being attempted
    /// rather than failing the run it is observing.
    recording: AtomicBool,
    book: RecordBook,
}

impl JsonlSpawnRecord {
    /// Open `path` for appending, creating it when it is absent. Existing lines are never
    /// touched, so a second run appends after the first.
    pub fn open(path: &Path) -> io::Result<Self> {
        Ok(Self {
            file: Mutex::new(open_for_appending(path)?),
            recording: AtomicBool::new(true),
            book: RecordBook::new(),
        })
    }

    /// Append one line, and stop recording for good if the file will not take it.
    pub(super) fn write_line(&self, line: &str) {
        if !self.recording.load(Ordering::Relaxed) {
            return;
        }
        let Ok(mut file) = self.file.lock() else {
            return;
        };
        if let Err(error) = file.write_all(line.as_bytes()) {
            log::warn!("the spawn record could not be written; recording stops: {error}");
            self.recording.store(false, Ordering::Relaxed);
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

/// Open `path` with `O_APPEND` and `O_CREAT`, so a line is added and existing lines are kept.
pub(super) fn open_for_appending(path: &Path) -> io::Result<File> {
    OpenOptions::new().create(true).append(true).open(path)
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

        let line = json!({
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
