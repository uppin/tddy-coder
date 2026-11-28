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

use std::path::PathBuf;
use std::sync::Mutex;

use tddy_lsp::{ProcessOutcome, ProcessStart, ProcessToken, SpawnObserver};

use super::jsonl::RecordBook;

/// The directory a run creates for its own state, and the record written beside it once it exists.
const STATE_DIRECTORY: &str = ".restructure";
const RECORD_FILE: &str = "spawns.jsonl";

/// A [`SpawnObserver`] that writes the cold command line's record, deferring every line until the
/// run's state directory exists.
pub struct ColdRunSpawnRecord {
    root: PathBuf,
    fs: std::sync::Arc<dyn tddy_fs::Fs>,
    state: Mutex<State>,
    book: RecordBook,
}

#[derive(Default)]
struct State {
    /// Whether the record is open for appending, once `.restructure/` existed for it.
    open: bool,
    /// Lines written before the file could be opened, in the order they were written.
    pending: Vec<String>,
    /// Set once a write failed: recording stops rather than failing the run.
    broken: bool,
}

impl ColdRunSpawnRecord {
    /// A record for a run under `root`, its file created when the run creates `.restructure/`.
    pub fn for_run(root: impl Into<PathBuf>) -> Self {
        Self::for_run_on(root, std::sync::Arc::new(tddy_fs::DiskFs))
    }

    /// [`ColdRunSpawnRecord::for_run`] through the filesystem port.
    pub fn for_run_on(root: impl Into<PathBuf>, fs: std::sync::Arc<dyn tddy_fs::Fs>) -> Self {
        Self {
            root: root.into(),
            fs,
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
        if !state.open && !self.open_now(&mut state) {
            state.pending.push(line.to_string());
            return;
        }
        append(&*self.fs, &self.path(), line, &mut state.broken);
    }

    /// Open the record file, flushing what was held, if the run's state directory now exists.
    ///
    /// Returns whether a file is open to write to.
    fn open_now(&self, state: &mut State) -> bool {
        if self
            .fs
            .metadata_follow(&self.root.join(STATE_DIRECTORY))
            .map(|m| m.kind != tddy_fs::FsKind::Dir)
            .unwrap_or(true)
        {
            return false;
        }
        match self.fs.append(&self.path(), b"") {
            Ok(()) => {
                let held: Vec<String> = state.pending.drain(..).collect();
                for line in &held {
                    append(&*self.fs, &self.path(), line, &mut state.broken);
                }
                state.open = true;
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
        if !state.open && !state.broken {
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
fn append(fs: &dyn tddy_fs::Fs, path: &std::path::Path, line: &str, broken: &mut bool) {
    if let Err(error) = fs.append(path, line.as_bytes()) {
        log::warn!("the spawn record could not be written; recording stops: {error}");
        *broken = true;
    }
}
