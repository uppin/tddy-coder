//! What the stub was asked, recorded for tests that observe the ACP client.
//!
//! With `TDDY_ACP_STUB_RECORD=<path>` every received `new_session`, `load_session`,
//! `set_session_model`, `cancel` and every permission outcome is appended to `<path>` as one JSON
//! line, after a first `started` line carrying the stub's pid. Without the variable nothing is
//! written. A JSONL file outlives the child, so a test can read it after the client dropped it.

use std::io::Write as _;
use std::path::PathBuf;

/// Appends JSON lines to the record file named by `TDDY_ACP_STUB_RECORD`.
#[derive(Debug, Clone, Default)]
pub struct Recorder {
    path: Option<PathBuf>,
}

impl Recorder {
    /// Read `TDDY_ACP_STUB_RECORD` and record the `started` line.
    pub fn from_env() -> Self {
        let recorder = Self {
            path: std::env::var_os("TDDY_ACP_STUB_RECORD").map(PathBuf::from),
        };
        recorder.record(serde_json::json!({ "event": "started", "pid": std::process::id() }));
        recorder
    }

    pub fn record(&self, line: serde_json::Value) {
        let Some(path) = &self.path else { return };
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap_or_else(|e| panic!("TDDY_ACP_STUB_RECORD {}: {e}", path.display()));
        writeln!(file, "{line}").unwrap_or_else(|e| panic!("record {}: {e}", path.display()));
    }
}
