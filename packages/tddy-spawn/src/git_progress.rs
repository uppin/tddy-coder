//! Git's clone progress, as `git clone --progress` writes it to stderr: one line per redraw,
//! separated by `\r` while a stage runs and `\n` when it ends.
//!
//! ```text
//! remote: Counting objects: 100% (1200/1200), done.
//! Receiving objects:  45% (36555/81234), 38.50 MiB | 4.25 MiB/s
//! Resolving deltas: 100% (51234/51234), done.
//! ```
//!
//! [`parse`] turns one line into a [`GitProgress`]; [`ProgressThrottle`] keeps the redraws that are
//! worth sending across a pipe and a LiveKit relay.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// One parsed progress line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitProgress {
    /// Git's words for the stage, without any `remote: ` prefix.
    pub stage: String,
    pub percent: u32,
    pub objects_done: u64,
    pub objects_total: u64,
    /// Bytes received so far; 0 when the line reports none.
    pub bytes_received: u64,
    /// Throughput; 0 when the line reports none.
    pub bytes_per_second: u64,
    /// The line as git wrote it, trimmed.
    pub line: String,
}

/// Parse one progress line; `None` for a line that carries no `NN% (done/total)` progress, such as
/// `Cloning into '…'...`.
pub fn parse(line: &str) -> Option<GitProgress> {
    // TODO(clone-progress): implement — strip "remote: ", "<stage>: NN% (a/b)", optional ", X.YY UNIT | Z.WW UNIT/s"
    let _ = line;
    None
}

/// The shortest gap between two forwarded updates of the same stage.
pub const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);

/// Decides which progress lines are forwarded: the first, every stage change, every 100%, and
/// otherwise at most one per [`PROGRESS_INTERVAL`].
#[derive(Debug, Default)]
pub struct ProgressThrottle {
    last: Option<(String, Instant)>,
}

impl ProgressThrottle {
    pub fn new() -> ProgressThrottle {
        ProgressThrottle::default()
    }

    /// Whether `progress`, seen at `now`, should be forwarded. Records it when it is.
    pub fn admit(&mut self, progress: &GitProgress, now: Instant) -> bool {
        // TODO(clone-progress): implement
        let _ = (progress, now, &self.last);
        false
    }
}
