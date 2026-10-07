//! The cold command line's sink: a record that only appears once the run has state of its own.
//!
//! A cold `restructure` run refuses a tree that did not compile before the plan touched it, and its
//! refusal says **nothing was written** — which is why `.restructure/` is created only after the
//! baseline `cargo check`. A record opened beside the journal would therefore exist before that
//! check, and the refusal would be false. So this sink holds its lines in memory until the run
//! creates `.restructure/`, writes them then, and writes nothing at all for a run that is refused
//! before its state directory exists.
//!
//! The gap it leaves is named in the docs: a run killed during the baseline check — the longest
//! wait before `.restructure/` exists — leaves no record on this path. The daemon's own record is
//! always open, so it has no such gap.

use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use tddy_lsp::{ProcessOutcome, ProcessStart, ProcessToken, SpawnObserver};

use super::jsonl::{open_for_appending, RecordBook};

/// The directory a run creates for its own state, and the record written beside it once it exists.
const STATE_DIRECTORY: &str = ".restructure";
const RECORD_FILE: &str = "spawns.jsonl";

/// A [`SpawnObserver`] that writes the cold command line's record, deferring every line until the
/// run's state directory exists.
pub struct ColdRunSpawnRecord {
    root: PathBuf,
    state: Mutex<State>,
    book: RecordBook,
}

#[derive(Default)]
struct State {
    /// The open record file, once `.restructure/` existed for it to be opened in.
    file: Option<File>,
    /// Lines written before the file could be opened, in the order they were written.
    pending: Vec<String>,
    /// Set once a write failed: recording stops rather than failing the run.
    broken: bool,
}

impl ColdRunSpawnRecord {
    /// A record for a run under `root`, its file created when the run creates `.restructure/`.
    pub fn for_run(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            state: Mutex::new(State::default()),
            book: RecordBook::new(),
        }
    }

    /// The file this record is written to, beside the run's journal.
    fn path(&self) -> PathBuf {
        self.root.join(STATE_DIRECTORY).join(RECORD_FILE)
    }

    /// Append one line, or hold it until `.restructure/` exists.
    fn write_line(&self, line: &str) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if state.broken {
            return;
        }
        if state.file.is_none() && !self.open_now(&mut state) {
            state.pending.push(line.to_string());
            return;
        }
        let State { file, broken, .. } = &mut *state;
        append(file.as_mut(), line, broken);
    }

    /// Open the record file, flushing what was held, if the run's state directory now exists.
    ///
    /// Returns whether a file is open to write to.
    fn open_now(&self, state: &mut State) -> bool {
        if !self.root.join(STATE_DIRECTORY).is_dir() {
            return false;
        }
        match open_for_appending(&self.path()) {
            Ok(mut file) => {
                for held in state.pending.drain(..) {
                    append(Some(&mut file), &held, &mut state.broken);
                }
                state.file = Some(file);
                true
            }
            Err(error) => {
                log::warn!("the spawn record could not be opened; recording stops: {error}");
                state.broken = true;
                false
            }
        }
    }
}

impl Drop for ColdRunSpawnRecord {
    fn drop(&mut self) {
        // A run that created `.restructure/` and then started nothing else still has its held
        // lines written before the process goes.
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if state.file.is_none() && !state.broken {
            self.open_now(&mut state);
        }
    }
}

impl SpawnObserver for ColdRunSpawnRecord {
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

/// Append one line, marking the state broken when the file will not take it.
fn append(file: Option<&mut File>, line: &str, broken: &mut bool) {
    let Some(file) = file else {
        return;
    };
    if let Err(error) = file.write_all(line.as_bytes()) {
        log::warn!("the spawn record could not be written; recording stops: {error}");
        *broken = true;
    }
}
