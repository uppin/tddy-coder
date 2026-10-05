//! The append-only file a record is written to: one JSON object per line, a `start` line when a
//! process exists and an `end` line when it was waited on, never truncated.

use std::io;
use std::path::Path;

use tddy_lsp::{ProcessOutcome, ProcessStart, ProcessToken, SpawnObserver};

/// A [`SpawnObserver`] that appends what it is told to a file, applying the redaction policy
/// before a byte is written.
pub struct JsonlSpawnRecord {
    _private: (),
}

impl JsonlSpawnRecord {
    /// Open `path` for appending, creating it when it is absent. Existing lines are never
    /// touched, so a second run appends after the first.
    ///
    /// TODO(spawn-record): implement. Until then a sink is refused rather than handed back
    /// empty, so nothing believes it is recording.
    pub fn open(path: &Path) -> io::Result<Self> {
        Err(io::Error::other(format!(
            "TODO(spawn-record): the JSONL spawn record is not implemented, so {} was not opened",
            path.display()
        )))
    }
}

impl SpawnObserver for JsonlSpawnRecord {
    fn started(&self, _process: &ProcessStart) -> ProcessToken {
        unreachable!("TODO(spawn-record): a JsonlSpawnRecord cannot be opened yet")
    }

    fn ended(&self, _token: ProcessToken, _outcome: &ProcessOutcome) {
        unreachable!("TODO(spawn-record): a JsonlSpawnRecord cannot be opened yet")
    }
}
